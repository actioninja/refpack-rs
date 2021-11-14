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
pub mod error;

use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use byteorder::{LittleEndian, BigEndian, ReadBytesExt };
use crate::control::{Control, Command};
use crate::error::Error;

const MAGIC: u16 = 0x10FB;

fn copy_within_slice<T: Copy>(v: &mut [T], from: usize, to: usize, len: usize) {
    if from > to {
        let (dst, src) = v.split_at_mut(from);
        dst[to..to + len].copy_from_slice(&src[..len]);
    } else {
        let (src, dst) = v.split_at_mut(to);
        dst[..len].copy_from_slice(&src[from..from + len]);
    }
}

pub fn compress<R: Read + Seek, W: Write>(reader: &mut R, writer: &mut W) -> Result<(), Error>{
    todo!()
}

pub fn easy_compress(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    compress(&mut reader, &mut writer)?;
    Ok(writer.into_inner())
}

pub fn decompress<R: Read + Seek, W: Write>(reader: &mut R, writer: &mut W) -> Result<(), Error>{
    let compressed_length = reader.read_u32::<LittleEndian>().unwrap();

    let magic = reader.read_u16::<BigEndian>()?;

    if magic != MAGIC {
        return Err(Error::InvalidMagic(magic));
    }

    let decompressed_length = reader.read_u24::<LittleEndian>()?;

    let mut decompression_buffer: Cursor<Vec<u8>> = Cursor::new(vec![0; decompressed_length as usize]);

    for control in control::Iter::new(reader) {
        if let Some(bytes) = control.bytes {
            decompression_buffer.write_all(&bytes)?;
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

pub fn easy_decompress(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut reader = Cursor::new(input);
    let mut writer: Cursor<Vec<u8>> = Cursor::new(vec![]);
    decompress(&mut reader, &mut writer)?;
    Ok(writer.into_inner())
}
