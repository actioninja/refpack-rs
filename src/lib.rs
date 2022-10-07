////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! A rust crate for compressing and decompressing data in the RefPack format utilized by
//! many EA games of the early 2000s
//!
//! More details on the refpack format can be found at [the niotso wiki](http://wiki.niotso.org/RefPack). The short explanation is that RefPack is a compression scheme loosely based on LZ77 compression.
//!
//! The [Original Refpack Implementation](http://download.wcnews.com/files/documents/sourcecode/shadowforce/transfer/asommers/mfcapp_src/engine/compress/RefPack.cpp)
//! was referenced to ensure proper compatibility
//!
//! # Usage
//!
//! `refpack-rs` exposes two functions: `compress` and `decompress`, along with `easy` variants
//! with easier but less flexible of usage.
//!
//! `compress` and `decompress` take mutable references to a buffer to read and write from,
//! that implements `std::io::Read` and `std::io::Write`, respectively.
//!
//! `decompress` will read from the buffer until it encounters a stopcode (byte within (0xFC..=0xFF)),
//! while `compress` will read in the provided length.
//!
//! ### Example
//!
//! ```rust
//! use std::io::Cursor;
//! use std::io::Seek;
//!
//! let mut source_reader = Cursor::new(b"Hello World!".to_vec());
//! let mut out_buf = Cursor::new(vec![]);
//! refpack::compress(source_reader.get_ref().len(), &mut source_reader, &mut out_buf).unwrap();
//! ```
//!
//! The easy variants are `compress_easy` and `decompress_easy`, which take a `&[u8]` and return
//! a `Result<Vec<u8>, RefPackError>`.
//!
//! Internally they simply call `compress` and `decompress` with a `Cursor` to the input and
//! output buffers, however they are more convenient to use in many cases.

#![warn(clippy::pedantic, clippy::cargo)]
// Due to the high amount of byte conversions, sometimes intentional lossy conversions are necessary.
#![allow(clippy::cast_possible_truncation)]
// Annoying and wrong, RefPack is a compression scheme.
#![allow(clippy::doc_markdown)]
// Default::default() is more idiomatic imo
#![allow(clippy::default_trait_access)]
// too many lines is a dumb metric
#![allow(clippy::too_many_lines)]

mod control;
mod error;

use crate::control::{
    Command, Control, MAX_COPY_MEDIUM_OFFSET, MAX_COPY_SHORT_OFFSET, MAX_LITERAL_LEN,
    MAX_OFFSET_DISTANCE, MIN_COPY_LONG_LEN, MIN_COPY_MEDIUM_LEN, MIN_COPY_OFFSET,
};
pub use crate::error::Error as RefPackError;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::max;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

pub const MAGIC: u16 = 0x10FB;
pub const MAX_WINDOW_SIZE: u32 = MAX_OFFSET_DISTANCE as u32;
pub const HEADER_LEN: u16 = 9;
pub const MAX_LITERAL_BLOCK: u16 = MAX_LITERAL_LEN as u16;

/// Simple utility function that does a fast memory region copy within a slice
fn copy_within_slice<T: Copy>(v: &mut [T], from: usize, to: usize, len: usize) {
    if from > to {
        let (dst, src) = v.split_at_mut(from);
        dst[to..to + len].copy_from_slice(&src[..len]);
    } else {
        let (src, dst) = v.split_at_mut(to);
        dst[..len].copy_from_slice(&src[from..from + len]);
    }
}

//Optimization trick from libflate_lz77
//Faster lookups for very large tables
#[derive(Debug)]
enum PrefixTable {
    Small(HashMap<[u8; 3], u32>),
    Large(LargePrefixTable),
}

