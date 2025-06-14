////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::cmp::min;

use crate::data::compression::match_length::{
    byte_offset_matches,
    match_length,
    match_length_except,
    match_length_or,
};
use crate::data::compression::prefix_search;
use crate::data::compression::prefix_search::hash_table::PrefixTable;
use crate::data::compression::prefix_search::{PrefixSearcher, HASH_CHAIN_BUFFER_SIZE};
use crate::data::control::{LONG_LENGTH_MAX, LONG_OFFSET_MAX};

/// A match between the current position and the contained position
///
/// consider the following bytes [0, 0, 0, 0, 1, 0, 0, 0, 0, 0]
/// which will have the following match positions, lengths, and skip lengths:
/// pos: (match, length, skip) where "n" represents none
/// 0: (n, n, n)
/// 1: (0, 3, 3)
/// 2-4: (n, n, n)
/// 5: (1, 3, 4)
/// 6: (5, 4, 4)
/// 7: (6, 3, 3)
#[derive(Copy, Clone, Debug)]
struct Match {
    /// the position of the matching sequence of bytes or u32::MAX 
    position: u32,
    /// the next position that has exactly `length` matching bytes with the current position
    /// and does not continue with the same byte as the match at `position`
    /// 
    /// that is, the byte at `position + length` != `bad_position + length`
    bad_position: u32,
    /// the number of bytes that match between this position and the match position
    length: u16,
    /// when following this chain by repeatedly following `position`,
    /// this is the longest non-decreasing match length with the current position
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
}

impl<const N: usize> Default for HashChainLink<N> {
    fn default() -> Self {
        Self {
            prev: [Match::default(); N],
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
            data: vec![HashChainLink::default(); min(bytes, HASH_CHAIN_BUFFER_SIZE)],
            #[cfg(debug_assertions)]
            last_index: 0,
        }
    }

    fn at(&self, i: usize) -> &HashChainLink<N> {
        #[cfg(debug_assertions)]
        debug_assert!(self.last_index - i <= LONG_OFFSET_MAX as usize);
        &self.data[i % HASH_CHAIN_BUFFER_SIZE]
    }

    fn at_mut(&mut self, i: usize) -> &mut HashChainLink<N> {
        #[cfg(debug_assertions)]
        {
            self.last_index = std::cmp::max(self.last_index, i);
            debug_assert!(self.last_index - i <= LONG_OFFSET_MAX as usize);
        }
        &mut self.data[i % HASH_CHAIN_BUFFER_SIZE]
    }
}

