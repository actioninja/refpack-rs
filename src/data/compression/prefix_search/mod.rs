////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

pub(crate) mod hash_chain;
pub(crate) mod hash_table;
pub(crate) mod multi_level_hash_chain;

pub(crate) fn prefix(input_buf: &[u8]) -> [u8; 3] {
    let buf: &[u8] = &input_buf[..3];
    [buf[0], buf[1], buf[2]]
}

pub(crate) trait PrefixSearcher<'a> {
    fn build(buffer: &'a [u8]) -> Self;

    /// Search for all increasingly large matches in the search buffer.
    ///
    /// It is required to call search in order for all elements.
    ///
    /// This function will call found_fn with the position and range of match lengths for that position.
    /// The function must return all matches with the search position `pos` starting from the closest
    /// found position that are within [LONG_OFFSET_MAX](crate::data::control::LONG_OFFSET_MAX) bytes.
    /// All returned matches should have a longer match length than the last.
    fn search<F: FnMut(usize, usize, usize)>(&mut self, pos: usize, found_fn: F);
}

// optimization: we only have to reserve LONG_OFFSET_MAX + 1 bytes
// but since it costs less instructions to do modulo by a power of two
// we'll use the next largest power of two
const HASH_CHAIN_BUFFER_SIZE: usize = 1 << 18;
