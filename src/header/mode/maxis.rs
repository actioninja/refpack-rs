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

pub struct Maxis;

impl Mode for Maxis {
    const LENGTH: usize = 9;

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let compressed_length_prewrap = reader.read_u32::<LittleEndian>()?;
        let compressed_length = if compressed_length_prewrap == 0 {
            None
        } else {
            Some(compressed_length_prewrap)
        };
        let magic = reader.read_u16::<BigEndian>()?;
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
        writer.write_u16::<BigEndian>(header::MAGIC)?;
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
    fn symmetrical_read_write(#[filter(#header.decompressed_length < 16_777_215)] header: Header) {
        let mut write_buf = vec![];
        let mut write_cur = Cursor::new(&mut write_buf);
        header.write::<Maxis>(&mut write_cur).unwrap();
        let mut read_cur = Cursor::new(&mut write_buf);
        let got = Header::read::<Maxis>(&mut read_cur).unwrap();

        prop_assert_eq!(header, got);
    }
}
