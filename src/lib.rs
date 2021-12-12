////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

#![warn(clippy::pedantic, clippy::cargo)]
// Due to the high amount of byte conversions, sometimes intentional lossy conversions are necessary.
#![allow(clippy::cast_possible_truncation)]
// Default::default() is more idiomatic imo
#![allow(clippy::default_trait_access)]
// too many lines is a dumb metric
#![allow(clippy::too_many_lines)]

mod control;
mod error;

use crate::control::{Command, Control};
pub use crate::error::Error as RefPackError;
use binrw::{BinWrite, WriteOptions};
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::max;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

pub const MAGIC: u16 = 0x10FB;
pub const MAX_WINDOW_SIZE: u32 = 131_072;
pub const HEADER_LEN: u16 = 9;
pub const MAX_LITERAL_BLOCK: u16 = 112;

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

/// # Errors
///
/// Will return `Error::Io` if there is an IO error
// Adapted from libflate_lz77
pub fn compress<R: Read + Seek, W: Write>(
    length: usize,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), RefPackError> {
    if length == 0 {
        return Err(RefPackError::EmptyInput);
    }
    let mut in_buffer = vec![0_u8; length];
    reader.read_exact(&mut in_buffer)?;

    let mut controls: Vec<Control> = vec![];
    let mut prefix_table = PrefixTable::new(in_buffer.len());
    let mut i = 0;
    let end = max(3, in_buffer.len()) - 3;
    let mut literal_block: Vec<u8> = Vec::with_capacity(MAX_LITERAL_BLOCK as usize);
    while i < end {
        // get current running prefix
        let key = prefix(&in_buffer[i..]);

        // get the position of the prefix in the table (if it exists)
        let matched = prefix_table.insert(key, i as u32);

        if let Some(found) = matched.map(|x| x as usize) {
            let distance = i - found;
            if distance < MAX_LITERAL_BLOCK as usize {
                // find the longest common prefix
                let match_length = 3 + &in_buffer[i..]
                    .iter()
                    .take(control::MAX_COPY_LEN - 3)
                    .zip(&in_buffer[found..])
                    .take_while(|(a, b)| a == b)
                    .count();

                if literal_block.len() > 3 {
                    let split_point: usize = literal_block.len() - (literal_block.len() % 4);
                    controls.push(Control::new_literal_block(&literal_block[..split_point]));
                    let second_block = &literal_block[split_point..];
                    controls.push(Control::new(
                        Command::new(distance, match_length, second_block.len()),
                        second_block.to_vec(),
                    ));
                } else {
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
            }
            //todo!("when a match is found, it needs to split the block! literals only in inc of 4, 0-3 on non-literal")
        } else {
            literal_block.push(in_buffer[i]);
            i += 1;
            if literal_block.len() >= (MAX_LITERAL_BLOCK as usize) {
                controls.push(Control::new_literal_block(&literal_block));
                literal_block.clear();
            }
        }
    }
    //Add remaining literals
    literal_block.extend_from_slice(&in_buffer[i..]);
    //Extremely similar to block up above, but with a different control type
    if literal_block.len() > 3 {
        let split_point: usize = literal_block.len() - (literal_block.len() % 4);
        controls.push(Control::new_literal_block(&literal_block[..split_point]));
        controls.push(Control::new_stop(&literal_block[split_point..]));
    } else {
        controls.push(Control::new_stop(&literal_block));
    }

    let mut out_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    controls.write_options(&mut out_buf, &WriteOptions::default(), ())?;
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
pub fn easy_compress(input: &[u8]) -> Result<Vec<u8>, RefPackError> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    compress(input.len(), &mut reader, &mut writer)?;
    Ok(writer.into_inner())
}

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
/// Takes a slice of bytes and returns a Vec of bytes
/// In implementation this just creates `Cursor`s for the reader and writer and calls `decompress`
///
/// # Errors
///
/// Will return `Error::InvalidMagic` if the header is malformed, indicating uncompressed data
/// Will return `Error::Io` if there is an IO error
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

    #[proptest]
    fn symmetrical_compression(#[filter(#input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress(&input).unwrap();
        let decompressed = easy_decompress(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }
}
