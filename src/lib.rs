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

pub mod data;
mod error;
mod format;
pub mod header;

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};
use data::control::{iterator, MAX_LITERAL_LEN, MAX_OFFSET_DISTANCE};

use crate::data::compression::encode_stream;
pub use crate::error::{Error as RefPackError, Result as RefPackResult};
use crate::format::Format;
use crate::header::mode::Mode;
use crate::header::Header;

pub const MAX_WINDOW_SIZE: u32 = MAX_OFFSET_DISTANCE as u32;
pub const HEADER_LEN: u32 = 9;
pub const MAX_LITERAL_BLOCK: u16 = MAX_LITERAL_LEN as u16;

/*
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
    let decompressed_length = reader.read_u32::<LittleEndian>()?;

    let magic = reader.read_u16::<BigEndian>()?;

    if magic != MAGIC {
        return Err(RefPackError::InvalidMagic(magic));
    }

    let _compressed_length = reader.read_u24::<BigEndian>()?;

    let mut decompression_buffer: Cursor<Vec<u8>> =
        Cursor::new(vec![0; decompressed_length as usize]);

    for control in iterator::Iter::new(reader) {
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
    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::*;

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

    #[test]
    fn blah() {
        let test = easy_compress(&[0x04, 0x23, 0x13, 0x98]).unwrap();
        println!("{:X?}", test);
    }
}
 */

/// Compress a data stream from a Reader to refpack format into a Writer.
///
/// First parameter is the length; allows for compressing an arbitrary block length from any reader.
///
/// Second and third parameter are the pregenerated reader and destination writ.er
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
fn compress<F: Format>(
    length: usize,
    reader: &mut (impl Read + Seek),
    writer: &mut (impl Write + Seek),
) -> RefPackResult<()> {
    if length == 0 {
        return Err(RefPackError::EmptyInput);
    }

    let controls = encode_stream(reader, length)?;

    let header_length = F::HeaderMode::LENGTH;

    let header_position = writer.stream_position()?;
    let data_start_pos = writer.seek(SeekFrom::Current(header_length as i64))?;

    for control in controls {
        control.write::<F::ControlMode>(writer)?;
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

/// Wrapped compress function with a bit easier and cleaner of an API.
/// Takes a slice of uncompressed bytes and returns a Vec of compressed bytes
/// In implementation this just creates `Cursor`s for the reader and writer and calls `compress`
///
/// Marked with `inline` so it should be inlined across crates and equivalent to manually creating
/// the cursors.
///
/// # Errors
///
/// Will return `Error::Io` if there is an IO error
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
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::<u8>(), (100_000..=500_000)))] input: Vec<u8>,
    ) {
        let _unused = easy_compress::<Reference>(&input).unwrap();
    }
}
