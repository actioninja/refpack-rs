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
use std::cmp::max;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use crate::data::control::{
    Command,
    Control,
    COPY_LITERAL_MAX,
    LITERAL_MAX,
    LONG_LENGTH_MAX,
    LONG_LENGTH_MIN,
    LONG_OFFSET_MAX,
    MEDIUM_LENGTH_MIN,
    MEDIUM_OFFSET_MAX,
    SHORT_OFFSET_MAX,
    SHORT_OFFSET_MIN,
};
use crate::format::Format;
use crate::header::mode::Mode as HeaderMode;
use crate::header::Header;
use crate::{RefPackError, RefPackResult};

// Optimization trick from libflate_lz77
// Faster lookups for very large tables
#[derive(Debug)]
enum PrefixTable {
    Small(HashMap<[u8; 3], u32>),
    Large(LargePrefixTable),
}

impl PrefixTable {
    fn new(bytes: usize) -> Self {
        if bytes < LONG_OFFSET_MAX as usize {
            PrefixTable::Small(HashMap::new())
        } else {
            PrefixTable::Large(LargePrefixTable::new())
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<u32> {
        match *self {
            PrefixTable::Small(ref mut table) => table.insert(prefix, position),
            PrefixTable::Large(ref mut table) => table.insert(prefix, position),
        }
    }
}

#[derive(Debug)]
struct LargePrefixTable {
    table: Vec<Vec<(u8, u32)>>,
}

impl LargePrefixTable {
    fn new() -> Self {
        LargePrefixTable {
            table: (0..=0xFFFF).map(|_| Vec::new()).collect(),
        }
    }

    fn insert(&mut self, prefix: [u8; 3], position: u32) -> Option<u32> {
        let p0 = prefix[0] as usize;
        let p1 = prefix[1] as usize;
        let p2 = prefix[2];

        let index = (p0 << 8) | p1;
        let positions = &mut self.table[index];
        for &mut (key, ref mut value) in &mut *positions {
            if key == p2 {
                let old = *value;
                *value = position;
                return Some(old);
            }
        }
        positions.push((p2, position));
        None
    }
}

fn prefix(input_buf: &[u8]) -> [u8; 3] {
    let buf: &[u8] = &input_buf[..3];
    [buf[0], buf[1], buf[2]]
}

/// Reads from an incoming `Read` reader and compresses and encodes to
/// `Vec<Control>`
pub(crate) fn encode_stream(
    reader: &mut (impl Read + Seek),
    length: usize,
) -> Result<Vec<Control>, RefPackError> {
    let mut in_buffer = vec![0_u8; length];
    reader.read_exact(&mut in_buffer)?;
    let mut controls: Vec<Control> = vec![];
    let mut prefix_table = PrefixTable::new(in_buffer.len());

    let mut i = 0;
    let end = max(3, in_buffer.len()) - 3;
    let mut literal_block: Vec<u8> = Vec::with_capacity(LITERAL_MAX as usize);
    while i < end {
        let key = prefix(&in_buffer[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        let pair = matched.map(|x| x as usize).and_then(|matched| {
            let distance = i - matched;
            if distance > LONG_OFFSET_MAX as usize || distance < SHORT_OFFSET_MIN as usize {
                None
            } else {
                // find the longest common prefix
                let max_copy_len = LONG_LENGTH_MAX as usize;
                let match_length = in_buffer[i..]
                    .iter()
                    .take(max_copy_len - 3)
                    .zip(&in_buffer[matched..])
                    .take_while(|(a, b)| a == b)
                    .count();

                // Insufficient similarity for given distance, reject
                if (match_length <= MEDIUM_LENGTH_MIN as usize
                    && distance > SHORT_OFFSET_MAX as usize)
                    || (match_length <= LONG_LENGTH_MIN as usize
                        && distance > MEDIUM_OFFSET_MAX as usize)
                {
                    None
                } else {
                    Some((matched, match_length))
                }
            }
        });

        if let Some((found, match_length)) = pair {
            let distance = i - found;

            // If the current literal block is longer than the copy limit we need to split
            // the block
            if literal_block.len() > COPY_LITERAL_MAX as usize {
                let split_point: usize = literal_block.len() - (literal_block.len() % 4);
                controls.push(Control::new_literal_block(&literal_block[..split_point]));
                let second_block = &literal_block[split_point..];
                controls.push(Control::new(
                    Command::new(distance, match_length, second_block.len()),
                    second_block.to_vec(),
                ));
            } else {
                // If it's not, just push a new block directly
                controls.push(Control::new(
                    Command::new(distance, match_length, literal_block.len()),
                    literal_block.clone(),
                ));
            }
            literal_block.clear();

            for k in (i..).take(match_length).skip(1) {
                if k >= end {
                    break;
                }
                prefix_table.insert(prefix(&in_buffer[k..]), k as u32);
            }

            i += match_length;
        } else {
            literal_block.push(in_buffer[i]);
            i += 1;
            // If it's reached the limit, push the block immediately and clear the running
            // block
            if literal_block.len() >= (LITERAL_MAX as usize) {
                controls.push(Control::new_literal_block(&literal_block));
                literal_block.clear();
            }
        }
    }
    // Add remaining literals if there are any
    if i < in_buffer.len() {
        literal_block.extend_from_slice(&in_buffer[i..]);
    }
    // Extremely similar to block up above, but with a different control type
    if literal_block.len() > 3 {
        let split_point: usize = literal_block.len() - (literal_block.len() % 4);
        controls.push(Control::new_literal_block(&literal_block[..split_point]));
        controls.push(Control::new_stop(&literal_block[split_point..]));
    } else {
        controls.push(Control::new_stop(&literal_block));
    }

    Ok(controls)
}

/// Compress a data stream from a Reader to refpack format into a Writer.
///
/// First parameter is the length; allows for compressing an arbitrary block
/// length from any reader.
///
/// Second and third parameter are the pregenerated reader and destination
/// writ.er
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
) -> RefPackResult<()> {
    if length == 0 {
        return Err(RefPackError::EmptyInput);
    }

    let controls = encode_stream(reader, length)?;

    let header_length = F::HeaderMode::length(length);

    let header_position = writer.stream_position()?;
    let data_start_pos = writer.seek(SeekFrom::Current(header_length as i64))?;

    for control in controls {
        control.write(writer)?;
    }

    let data_end_pos = writer.stream_position()?;

    let compression_length = data_end_pos - data_start_pos;

    let header = Header {
        compressed_length: Some(compression_length as u32),
        decompressed_length: length as u32,
    };

    writer.seek(SeekFrom::Start(header_position))?;

    header.write::<F::HeaderMode>(writer)?;

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
pub fn easy_compress<F: Format>(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    compress::<F>(input.len(), &mut reader, &mut writer)?;
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
        #[strategy(proptest::collection::vec(any::<u8>(), 100_000..=500_000))] input: Vec<u8>,
    ) {
        let _unused = easy_compress::<Reference>(&input).unwrap();
    }

    #[test]
    fn empty_input_yields_error() {
        let input = vec![];
        let result = easy_compress::<Reference>(&input);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RefPackError::EmptyInput));
    }
}
