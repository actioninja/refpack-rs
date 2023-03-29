////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! things relating the actual compressed data block. Anything past the header info,
//! the actual compression algorithms themselves, control codes, etc.

use std::io::{Read, Seek};

use crate::RefPackError;

pub mod compression;
pub mod control;
pub mod decompression;

/// Fast decoding of run length encoded data
/// Based on https://github.com/WanzenBug/rle-decode-helper/blob/master/src/lib.rs
///
/// Takes the last `offset` items of the buffer and repeatedly copies them
/// to `position` until `length` items have been copied.
///
/// If this function errors no data will have been copied
///
/// # Errors
/// * `offset` is 0
/// * `offset` > `position`
/// * `position + length` > `buffer.len()`
///
/// # Panics
/// * `fill_length + buffer.len()` would overflow
///
/// # Returns
/// * the new position of the buffer after the read
#[inline(always)]
pub(crate) fn rle_decode_fixed<T>(
    buffer: &mut [T],
    mut position: usize,
    mut offset: usize,
    mut length: usize,
) -> Result<usize, RefPackError>
where
    T: Copy,
{
    if offset == 0 {
        return Err(RefPackError::BadOffset);
    }
    if offset > position {
        return Err(RefPackError::NegativePosition(position, offset));
    }
    if position + length > buffer.len() {
        return Err(RefPackError::BadLength(position + length - buffer.len()));
    }

    let copy_fragment_start = position - offset;

    while length > offset {
        buffer.copy_within(copy_fragment_start..position, position);
        length -= offset;
        position += offset;
        offset *= 2;
    }

    buffer.copy_within(
        copy_fragment_start..(copy_fragment_start + length),
        position,
    );
    position += length;

    Ok(position)
}

/// Copy `length` bytes from the reader into `buffer` at `position`
///
/// # Errors
/// * `position + length` > `buffer.len()`
/// * General IO error
///
/// # Returns
/// * the new position of the buffer after the read
#[inline(always)]
pub(crate) fn copy_from_reader(
    buffer: &mut [u8],
    reader: &mut (impl Read + Seek),
    position: usize,
    length: usize,
) -> Result<usize, RefPackError> {
    if position + length > buffer.len() {
        return Err(RefPackError::BadLength(position + length - buffer.len()));
    }

    reader.read_exact(&mut buffer[position..(position + length)])?;

    Ok(position + length)
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use crate::format::Reference;
    use crate::{easy_compress, easy_decompress};

    #[proptest(ProptestConfig { cases: 100_000, ..Default::default() })]
    fn symmetrical_compression(#[filter(# input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[proptest(ProptestConfig {
    max_shrink_iters: 1_000_000,
    ..Default::default()
    })]
    fn symmetrical_compression_large_input(
        #[strategy(proptest::collection::vec(any::< u8 > (), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }
}