impl PrefixTable {
    fn new(bytes: usize) -> Self {
        if bytes < MAX_WINDOW_SIZE as usize {
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
        for &mut (key, ref mut value) in positions.iter_mut() {
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

fn encode_stream<R: Read + Seek>(
    reader: &mut R,
    length: usize,
) -> Result<Vec<Control>, RefPackError> {
    let mut in_buffer = vec![0_u8; length];
    reader.read_exact(&mut in_buffer)?;
    let mut controls: Vec<Control> = vec![];
    let mut prefix_table = PrefixTable::new(in_buffer.len());

    let mut i = 0;
    let end = max(3, in_buffer.len()) - 3;
    let mut literal_block: Vec<u8> = Vec::with_capacity(MAX_LITERAL_BLOCK as usize);
    while i < end {
        let key = prefix(&in_buffer[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        let pair = matched.map(|x| x as usize).and_then(|matched| {
            let distance = i - matched;
            if distance > MAX_OFFSET_DISTANCE || distance < MIN_COPY_OFFSET as usize {
                None
            } else {
                // find the longest common prefix
                let match_length = in_buffer[i..]
                    .iter()
                    .take(control::MAX_COPY_LEN - 3)
                    .zip(&in_buffer[matched..])
                    .take_while(|(a, b)| a == b)
                    .count();

                if (match_length <= MIN_COPY_MEDIUM_LEN as usize
                    && distance > MAX_COPY_SHORT_OFFSET as usize)
                    || (match_length <= MIN_COPY_LONG_LEN as usize
                        && distance > MAX_COPY_MEDIUM_OFFSET as usize)
                {
                    None
                } else {
                    Some((matched, match_length))
                }
            }
        });

        if let Some((found, match_length)) = pair {
            let distance = i - found;

            // If the current literal block is longer than 3 we need to split the block
            if literal_block.len() > 3 {
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
            if literal_block.len() >= (MAX_LITERAL_BLOCK as usize) {
                controls.push(Control::new_literal_block(&literal_block));
                literal_block.clear();
            }
        }
    }
    //Add remaining literals if there are any
    if i < in_buffer.len() {
        literal_block.extend_from_slice(&in_buffer[i..]);
    }
    //Extremely similar to block up above, but with a different control type
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
/// First parameter is the length; allows for compressing an arbitrary block length from any reader.
///
/// Second and third parameter are the source reader and destination writ.er
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
///
/// ```
///
/// # Errors
///
/// Will return `Error::Io` if there is an IO error
/// Will return `Error::EmptyInput` if the length provided is 0
// Adapted from libflate_lz77
pub fn compress<R: Read + Seek, W: Write>(
    length: usize,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), RefPackError> {
    if length == 0 {
        return Err(RefPackError::EmptyInput);
    }

    let controls = encode_stream(reader, length)?;

    let mut out_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());

    for control in controls {
        control.write(&mut out_buf)?;
    }

    let out_buf = out_buf.into_inner();

    writer.write_u32::<LittleEndian>(u32::from(HEADER_LEN) + (out_buf.len() as u32))?;
    writer.write_u16::<BigEndian>(MAGIC)?;
    writer.write_u24::<BigEndian>(length as u32)?;
    writer.write_all(&out_buf)?;
    writer.flush()?;
    Ok(())
}

/// Wrapped compress function with a bit easier and cleaner of an API.
/// Takes a slice of uncompressed bytes and returns a Vec of compressed bytes
/// In implementation this just creates `Cursor`s for the reader and writer and calls `compress`
///
/// # Errors
///
/// Will return `Error::Io` if there is an IO error
#[inline]
pub fn easy_compress(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    compress(input.len(), &mut reader, &mut writer)?;
    Ok(writer.into_inner())
}

/// Decompress refpack data.
///
/// Accepts arbitrary `Read`s and `Write`s.
///
/// # Example
///
/// ```Rust
/// use std::io::Cursor;
///
/// let mut input = Cursor::new(/* some refpack data */);
/// let mut output = Cursor::new(Vec::new());
///
/// // decompress the input into the output
/// refpack::compress(&mut input, &mut output);
/// // output now contains the decompressed version of the input
///
/// ```
/// # Errors
///
/// Will return `Error::InvalidMagic` if the header is malformed, indicating uncompressed data
/// Will return `Error::Io` if there is an IO error
pub fn decompress<R: Read + Seek, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<(), RefPackError> {
    let _compressed_length = reader.read_u32::<LittleEndian>()?;

    let magic = reader.read_u16::<BigEndian>()?;

    if magic != MAGIC {
        return Err(RefPackError::InvalidMagic(magic));
    }

    let decompressed_length = reader.read_u24::<BigEndian>()?;

    let mut decompression_buffer: Cursor<Vec<u8>> =
        Cursor::new(vec![0; decompressed_length as usize]);

    for control in control::Iter::new(reader) {
        if !control.bytes.is_empty() {
            decompression_buffer.write_all(&control.bytes)?;
        }

        if let Some((offset, length)) = control.command.offset_copy() {
            let decomp_pos = decompression_buffer.position() as usize;
            let src_pos = decomp_pos - offset;

            let buf = decompression_buffer.get_mut();

            if (src_pos + length) < decomp_pos {
                copy_within_slice(buf, src_pos, decomp_pos, length);
            } else {
                for i in 0..length {
                    let target = decomp_pos + i;
                    let source = src_pos + i;
                    buf[target] = buf[source];
                }
            }
            decompression_buffer.seek(SeekFrom::Current(length as i64))?;
        }
    }

    writer.write_all(decompression_buffer.get_ref())?;
    writer.flush()?;

    Ok(())
}

/// Wrapped decompress function with a bit easier and cleaner of an API.
/// Takes a slice of bytes and returns a Vec of byes
/// In implementation this just creates `Cursor`s for the reader and writer and calls `decompress`
///
/// # Returns
///
/// A Result containing either `Vec<u8>` of the decompressed data or a `RefPackError`.
///
/// # Errors
///
/// Will return `Error::InvalidMagic` if the header is malformed, indicating uncompressed data
/// Will return `Error::Io` if there is an IO error
#[inline]
pub fn easy_decompress(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    decompress(&mut reader, &mut writer)?;
    Ok(writer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use test_strategy::proptest;

    #[proptest(ProptestConfig { cases: 100_000, ..Default::default() })]
    fn symmetrical_compression(#[filter(#input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress(&input).unwrap();
        let decompressed = easy_decompress(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[proptest]
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::<u8>(), (100_000..=500_000)))] input: Vec<u8>,
    ) {
        let _unused = easy_compress(&input).unwrap();
    }

    #[proptest(ProptestConfig {
        max_shrink_iters: 1_000_000,
        ..Default::default()
    })]
    fn symmetrical_compression_large_input(
        #[strategy(proptest::collection::vec(any::<u8>(), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress(&input).unwrap();
        let decompressed = easy_decompress(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }
}
