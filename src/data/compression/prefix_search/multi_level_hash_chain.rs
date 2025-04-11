use std::cmp::min;
use std::collections::HashMap;

use crate::data::compression::match_length::{
    byte_offset_matches,
    match_length,
    match_length_except,
    match_length_or,
};
use crate::data::compression::prefix_search;
use crate::data::compression::prefix_search::{PrefixSearcher, HASH_CHAIN_MODULO};
use crate::data::control::{LONG_LENGTH_MAX, LONG_OFFSET_MAX};

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

impl<const N: usize> Default for HashChainLink<N> {
    fn default() -> Self {
        Self {
            prev: [Match::default(); N],
            short_skip_pos: u32::MAX,
        }
    }
}

struct MultiLevelHashChain<const N: usize> {
    data: Vec<HashChainLink<N>>,
    #[cfg(debug_assertions)]
    last_index: usize,
}

impl<const N: usize> MultiLevelHashChain<N> {
    fn new(bytes: usize) -> Self {
        Self {
            data: vec![HashChainLink::default(); min(bytes, HASH_CHAIN_MODULO)],
            #[cfg(debug_assertions)]
            last_index: 0,
        }
    }

    fn at(&self, i: usize) -> &HashChainLink<N> {
        #[cfg(debug_assertions)]
        debug_assert!(self.last_index - i <= LONG_OFFSET_MAX as usize);
        &self.data[i % HASH_CHAIN_MODULO]
    }

    fn at_mut(&mut self, i: usize) -> &mut HashChainLink<N> {
        #[cfg(debug_assertions)]
        {
            self.last_index = std::cmp::max(self.last_index, i);
            debug_assert!(self.last_index - i <= LONG_OFFSET_MAX as usize);
        }
        &mut self.data[i % HASH_CHAIN_MODULO]
    }
}

pub(crate) struct MultiLevelPrefixSearcher<'a, const N: usize> {
    buffer: &'a [u8],
    head: HashMap<[u8; 3], (usize, u16, u32)>,
    prev: MultiLevelHashChain<N>,
}

impl<const N: usize> MultiLevelPrefixSearcher<'_, N> {
    fn search_break<F: FnMut(usize, usize)>(
        buffer: &[u8],
        prev: &MultiLevelHashChain<N>,
        pos: usize,
        mut from: usize,
        mut prev_matched_len: u16,
        mut found_fn: F,
    ) -> (usize, usize) {
        let long_offset_limit = pos.saturating_sub(LONG_OFFSET_MAX as usize);
        let mut prev_from = from;
        let mut prev_prev_match_len = prev_matched_len;

        'outer: loop {
            if prev_matched_len >= LONG_LENGTH_MAX || from < long_offset_limit {
                break;
            }
            for n in 0..N {
                let n_match_length = prev.at(from).prev[n].length;
                if n_match_length == 0 || n_match_length > prev_matched_len {
                    break 'outer;
                } else if n_match_length == prev_matched_len {
                    let match_pos = prev.at(from).prev[n].position;
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
        prev: &MultiLevelHashChain<N>,
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

            let p = &prev.at(from);

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
}

impl<'a, const N: usize> PrefixSearcher<'a> for MultiLevelPrefixSearcher<'a, N> {
    fn build(buffer: &'a [u8]) -> Self {
        let mut head = HashMap::with_capacity(256);

        head.insert(prefix_search::prefix(buffer), (0, 0, u32::MAX));

        let prev = MultiLevelHashChain::new(buffer.len());

        Self { buffer, head, prev }
    }

    fn search<F: FnMut(usize, usize, usize)>(&mut self, pos: usize, mut found_fn: F) {
        let i = pos;

        let p = prefix_search::prefix(&self.buffer[i..]);

        let prev_pos = self.head.get(&p).copied();
        let mut short_skip_pos = u32::MAX;
        let new = prev_pos
            .filter(|(prev_pos, ..)| pos - prev_pos <= LONG_OFFSET_MAX as usize)
            .map(|(prev_pos, prev_len, prev_short_skip)| {
                let match_length =
                    match_length(self.buffer, i, prev_pos, LONG_LENGTH_MAX as usize, 3) as u16;

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

        self.head.insert(p, (i, new.length, short_skip_pos));

        self.prev.at_mut(i).prev = [Match::default(); N];
        self.prev.at_mut(i).short_skip_pos = short_skip_pos;
        self.prev.at_mut(i).prev[0] = new;


        if self.prev.at(i).prev[0].length > 0 {
            for origin in 0..N {
                let next = origin + 1;

                let po = self.prev.at(i).prev[origin];

                let (spos, slen) = Self::search_break(
                    self.buffer,
                    &self.prev,
                    i,
                    po.position as usize,
                    po.length,
                    |_, _| {},
                );
                self.prev.at_mut(i).prev[origin].skip_length = slen as u16;

                if slen < min(LONG_LENGTH_MAX as usize, self.buffer.len() - spos - 1) {
                    if next < N {
                        if let Some((fpos, flen)) = Self::search_from_offset(
                            &self.prev,
                            i,
                            po.length as usize,
                            po.position as usize,
                            po.length,
                            |pos, skip| {
                                match_length_or(
                                    self.buffer,
                                    i,
                                    pos as usize,
                                    po.position as usize,
                                    po.length as usize,
                                    skip as usize,
                                )
                            },
                        ) {
                            if byte_offset_matches(self.buffer, i, fpos, po.length as usize) {
                                if let Some((bpos, _blen)) = Self::search_from_offset(
                                    &self.prev,
                                    i,
                                    po.length as usize,
                                    fpos,
                                    po.length,
                                    |pos, skip| {
                                        match_length_except(
                                            self.buffer,
                                            i,
                                            po.position as usize,
                                            pos as usize,
                                            po.length as usize,
                                            skip as usize,
                                        )
                                    },
                                ) {
                                    self.prev.at_mut(i).prev[origin].bad_position = bpos as u32;
                                }
                                if flen > slen {
                                    // found the next good position, search for the bad position
                                    self.prev.at_mut(i).prev[next].position = fpos as u32;
                                    self.prev.at_mut(i).prev[next].length = flen as u16;
                                    continue;
                                }
                            } else {
                                self.prev.at_mut(i).prev[origin].bad_position = fpos as u32;
                            }

                            // found the bad position, search for the good position
                            if let Some((pos, len)) = Self::search_from_offset(
                                &self.prev,
                                i,
                                slen,
                                min(fpos, spos),
                                slen as u16,
                                |pos, skip| {
                                    match_length(
                                        self.buffer,
                                        i,
                                        pos as usize,
                                        LONG_LENGTH_MAX as usize,
                                        skip as usize,
                                    ) as u16
                                },
                            ) {
                                self.prev.at_mut(i).prev[next].position = pos as u32;
                                self.prev.at_mut(i).prev[next].length = len as u16;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    } else if let Some((bpos, _blen)) = Self::search_from_offset(
                        &self.prev,
                        i,
                        po.length as usize,
                        po.position as usize,
                        po.length,
                        |pos, skip| {
                            match_length_except(
                                self.buffer,
                                i,
                                po.position as usize,
                                pos as usize,
                                po.length as usize,
                                skip as usize,
                            )
                        },
                    ) {
                        self.prev.at_mut(i).prev[origin].bad_position = bpos as u32;
                    }
                } else {
                    break;
                }
            }
        }

        let start_match = self.prev.at(pos);

        // matches have to be 3 bytes minimum, so skip match lengths 0 to 2
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
