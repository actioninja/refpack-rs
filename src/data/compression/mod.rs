////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Compression scheme is heavily based on lz77. Exact compression algorithm may
//! be subject to change.
//!
//! Basic concept is to track literal bytes as you encounter them, and have some
//! way of identifying when current bytes match previously encountered
//! sequences.
//!
//! Current tracked literal bytes *must* be written before a back-reference
//! copy command is written
//!
//! Literal blocks have a max length of 112, and if this limit is reached
//! the literal sequence must be split into two (or more) blocks to properly
//! encode the literals
//!
//! Due to the limited precision of literal blocks, special handling is required
//! for writing literal blocks before copy or stop controls. The literal block
//! needs to be "split" to make the literal take an even multiple of 4 bytes.
//!
//! This is done by getting the modulus of the number of bytes modulo 4
//! and then subtracting this remainder from the total length.
//!
//! Simple pseudo-rust:
//! ```
//! let tracked_bytes_length = 117;
//! let num_bytes_in_copy = tracked_bytes_length % 4; // 1
//! let num_bytes_in_literal = 117 - num_bytes_in_copy; // 116; factors by 4
//! ```
//!
//! See [Command] for a specification of control codes
mod fast;
mod fastest;
pub(crate) mod match_length;
mod optimal;
pub(crate) mod prefix_search;

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use crate::data::compression::fast::encode;
use crate::data::compression::optimal::{encode_slice_hc, HASH_CHAINING_LEVELS};
#[cfg(test)]
use crate::data::compression::prefix_search::hash_chain::HashChainPrefixSearcher;
use crate::data::compression::prefix_search::multi_level_hash_chain::MultiLevelPrefixSearcher;
use crate::data::control::{
    LONG_LENGTH_MAX,
    LONG_LENGTH_MIN,
    LONG_OFFSET_MAX,
    MEDIUM_LENGTH_MAX,
    MEDIUM_LENGTH_MIN,
    MEDIUM_OFFSET_MAX,
    SHORT_LENGTH_MAX,
    SHORT_OFFSET_MAX,
};
use crate::format::Format;
use crate::header::mode::Mode as HeaderMode;
use crate::header::Header;
use crate::{RefPackError, RefPackResult};

