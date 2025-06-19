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
    /// search for the longest increasing match with `pos`
    ///
    /// Will return all matches in the hash chain that have an increasingly large match length with
    /// the position `pos`, starting the search from position `from` with the knowledge that `from` matches `pos`
    /// with a length of `from_matched_len`
    ///
    /// The longest match of this chain is not returned, since it is often a good candidate for storing in
    /// higher levels of the hash chain.
    ///
    /// Returns the position to continue searching from to find more matches.
    fn search_break<F: FnMut(usize, usize)>(
        buffer: &[u8],
        prev: &MultiLevelHashChain<N>,
        pos: usize,
        mut from: usize,
        mut from_matched_len: u16,
        mut found_fn: F,
    ) -> (usize, usize) {
        // position past which we know that no match can be encoded
        let long_offset_limit = pos.saturating_sub(LONG_OFFSET_MAX as usize);
        let mut prev_from = from;
        let mut prev_from_matched_len = from_matched_len;

        while from_matched_len < LONG_LENGTH_MAX && from >= long_offset_limit {
            // find the level that has a match length that is equal to the match length with the `from` position
            // having an equal match length means that the match potentially has more bytes in common with `pos`
            // since the byte past the match length differs from the byte at the `from` position
            let found = (0..N)
                .find_map(|level| {
                    let level_match_length = prev.at(from).prev[level].length;
                    if level_match_length == from_matched_len {
                        let match_pos = prev.at(from).prev[level].position;
                        let match_len = match_length(
                            buffer,
                            pos,
                            match_pos as usize,
                            LONG_LENGTH_MAX as usize,
                            from_matched_len as usize,
                        );
                        Some(Some((match_pos, match_len)))
                    } else if level_match_length == 0 || level_match_length > from_matched_len {
                        // the match lengths in the chain always increase in length with levels so if the match length of
                        // the current level is greater than the current match length there cannot be an equal length match
                        Some(None)
                    } else {
                        None
                    }
                })
                .flatten();

            if let Some((match_pos, match_len)) = found {
                if match_len > from_matched_len as usize {
                    // optimization: do not skip the longest match in this chain
                    // since these are often good candidates for nice skip intervals
                    if from != prev_from {
                        found_fn(from, from_matched_len as usize);
                    }

                    prev_from = from;
                    prev_from_matched_len = from_matched_len;

                    from = match_pos as usize;
                    from_matched_len = match_len as u16;
                } else {
                    // match_len == from_match_len
                    break;
                }
            } else {
                break;
            }
        }

        (prev_from, prev_from_matched_len as usize)
    }

    /// search for the next match that is longer than `min_length`
    /// where `match_fn` is the byte-wise comparison function
    ///
    /// Returns the tuple (match_pos, match_len)
    fn search_from_offset<F: Fn(u32, u16) -> u16>(
        prev: &MultiLevelHashChain<N>,
        pos: usize,
        min_length: usize,
        mut from: usize,
        mut from_matched_len: u16,
        match_fn: F,
    ) -> Option<(usize, usize)> {
        // the maximum positions after which matches can no longer be encoded
        let long_offset_limit = pos.saturating_sub(LONG_OFFSET_MAX as usize);
        // the current search level
        // this function can also be implemented by looping through all levels at every position
        // but since consecutive searches often result in the same level
        // we remember the level and reuse it in the next position as a starting point
        let mut level = 0;

        'outer: loop {
            if from < long_offset_limit {
                return None;
            }

            // get a reference to the current position that we can reuse
            let cur_pos_chain = prev.at(from);

            // try to find the level that has an exact match with the current match length
            // since all levels that do not have an equal length cannot extend the current match
            loop {
                // copy the match for the current level into the stack
                // this is efficient because it can be done using SIMD
                let cur_level_match = cur_pos_chain.prev[level];

                if cur_level_match.length == 0 {
                    // either we are past the maximum level at this position
                    // or (at level == 0) there are no matches at all with this prefix
                    if level == 0 {
                        return None;
                    }
                    level -= 1;
                } else if from_matched_len < cur_level_match.length {
                    // the current match length is more than the match with the current position, meaning if there is
                    // an equal match length it must be in a level that is lower than the current level
                    if level == 0 || from_matched_len > cur_pos_chain.prev[level - 1].skip_length {
                        // either there are no levels below this, or the match length below is smaller than the current
                        // meaning there is no exact match at this position, so move to the next position in the chain
                        // follow the level that is larger than the current match length
                        // as all matches between the current position and the match must be too small
                        from = cur_level_match.position as usize;
                        // the match length does not change since this match matches more bytes than the current match length
                        continue 'outer;
                    }
                    level -= 1;
                } else if from_matched_len == cur_level_match.length {
                    // we found an exact match with the match length of the current position
                    // so this is a good candidate for extending the match length

                    if (cur_level_match.position as usize) < long_offset_limit {
                        // early exit so we don't have to check match length
                        return None;
                    }

                    // check the actual match length with the candidate position
                    let match_len = match_fn(cur_level_match.position, from_matched_len);
                    if match_len as usize > min_length {
                        // the match is longer than the requested minimum length, so return it
                        return Some((cur_level_match.position as usize, match_len as usize));
                    }

                    if match_len as usize == cur_level_match.length as usize {
                        // bad match
                        // we could not extend the match, so we need to find a position that matches just as many bytes
                        // but has a different next byte than the candidate position
                        if cur_level_match.bad_position == u32::MAX
                            || (cur_level_match.bad_position as usize) < long_offset_limit
                        {
                            // there is no viable bad match position
                            // so there cannot be any match with a different continuation byte
                            return None;
                        }

                        // check the match length with the bad match position
                        let match_len = match_fn(cur_level_match.bad_position, from_matched_len);
                        if match_len as usize > min_length {
                            // same as above
                            return Some((
                                cur_level_match.bad_position as usize,
                                match_len as usize,
                            ));
                        }

                        // whether the match was extended or not doesn't matter, we'll follow it either way
                        // as it is the farthest point that we know of where all in between positions can't be a good match
                        from_matched_len = match_len;
                        from = cur_level_match.bad_position as usize;
                        continue 'outer;
                    }

                    // the candidate position was a good match and extended the match length, follow it
                    from_matched_len = match_len;
                    from = cur_level_match.position as usize;
                    continue 'outer;
                } else if from_matched_len <= cur_level_match.skip_length {
                    // from_matched_len > cur_level_match.length;
                    // there might be a good match somewhere in the skip sequence, follow it
                    from_matched_len = cur_level_match.length;
                    from = cur_level_match.position as usize;
                    continue 'outer;
                } else {
                    // from_matched_len > cur_level_match.skip_length
                    // a good match might be in a higher level, so try to go up a level and check there
                    if level == N - 1 {
                        // we're already at the max level, follow this match
                        from_matched_len = cur_level_match.length;
                        from = cur_level_match.position as usize;
                        continue 'outer;
                    } else if cur_pos_chain.prev[level + 1].length == 0 {
                        // the level above this has no match so there can't be any match
                        return None;
                    } else if from_matched_len < cur_pos_chain.prev[level + 1].length {
                        // this would also happen if we just continue to the outer loop, but this saves some lookups
                        from = cur_pos_chain.prev[level + 1].position as usize;
                        level += 1;
                        continue 'outer;
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
