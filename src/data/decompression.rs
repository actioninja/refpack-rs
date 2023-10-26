////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Decompression parsing, algorithms, and functionality
use std::io::{Cursor, Read, Seek, Write};

use crate::data::control::Command;
use crate::data::{copy_from_reader, rle_decode_fixed};
use crate::format::Format;
use crate::header::Header;
use crate::RefPackError;

// Returning the internal buffer is the fastest way to return the data
// since that way the buffer doesn't have to be copied,
// this function is used to reach optimal performance
fn decompress_internal<F: Format>(
    reader: &mut (impl Read + Seek),
) -> Result<Vec<u8>, RefPackError> {
    let Header {
        decompressed_length,
        ..
    } = Header::read::<F::HeaderMode>(reader)?;

    let mut decompression_buffer = vec![0; decompressed_length as usize];
    let mut position = 0usize;

    loop {
        let command = Command::read(reader)?;

        match command {
            Command::Short {
                offset,
                length,
                literal,
            }
            | Command::Medium {
                offset,
                length,
                literal,
            } => {
                if literal > 0 {
                    position = copy_from_reader(
                        &mut decompression_buffer,
                        reader,
                        position,
                        literal as usize,
                    )?;
                }
                position = rle_decode_fixed(
                    &mut decompression_buffer,
                    position,
                    offset as usize,
                    length as usize,
                )
                .map_err(|error| RefPackError::ControlError { error, position })?;
            }
            Command::Long {
                offset,
                length,
                literal,
            } => {
                if literal > 0 {
                    position = copy_from_reader(
                        &mut decompression_buffer,
                        reader,
                        position,
                        literal as usize,
                    )?;
                }
                position = rle_decode_fixed(
                    &mut decompression_buffer,
                    position,
                    offset as usize,
                    length as usize,
                )
                .map_err(|error| RefPackError::ControlError { error, position })?;
            }
            Command::Literal(literal) => {
                position = copy_from_reader(
                    &mut decompression_buffer,
                    reader,
                    position,
                    literal as usize,
                )?;
            }
            Command::Stop(literal) => {
                copy_from_reader(
                    &mut decompression_buffer,
                    reader,
                    position,
                    literal as usize,
                )?;
                break;
            }
        }
    }

    Ok(decompression_buffer)
}

/// Decompress `refpack` data. Accepts arbitrary `Read`s and `Write`s.
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
/// ```
/// # Errors
///
/// - Will return `Error::InvalidMagic` if the header is malformed, indicating
///   uncompressed data or
/// attempting to decompress data in the incorrect format
/// - Will return `Error::Io` if there is an IO error
pub fn decompress<F: Format>(
    reader: &mut (impl Read + Seek),
    writer: &mut impl Write,
) -> Result<(), RefPackError> {
    let data = decompress_internal::<F>(reader)?;

    writer.write_all(data.as_slice())?;
    writer.flush()?;

    Ok(())
}

/// Wrapped decompress function with a bit easier and cleaner of an API.
/// Takes a slice of bytes and returns a Vec of byes
/// In implementation this just creates `Cursor`s for the reader and writer and
/// calls `decompress`
///
/// # Returns
///
/// A Result containing either `Vec<u8>` of the decompressed data or a
/// `RefPackError`.
///
/// # Errors
///
/// Will return `Error::InvalidMagic` if the header is malformed, indicating
/// uncompressed data Will return `Error::Io` if there is an IO error
#[inline]
pub fn easy_decompress<F: Format>(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    decompress_internal::<F>(&mut reader)
}
