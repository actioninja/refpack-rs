////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Decompression parsing, algorithms, and functionality
use std::io::{Cursor, Read, Seek, Write};

use crate::data::control::iterator::Iter;
use crate::data::rle_decode;
use crate::format::Format;
use crate::header::Header;
use crate::RefPackError;

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
    let Header {
        decompressed_length,
        ..
    } = Header::read::<F::HeaderMode>(reader)?;

    let mut decompression_buffer = Vec::new();
    decompression_buffer.reserve_exact(decompressed_length as usize);

    for control in Iter::<_, F::ControlMode>::new(reader) {
        if !control.bytes.is_empty() {
            decompression_buffer.write_all(&control.bytes)?;
        }

        if let Some((offset, length)) = control.command.offset_copy() {
            let new_length = decompression_buffer.len() + length;
            if new_length > decompressed_length as usize {
                return Err(RefPackError::BadLength(new_length - decompressed_length as usize));
            }

            rle_decode(&mut decompression_buffer, offset, length)?;
        } else if decompression_buffer.len() > decompressed_length as usize {
            return Err(RefPackError::BadLength(decompression_buffer.len() - decompressed_length as usize));
        }
    }

    writer.write_all(decompression_buffer.as_slice())?;
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
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    decompress::<F>(&mut reader, &mut writer)?;
    Ok(writer.into_inner())
}
