////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! things relating the actual compressed data block. Anything past the header
//! info, the actual compression algorithms themselves, control codes, etc.

use std::io::{Read, Seek};

use onlyerror::Error;

use crate::RefPackError;

pub mod compression;
pub mod control;
pub mod decompression;

#[derive(Error, Debug)]
pub enum DecodeError {
    /// Error indicating that offset was 0 in refpack control byte. This doesn't
    /// make sense, and likely indicated the data is corrupted or malformed.
    #[error("Offset is 0 in compressed data control command")]
    BadOffset,
    /// Error indicating that the requested copy offset would go past the start
    /// of the buffer. This indicates malformed or corrupted data.
    ///
    /// ### Fields
    /// - usize: buffer length
    /// - usize: offset requested
    #[error("Offset went past start of buffer: buffer length `{0}`, offset `{1}`")]
    NegativePosition(usize, usize),
    /// Error indicating that during decompression, the RLE decode attempted to
    /// write past the end of the decompression buffer
    ///
    /// This error exists to prevent maliciously constructed data from using an
    /// unbounded amount of memory
    ///
    /// ### Fields
    /// - usize: amount of bytes attempted to write past
    #[error("Decompressed data overran decompressed size in header by `{0}` bytes")]
    BadLength(usize),
}

/// Fast decoding of run length encoded data
/// Based on https://github.com/WanzenBug/rle-decode-helper/blob/master/src/lib.rs
///
/// Takes the last `offset` items of the buffer and repeatedly copies them
/// to `position` until `length` items have been copied.
///
/// If this function errors no data will have been copied
///
/// # Errors
/// - [DecodeError::BadOffset]: `offset` is 0
/// - [DecodeError::NegativePosition]: `offset` > `position`
/// - [DecodeError::BadLength]: `position + length` > `buffer.len()`
///
/// # Panics
/// - `fill_length + buffer.len()`
/// * `fill_length + buffer.len()` would overflow
///
/// # Returns
/// the new position of the buffer after the read
#[inline(always)]
pub(crate) fn rle_decode_fixed<T: Copy>(
    buffer: &mut [T],
    mut position: usize,
    mut offset: usize,
    mut length: usize,
) -> Result<usize, DecodeError> {
    if offset == 0 {
        return Err(DecodeError::BadOffset);
    }
    if offset > position {
        return Err(DecodeError::NegativePosition(position, offset));
    }
    if position + length > buffer.len() {
        return Err(DecodeError::BadLength(position + length - buffer.len()));
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
/// # Returns
/// the new position of the buffer after the read
///
/// # Errors
/// - [RefPackError::Io]: General IO Error when reading from the reader
///
/// # Panics
/// Panics if a copy would go past the end of the buffer to copy to
#[inline(always)]
pub(crate) fn copy_from_reader(
    buffer: &mut [u8],
    reader: &mut (impl Read + Seek),
    position: usize,
    length: usize,
) -> Result<usize, RefPackError> {
    assert!(
        buffer.len() >= position + length,
        "Attempted to copy past end of input buffer; position: {position}; length: {length}"
    );

    reader.read_exact(&mut buffer[position..(position + length)])?;

    Ok(position + length)
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::*;
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
        #[strategy(proptest::collection::vec(any::<u8>(), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    mod rle_decode {
        use super::*;

        #[test]
        fn errors_on_bad_offset() {
            let error = rle_decode_fixed(&mut [0], 0, 0, 1).unwrap_err();
            assert!(matches!(error, DecodeError::BadOffset));
        }

        #[test]
        fn errors_on_negative_position() {
            let error = rle_decode_fixed(&mut [0], 0, 1, 1).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Offset went past start of buffer: buffer length `0`, offset `1`"
            );
        }

        #[test]
        fn errors_on_bad_length() {
            let error = rle_decode_fixed(&mut [0, 0], 1, 1, 10).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Decompressed data overran decompressed size in header by `9` bytes"
            );
        }
    }
}
