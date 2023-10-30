////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Decompression parsing, algorithms, and functionality. Exact decompression
//! algorithm is subject to change.
//!
//! Basic concept is to parse the header, identify key information such as
//! decompressed header, then parse as a repeating stream of "command" blocks,
//! consisting of a control code and any (if any) following literal bytes.
//!
//! Literal bytes should always be written before performing the control code
//! operation.
//!
//! Control code operations are "run length encoded", which is a format of
//! encoding where the "length" of a pointer-style control can be longer than
//! the lookback length. This indicates that the section within the lookback
//! region should repeat until the length is fulfilled
//!
//! # Example Byte-by-byte Algorithm
//!
//! This is *normally* handled via byte-by-byte decoding, where a lookback
//! position and write position counter are incremented at the same time,
//! however other faster implementations may be possible. This algorithm
//! explanation is purely intended to illustrate the concept.
//!
//! ## Algorithm steps
//!
//! while in reality the possible allowed values are any arbitrary byte,
//! characters are used to indicate whole bytes to simplify illustration
//!
//! Given the current decoded output of `DEADBEEF`, the next control encountered
//! has a lookback of `4`, and a length of `16`.
//!
//! ### 1. Create Pointers
//! Create pointers/get indexes for the current lookback position and current
//! output position
//!
//! ```text
//! DEADBEEF
//!     ^   ^
//!     LB  O
//! ```
//!
//! ### 2. Copy from lookback to output
//! Take the current byte at the lookback position, and copy it to the output
//! position
//!
//! ```text
//! DEADBEEFB
//!     ^   ^
//!     LB  O
//! ```
//!
//! ### 3. Advance Pointers
//! Increment both the lookback position and the output position
//!
//! ```text
//! DEADBEEFB
//!      ^   ^
//!      LB  O
//! ```
//!
//! ### 4. Repeat `length` times
//! Steps 2 and 3 should repeat a total of times equal to the length of the
//! control block.
//!
//! #### 4.A "What about when the lookback reaches already output?"
//! In a case where the length overruns into the already written output, the
//! process continues as normal and the output starts to repeat.
//!
//! in this example output, On the 5th iteration you will encounter this state:
//! ```text
//! DEADBEEFBEEF
//!         ^   ^
//!         LB  O
//! ```
//! The lookback is currently pointing at the first "B" that has been written.
//! This isn't actually a problem, and step 2 and 3 should be followed as normal:
//!
//! ```text
//! DEADBEEFBEEFB
//!         ^   ^
//!         LB  O
//! ```
//!
//! ```text
//! DEADBEEFBEEFB
//!          ^   ^
//!          LB  O
//! ```
//!
//! ## Final Result
//! Given that the algorithm was followed properly, the final output of the
//! example input should be
//! ```text
//! DEADBEEFBEEFBEEFBEEFBEEF
//! ```
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
/// - [RefPackError::BadMagic]: Header magic was malformed, likely indicating
///   either uncompressed data or attempting to decompress data in an incorrect
///   format
/// - [RefPackError::BadFlags]: Header magic was malformed, likely indicating
///   either uncompressed data or attempting to decompress data in an incorrect
///   format
/// - [RefPackError::ControlError]: Invalid control code operation was attempted
///   to be performed. This normally indicated corrupted or invalid refpack
///   data
/// - [RefPackError::Io]: Generic IO error occured while attempting to read or
///   write data
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
/// - [RefPackError::BadMagic]: Header was malformed, likely indicating either
///   uncompressed data or attempting to decompress data in an incorrect format
/// - [RefPackError::BadFlags]: Header magic was malformed, likely indicating
///   either uncompressed data or attempting to decompress data in an incorrect
///   format
/// - [RefPackError::ControlError]: Invalid control code operation was attempted
///   to be performed. This normally indicated corrupted or invalid refpack
///   data
/// - [RefPackError::Io]: Generic IO error occured while attempting to read or
///   write data
#[inline]
pub fn easy_decompress<F: Format>(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    decompress_internal::<F>(&mut reader)
}
