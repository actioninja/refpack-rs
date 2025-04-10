pub(crate) mod hash_table;

use std::cmp::min;
use std::collections::HashMap;

use crate::data::compression::match_length::{
    byte_offset_matches,
    match_length,
    match_length_except,
    match_length_or,
};
use crate::data::control::{LONG_LENGTH_MAX, LONG_OFFSET_MAX};

pub fn prefix(input_buf: &[u8]) -> [u8; 3] {
    let buf: &[u8] = &input_buf[..3];
    [buf[0], buf[1], buf[2]]
}

#[derive(Copy, Clone, Debug)]
struct Match {
    position: u32,
    bad_position: u32,
    length: u16,
    skip_length: u16,
}

impl Default for Match {
    fn default() -> Self {
        Self {
            position: u32::MAX,
            bad_position: u32::MAX,
            length: 0,
            skip_length: 0,
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct HashChainLink<const N: usize> {
    prev: [Match; N],
    short_skip_pos: u32,
}

pub struct PrefixSearcher<'a, const N: usize> {
    buffer: &'a [u8],
    prev: Vec<HashChainLink<N>>,
}

impl<'a, const N: usize> PrefixSearcher<'a, N> {
    fn search_break<F: FnMut(usize, usize)>(
        buffer: &[u8],
        prev: &[HashChainLink<N>],
        pos: usize,
        mut from: usize,
        mut prev_matched_len: u16,
        mut found_fn: F,
    ) -> (usize, usize) {
        let mut prev_from = from;
        let mut prev_prev_match_len = prev_matched_len;

        'outer: loop {
            if prev_matched_len >= LONG_LENGTH_MAX {
                break;
            }
            for n in 0..N {
                let n_match_length = prev[from].prev[n].length;
                if n_match_length == 0 || n_match_length > prev_matched_len {
                    break 'outer;
                } else if n_match_length == prev_matched_len {
                    let match_pos = prev[from].prev[n].position;
                    let match_len = match_length(
                        buffer,
                        pos,
                        match_pos as usize,
                        LONG_LENGTH_MAX as usize,
                        prev_matched_len as usize,
                    );
                    if match_len > prev_matched_len as usize {
                        found_fn(match_pos as usize, match_len);

                        prev_from = from;
                        prev_prev_match_len = prev_matched_len;

                        from = match_pos as usize;
                        prev_matched_len = match_len as u16;
                        continue 'outer;
                    }
                    break 'outer;
                } else if n_match_length > prev_matched_len {
                    break 'outer;
                }
            }
            break;
        }

        (prev_from, prev_prev_match_len as usize)
    }

    fn search_from_offset<F: Fn(u32, u16) -> u16>(
        prev: &[HashChainLink<N>],
        pos: usize,
        min_length: usize,
        mut from: usize,
        mut prev_matched_len: u16,
        match_fn: F,
    ) -> Option<(usize, usize)> {
        let long_offset_limit = pos.saturating_sub(LONG_OFFSET_MAX as usize);
        let mut level = 0;

        'outer: loop {
            if from < long_offset_limit {
                return None;
            }

            let p = &prev[from];

            loop {
                let pl = p.prev[level];

                if pl.length == 0 {
                    if level == 0 {
                        return None;
                    }
                    level -= 1;
                } else if prev_matched_len < pl.length {
                    if level == 0 {
                        // from = pl.position as usize;
                        from = p.short_skip_pos as usize;
                        continue 'outer;
                    } else if prev_matched_len > p.prev[level - 1].skip_length {
                        from = pl.position as usize;
                        continue 'outer;
                    }
                    level -= 1;
                } else if prev_matched_len == pl.length {
                    if (pl.position as usize) < long_offset_limit {
                        return None;
                    }

                    let match_len = match_fn(pl.position, prev_matched_len);
                    if match_len as usize > min_length {
                        return Some((pl.position as usize, match_len as usize));
                    }

                    if match_len as usize == pl.length as usize {
                        if pl.bad_position == u32::MAX
                            || (pl.bad_position as usize) < long_offset_limit
                        {
                            return None;
                        }

                        let match_len = match_fn(pl.bad_position, prev_matched_len);
                        if match_len as usize > min_length {
                            return Some((pl.bad_position as usize, match_len as usize));
                        }

                        prev_matched_len = match_len;
                        from = pl.bad_position as usize;
                        continue 'outer;
                    }
                    prev_matched_len = match_len;
                    from = pl.position as usize;
                    continue 'outer;
                } else if prev_matched_len <= pl.skip_length {
                    prev_matched_len = pl.length;
                    from = pl.position as usize;
                    continue 'outer;
                } else {
                    // prev_matched_len > pl.skip_match_length
                    if level == N - 1 {
                        prev_matched_len = pl.length;
                        from = pl.position as usize;
                        continue 'outer;
                    } else if p.prev[level + 1].length == 0 {
                        return None;
                    }
                    level += 1;
                }
            }
        }
    }