/// This is an advanced version of the HashChain prefix searcher
/// 
/// In the case of N=1 this is essentially equivalent to a standard hash chain;
/// every link in the hash chain points to the most recent previous occurrence of the prefix at that position.
///
/// The multi level hash chain extends the normal hash chain by implementing a structure similar to a skip list:
/// levels of the chain that are above the lowest level will only refer to matches
/// that match more bytes than the `skip_match_length` of the layer below it.
///
/// This structure produces intervals that tend to be spaced at distances that grow exponentially for each layer,
/// meaning search actions through the graph take amortized logarithmic time.
/// Certain degenerate cases can still lead to search times that appear linear,
/// but a detailed algorithmic complexity analysis has not been done to identify these cases.
pub(crate) struct MultiLevelPrefixSearcher<'a, const N: usize> {
    buffer: &'a [u8],
    /// the latest found position of any prefix
    head: PrefixTable,
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
                        if from != prev_from {
                            found_fn(from, prev_matched_len as usize);
                        }

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

            let p = prev.at(from);

            loop {
                let pl = p.prev[level];

                if pl.length == 0 {
                    if level == 0 {
                        return None;
                    }
                    level -= 1;
                } else if prev_matched_len < pl.length {
                    if level == 0 || prev_matched_len > p.prev[level - 1].skip_length {
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
        let mut head = PrefixTable::new(buffer.len());

        head.insert(prefix_search::prefix(buffer), 0);

        let prev = MultiLevelHashChain::new(buffer.len());

        Self { buffer, head, prev }
    }

    fn search<F: FnMut(usize, usize, usize)>(&mut self, search_position: usize, mut found_fn: F) {
        let p = prefix_search::prefix(&self.buffer[search_position..]);

        *self.prev.at_mut(search_position) = HashChainLink::default();

        // matches have to be 3 bytes minimum, so skip match lengths 0 to 2

        let prev_pos = self.head.insert(p, search_position as u32);
        if let Some(prev_pos) = prev_pos {
            if search_position as u32 - prev_pos <= LONG_OFFSET_MAX {
                let match_length = match_length(
                    self.buffer,
                    search_position,
                    prev_pos as usize,
                    LONG_LENGTH_MAX as usize,
                    3,
                ) as u16;

                self.prev.at_mut(search_position).prev[0] = Match {
                    position: prev_pos,
                    bad_position: u32::MAX,
                    length: match_length,
                    skip_length: 0,
                };

                let mut max_matched = 2;

                for origin in 0..N {
                    let next = origin + 1;

                    let po = self.prev.at(search_position).prev[origin];

                    found_fn(
                        po.position as usize,
                        max_matched + 1,
                        po.length as usize + 1,
                    );
                    max_matched = po.length as usize;
                    // latest_pos = po.position as usize;

                    let (spos, slen) = Self::search_break(
                        self.buffer,
                        &self.prev,
                        search_position,
                        po.position as usize,
                        po.length,
                        |position, length| {
                            found_fn(position, max_matched + 1, length + 1);
                            max_matched = length;
                        },
                    );
                    self.prev.at_mut(search_position).prev[origin].skip_length = slen as u16;

                    if slen < min(LONG_LENGTH_MAX as usize, self.buffer.len() - spos - 1) {
                        if next < N {
                            if let Some((fpos, flen)) = Self::search_from_offset(
                                &self.prev,
                                search_position,
                                po.length as usize,
                                po.position as usize,
                                po.length,
                                |pos, skip| {
                                    match_length_or(
                                        self.buffer,
                                        search_position,
                                        pos as usize,
                                        po.position as usize,
                                        po.length as usize,
                                        skip as usize,
                                    )
                                },
                            ) {
                                if byte_offset_matches(
                                    self.buffer,
                                    search_position,
                                    fpos,
                                    po.length as usize,
                                ) {
                                    if let Some((bpos, _blen)) = Self::search_from_offset(
                                        &self.prev,
                                        search_position,
                                        po.length as usize,
                                        fpos,
                                        po.length,
                                        |pos, skip| {
                                            match_length_except(
                                                self.buffer,
                                                search_position,
                                                po.position as usize,
                                                pos as usize,
                                                po.length as usize,
                                                skip as usize,
                                            )
                                        },
                                    ) {
                                        self.prev.at_mut(search_position).prev[origin]
                                            .bad_position = bpos as u32;
                                    }
                                    if flen > slen {
                                        // found the next good position, search for the bad position
                                        self.prev.at_mut(search_position).prev[next].position =
                                            fpos as u32;
                                        self.prev.at_mut(search_position).prev[next].length =
                                            flen as u16;
                                        continue;
                                    }
                                } else {
                                    self.prev.at_mut(search_position).prev[origin].bad_position =
                                        fpos as u32;
                                }

                                // found the bad position, search for the good position
                                if let Some((pos, len)) = Self::search_from_offset(
                                    &self.prev,
                                    search_position,
                                    slen,
                                    min(fpos, spos),
                                    slen as u16,
                                    |pos, skip| {
                                        crate::data::compression::match_length::match_length(
                                            self.buffer,
                                            search_position,
                                            pos as usize,
                                            LONG_LENGTH_MAX as usize,
                                            skip as usize,
                                        ) as u16
                                    },
                                ) {
                                    self.prev.at_mut(search_position).prev[next].position =
                                        pos as u32;
                                    self.prev.at_mut(search_position).prev[next].length =
                                        len as u16;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        } else {
                            if let Some((bpos, _blen)) = Self::search_from_offset(
                                &self.prev,
                                search_position,
                                po.length as usize,
                                po.position as usize,
                                po.length,
                                |pos, skip| {
                                    match_length_except(
                                        self.buffer,
                                        search_position,
                                        po.position as usize,
                                        pos as usize,
                                        po.length as usize,
                                        skip as usize,
                                    )
                                },
                            ) {
                                self.prev.at_mut(search_position).prev[origin].bad_position =
                                    bpos as u32;
                            }

                            // last loop, find the rest of the matches for the search function
                            let mut cur_pos = spos;

                            while let Some((match_pos, len)) = Self::search_from_offset(
                                &self.prev,
                                search_position,
                                max_matched,
                                cur_pos,
                                max_matched as u16,
                                |test_pos, skip| {
                                    crate::data::compression::match_length::match_length(
                                        self.buffer,
                                        search_position,
                                        test_pos as usize,
                                        LONG_LENGTH_MAX as usize,
                                        skip as usize,
                                    ) as u16
                                },
                            ) {
                                found_fn(match_pos, max_matched + 1, len + 1);
                                max_matched = len;
                                cur_pos = match_pos;

                                if len == LONG_LENGTH_MAX as usize {
                                    return;
                                }
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }
}
