////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::header::mode::Mode;
use crate::header::Header;
use crate::{header, RefPackError, RefPackResult};

/// Header used by many Maxis and SimEA games
///
/// ## Structure
/// - Little Endian u32: Compressed length
/// - u8: Flags field
///     only useful flag is large length which tells it to read decompressed length as u32
/// - Magic Number: 0xFB
/// - Big Endian u24/u32: Decompressed Length
pub struct Maxis {
    _private: (),
}

pub const FLAGS: u8 = 0x10;

impl Mode for Maxis {
    const LENGTH: usize = 9;

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let compressed_length_prewrap = reader.read_u32::<LittleEndian>()?;
        let compressed_length = if compressed_length_prewrap == 0 {
            None
        } else {
            Some(compressed_length_prewrap)
        };
        let _flags = reader.read_u8()?;
        let magic = reader.read_u8()?;
        if magic != header::MAGIC {
            return Err(RefPackError::BadMagic(magic));
        }
        //Inexplicably this weird three byte number is stored Big Endian
        let decompressed_length = reader.read_u24::<BigEndian>()?;
        Ok(Header {
            decompressed_length,
            compressed_length,
        })
    }

    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()> {
        writer.write_u32::<LittleEndian>(header.compressed_length.unwrap_or(0))?;
        writer.write_u8(FLAGS)?;
        writer.write_u8(header::MAGIC)?;
        writer.write_u24::<BigEndian>(header.decompressed_length)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use proptest::prop_assert_eq;
    use test_strategy::proptest;

    use super::*;
    use crate::header::Header;

    #[proptest]
    fn symmetrical_read_write(
        #[any(decompressed_limit = 16_777_214, compressed_limit = Some(u32::MAX))] header: Header,
    ) {
        let mut write_buf = vec![];
        let mut write_cur = Cursor::new(&mut write_buf);
        header.write::<Maxis>(&mut write_cur).unwrap();
        let mut read_cur = Cursor::new(&mut write_buf);
        let got = Header::read::<Maxis>(&mut read_cur).unwrap();

        prop_assert_eq!(header, got);
    }
}