    pub fn build(buffer: &'a [u8]) -> Self {
        let mut head = HashMap::with_capacity(256);

        let mut prev = buffer
            .windows(3)
            .enumerate()
            .map(|(i, window)| {
                let p = prefix(window);

                let prev_pos = head.get(&p).copied();
                let mut short_skip_pos = u32::MAX;
                let new = prev_pos
                    .map(|(prev_pos, prev_len, prev_short_skip)| {
                        let match_length =
                            match_length(buffer, i, prev_pos, LONG_LENGTH_MAX as usize, 3) as u16;

                        short_skip_pos = if match_length >= prev_len {
                            prev_pos as u32
                        } else {
                            prev_short_skip
                        };

                        Match {
                            position: prev_pos as u32,
                            bad_position: u32::MAX,
                            length: match_length,
                            skip_length: 0,
                        }
                    })
                    .unwrap_or_default();

                head.insert(p, (i, new.length, short_skip_pos));

                let mut link = HashChainLink {
                    prev: [Match::default(); N],
                    short_skip_pos,
                };
                link.prev[0] = new;

                link
            })
            .collect::<Vec<_>>();

        for i in 0..prev.len() {
            if prev[i].prev[0].length > 0 {
                for o in 0..N {
                    let n = o + 1;

                    let po = prev[i].prev[o];

                    let (spos, slen) = Self::search_break(
                        buffer,
                        &prev,
                        i,
                        po.position as usize,
                        po.length,
                        |_, _| {},
                    );
                    prev[i].prev[o].skip_length = slen as u16;

                    if slen < min(LONG_LENGTH_MAX as usize, buffer.len() - spos - 1) {
                        if n < N {
                            if let Some((fpos, flen)) = Self::search_from_offset(
                                &prev,
                                i,
                                po.length as usize,
                                po.position as usize,
                                po.length,
                                |pos, skip| {
                                    match_length_or(
                                        buffer,
                                        i,
                                        pos as usize,
                                        po.position as usize,
                                        po.length as usize,
                                        skip as usize,
                                    )
                                },
                            ) {
                                if byte_offset_matches(buffer, i, fpos, po.length as usize) {
                                    if let Some((bpos, _blen)) = Self::search_from_offset(
                                        &prev,
                                        i,
                                        po.length as usize,
                                        fpos,
                                        po.length,
                                        |pos, skip| {
                                            match_length_except(
                                                buffer,
                                                i,
                                                po.position as usize,
                                                pos as usize,
                                                po.length as usize,
                                                skip as usize,
                                            )
                                        },
                                    ) {
                                        prev[i].prev[o].bad_position = bpos as u32;
                                    }
                                    if flen > slen {
                                        // found the next good position, search for the bad position
                                        prev[i].prev[n].position = fpos as u32;
                                        prev[i].prev[n].length = flen as u16;
                                        continue;
                                    }
                                } else {
                                    prev[i].prev[o].bad_position = fpos as u32;
                                }

                                // found the bad position, search for the good position
                                if let Some((pos, len)) = Self::search_from_offset(
                                    &prev,
                                    i,
                                    slen,
                                    min(fpos, spos),
                                    slen as u16,
                                    |pos, skip| {
                                        match_length(
                                            buffer,
                                            i,
                                            pos as usize,
                                            LONG_LENGTH_MAX as usize,
                                            skip as usize,
                                        ) as u16
                                    },
                                ) {
                                    prev[i].prev[n].position = pos as u32;
                                    prev[i].prev[n].length = len as u16;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        } else if let Some((bpos, _blen)) = Self::search_from_offset(
                            &prev,
                            i,
                            po.length as usize,
                            po.position as usize,
                            po.length,
                            |pos, skip| {
                                match_length_except(
                                    buffer,
                                    i,
                                    po.position as usize,
                                    pos as usize,
                                    po.length as usize,
                                    skip as usize,
                                )
                            },
                        ) {
                            prev[i].prev[o].bad_position = bpos as u32;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        Self { buffer, prev }
    }

    pub fn search<F: FnMut(usize, usize, usize)>(&mut self, pos: usize, mut found_fn: F) {
        // matches have to be 3 bytes minimum, so skip match lengths 0 to 2
        let start_match = self.prev[pos];

        let mut max_matched = 2;

        for cur_match in &start_match.prev[..N - 1] {
            let match_pos = cur_match.position as usize;
            let match_len = cur_match.length as usize;

            if match_len == 0 {
                return;
            }

            found_fn(match_pos, max_matched + 1, match_len + 1);

            if match_len == LONG_LENGTH_MAX as usize {
                return;
            }

            max_matched = match_len;

            Self::search_break(
                self.buffer,
                &self.prev,
                pos,
                match_pos,
                match_len as u16,
                |pos, len| {
                    if len <= cur_match.skip_length as usize {
                        found_fn(pos, max_matched + 1, len + 1);
                        max_matched = len;
                    }
                },
            );
        }

        if start_match.prev[N - 1].length > 0 {
            let mut cur_match = Some((
                start_match.prev[N - 1].position as usize,
                start_match.prev[N - 1].length as usize,
            ));

            while let Some((match_pos, len)) = cur_match {
                found_fn(match_pos, max_matched + 1, len + 1);
                max_matched = len;
                let from = match_pos;

                if len == LONG_LENGTH_MAX as usize {
                    return;
                }

                cur_match = Self::search_from_offset(
                    &self.prev,
                    pos,
                    max_matched,
                    from,
                    max_matched as u16,
                    |test_pos, skip| {
                        match_length(
                            self.buffer,
                            pos,
                            test_pos as usize,
                            LONG_LENGTH_MAX as usize,
                            skip as usize,
                        ) as u16
                    },
                );
            }
        }
    }
}
