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
    reader: &mut (impl Read + Seek)
) -> Result<Vec<u8>, RefPackError> {
    let Header {
        decompressed_length,
        ..
    } = Header::read::<F::HeaderMode>(reader)?;

    let mut decompression_buffer = vec![0; decompressed_length as usize];
    let mut position = 0usize;

    while {
        let command = Command::read::<F::ControlMode>(reader)?;

        if let Some(bytes) = command.num_of_literal() {
            position = copy_from_reader(&mut decompression_buffer, reader, position, bytes)?;
        }

        if let Some((offset, length)) = command.offset_copy() {
            position = rle_decode_fixed(&mut decompression_buffer, position, offset, length)?;
        }

        !command.is_stop()
    } {}

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
///
/// ```
/// # Errors
///
/// - Will return `Error::InvalidMagic` if the header is malformed, indicating uncompressed data or
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
pub fn easy_decompress<F: Format>(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    decompress_internal::<F>(&mut reader)
}
