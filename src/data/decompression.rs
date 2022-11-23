////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use crate::data::control::iterator::Iter;
use crate::data::copy_within_slice;
use crate::format::Format;
use crate::header::Header;
use crate::RefPackError;

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
pub fn decompress<F: Format>(
    reader: &mut (impl Read + Seek),
    writer: &mut impl Write,
) -> Result<(), RefPackError> {
    let Header {
        decompressed_length,
        ..
    } = Header::read::<F::HeaderMode>(reader)?;

    let mut decompression_buffer: Cursor<Vec<u8>> =
        Cursor::new(vec![0; decompressed_length as usize]);

    for control in Iter::<_, F::ControlMode>::new(reader) {
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
pub fn easy_decompress<F: Format>(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    decompress::<F>(&mut reader, &mut writer)?;
    Ok(writer.into_inner())
}
