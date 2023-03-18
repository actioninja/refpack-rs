////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! things relating the actual compressed data block. Anything past the header info,
//! the actual compression algorithms themselves, control codes, etc.

use crate::RefPackError;

pub mod compression;
pub mod control;
pub mod decompression;

/// Fast decoding of run length encoded data
/// Based on https://github.com/WanzenBug/rle-decode-helper/blob/master/src/lib.rs
///
/// Takes the last `lookbehind_length` items of the buffer and repeatedly appends them until
/// `fill_length` items have been copied.
///
/// If this function errors no data will have been copied
///
/// # Errors
/// * `lookbehind_length` is 0
/// * `lookbehind_length` >= `buffer.len()`
///
/// # Panics
/// * `fill_length + buffer.len()` would overflow
#[inline(always)]
pub(crate) fn rle_decode<T>(
    buffer: &mut Vec<T>,
    mut lookbehind_length: usize,
    mut fill_length: usize,
) -> Result<(), RefPackError> where T: Copy  {
    if lookbehind_length == 0 {
        return Err(RefPackError::BadOffset);
    }

    let copy_fragment_start = buffer.len()
        .checked_sub(lookbehind_length)
        .ok_or_else(|| RefPackError::NegativePosition(buffer.len(), lookbehind_length))?;

    // we don't need to reserve here because refpack has a decoded_length reserve

    // if fill_length == lookbehind_length, do only one call to extend_from_within so don't loop
    // (prevents call with (buffer, copy_fragment_start..copy_fragment_start) which does nothing)
    while fill_length > lookbehind_length {
        buffer.extend_from_within(
            copy_fragment_start..(copy_fragment_start + lookbehind_length)
        );
        fill_length -= lookbehind_length;
        lookbehind_length *= 2;
    }

    // Copy the last remaining bytes
    buffer.extend_from_within(
        copy_fragment_start..(copy_fragment_start + fill_length),
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use crate::format::Reference;
    use crate::{easy_compress, easy_decompress};

    #[proptest(ProptestConfig { cases: 100_000, ..Default::default() })]
    fn symmetrical_compression(#[filter(#input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[proptest(ProptestConfig {
    max_shrink_iters: 1_000_000,
    ..Default::default()
    })]
    fn symmetrical_compression_large_input(
        #[strategy(proptest::collection::vec(any::<u8>(), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }
}