// used in both fast and high compression algorithms
fn bytes_for_match(length: usize, offset: usize) -> Option<(Option<usize>, usize)> {
    if offset > LONG_OFFSET_MAX as usize {
        return None;
    }
    if length >= LONG_LENGTH_MIN as usize {
        if length > MEDIUM_LENGTH_MAX as usize || offset > MEDIUM_OFFSET_MAX as usize {
            Some((Some(4), LONG_LENGTH_MAX as usize))
        } else if length > SHORT_LENGTH_MAX as usize || offset > SHORT_OFFSET_MAX as usize {
            Some((Some(3), MEDIUM_LENGTH_MAX as usize))
        } else {
            Some((Some(2), SHORT_LENGTH_MAX as usize))
        }
    } else if offset <= SHORT_OFFSET_MAX as usize {
        Some((Some(2), SHORT_LENGTH_MAX as usize))
    } else if offset <= MEDIUM_OFFSET_MAX as usize {
        if length >= MEDIUM_LENGTH_MIN as usize {
            Some((Some(3), MEDIUM_LENGTH_MAX as usize))
        } else {
            Some((None, MEDIUM_LENGTH_MIN as usize - 1))
        }
    } else {
        Some((None, LONG_LENGTH_MIN as usize - 1))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[non_exhaustive]
#[cfg_attr(test, derive(test_strategy::Arbitrary))]
pub enum CompressionOptions {
    Fastest,
    #[default]
    Fast,
    Optimal,
    #[cfg(test)]
    OptimalReference,
}

/// Compress a data stream from a Reader to refpack format into a Writer.
///
/// First parameter is the length; allows for compressing an arbitrary block
/// length from any reader.
///
/// Second and third parameter are the pregenerated reader and destination
/// writer
///
/// # Example
///
/// ```Rust
/// use std::io::Cursor;
///
/// let mut input = Cursor::new(b"Hello World!");
/// let mut output = Cursor::new(Vec::new());
///
/// // Compress the input into the output
/// refpack::compress(input.len(), &mut input, &mut output);
/// // output now contains the compressed version of the input
/// ```
///
/// # Errors
/// - [RefPackError::EmptyInput]: Length provided is 0
/// - [RefPackError::Io]: Generic IO error when reading or writing
pub fn compress<F: Format>(
    length: usize,
    reader: &mut (impl Read + Seek),
    writer: &mut (impl Write + Seek),
    compression_options: CompressionOptions,
) -> RefPackResult<()> {
    let mut buf = vec![0; length];
    reader.read_exact(buf.as_mut_slice())?;
    let out = easy_compress::<F>(&buf, compression_options)?;
    writer.write_all(&out)?;
    Ok(())
}

/// Wrapped compress function with a bit easier and cleaner of an API. Takes a
/// `&[u8]` slice of uncompressed bytes and returns a `Vec<u8>` of compressed
/// bytes
///
/// In implementation this just creates `Cursor`s for the reader and writer and
/// calls `compress`
///
/// Marked with `inline` so it should be inlined across crates and equivalent to
/// manually creating the cursors.
///
/// # Errors
/// - [RefPackError::EmptyInput]: Length provided is 0
/// - [RefPackError::Io]: Generic IO error when reading or writing
#[inline]
pub fn easy_compress<F: Format>(
    input: &[u8],
    compression_options: CompressionOptions,
) -> Result<Vec<u8>, RefPackError> {
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);

    let length = input.len();

    if input.is_empty() {
        return Err(RefPackError::EmptyInput);
    }

    let controls = match compression_options {
        CompressionOptions::Fastest => fastest::encode(input),
        CompressionOptions::Fast => encode(input),
        CompressionOptions::Optimal => {
            encode_slice_hc::<MultiLevelPrefixSearcher<{ HASH_CHAINING_LEVELS }>>(input)
        }
        #[cfg(test)]
        CompressionOptions::OptimalReference => encode_slice_hc::<HashChainPrefixSearcher>(input),
    };

    let header_length = F::HeaderMode::length(length);

    let header_position = writer.stream_position()?;
    let data_start_pos = writer.seek(SeekFrom::Current(header_length as i64))?;

    for control in controls {
        control.write(&mut writer)?;
    }

    let data_end_pos = writer.stream_position()?;

    let compression_length = data_end_pos - data_start_pos;

    let header = Header {
        compressed_length: Some(compression_length as u32),
        decompressed_length: length as u32,
    };

    writer.seek(SeekFrom::Start(header_position))?;

    header.write::<F::HeaderMode>(&mut writer)?;
    Ok(writer.into_inner())
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::*;
    use crate::format::Reference;

    #[proptest]
    #[ignore]
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::< u8 > (), 100_000..=500_000))] input: Vec<u8>,
        #[strategy(any::<CompressionOptions>())] options: CompressionOptions,
    ) {
        let _unused = easy_compress::<Reference>(&input, options).unwrap();
    }

    #[proptest]
    fn reader_compression(
        #[strategy(proptest::collection::vec(any::< u8 > (), 1..=256))] input: Vec<u8>,
        #[strategy(any::<CompressionOptions>())] options: CompressionOptions,
    ) {
        let length = input.len();
        let mut in_cursor = Cursor::new(input);
        let mut out_cursor = Cursor::new(Vec::new());
        compress::<Reference>(length, &mut in_cursor, &mut out_cursor, options).unwrap();
    }

    #[test]
    fn empty_input_yields_error() {
        let input = vec![];
        let result = easy_compress::<Reference>(&input, CompressionOptions::Fast);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RefPackError::EmptyInput));
    }

    #[proptest]
    fn optimal_matches_reference(
        #[strategy(proptest::collection::vec(0..=3u8, 1..=1_000_000))] input: Vec<u8>,
    ) {
        println!("in: {}", input.len());
        let compressed_reference =
            easy_compress::<Reference>(&input, CompressionOptions::OptimalReference)?;
        let compressed_optimal = easy_compress::<Reference>(&input, CompressionOptions::Optimal)?;
        println!(
            "out ref: {} opt: {}",
            compressed_reference.len(),
            compressed_optimal.len()
        );
        prop_assert_eq!(
            &compressed_reference,
            &compressed_optimal,
            "Optimal compression should match the reference implementation."
        );
    }
}
