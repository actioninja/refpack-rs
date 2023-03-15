////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::header::mode::Mode;
use crate::header::Header;
use crate::{header, RefPackError, RefPackResult};

/// Header used by many Maxis and SimEA games
/// Exactly the same as [Maxis](crate::header::mode::Maxis) but without the compressed length u32
///
/// ## Structure
/// - u8: Flags field
/// - Magic Number: 0xFB
/// - Big Endian u24/u32: Decompressed Length
pub struct Maxis2 {
    _private: (),
}

/// The decompressed length flag
/// Taken from http://simswiki.info/wiki.php?title=Sims_3:DBPF/Compression#Compression_Types
enum Flags {
    Little = 0x10,
    LittleRestricted = 0x40,
    Big = 0x80,
}

impl Mode for Maxis2 {
    fn length(decompressed_size: usize) -> usize {
        if decompressed_size > 0xFF_FF_FF {
            6
        } else {
            5
        }
    }

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let flags = match reader.read_u8()? {
            x if x == Flags::Little as u8 => Flags::Little,
            x if x == Flags::LittleRestricted as u8 => Flags::LittleRestricted,
            x if x == Flags::Big as u8 => Flags::Big,
            x => return Err(RefPackError::BadMagic(x)),
        };
        let magic = reader.read_u8()?;
        if magic != header::MAGIC {
            return Err(RefPackError::BadMagic(magic));
        }
        //Inexplicably this weird three byte number is stored Big Endian
        let decompressed_length = match flags {
            Flags::Little | Flags::LittleRestricted => reader.read_u24::<BigEndian>()?,
            Flags::Big => reader.read_u32::<BigEndian>()?,
        };
        Ok(Header {
            decompressed_length,
            compressed_length: None,
        })
    }

    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()> {
        let big_decompressed = header.decompressed_length > 0xFF_FF_FF;
        writer.write_u8(if big_decompressed { Flags::Big } else { Flags::Little } as u8)?;
        writer.write_u8(header::MAGIC)?;
        if big_decompressed {
            writer.write_u32::<BigEndian>(header.decompressed_length)?;
        } else {
            writer.write_u24::<BigEndian>(header.decompressed_length)?;
        }
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
        #[any(decompressed_limit = 16_777_214 * 2, compressed_limit = None)] header: Header,
    ) {
        let mut write_buf = vec![];
        let mut write_cur = Cursor::new(&mut write_buf);
        header.write::<Maxis2>(&mut write_cur).unwrap();

        prop_assert_eq!(write_buf.len(), Maxis2::length(header.decompressed_length as usize));

        let mut read_cur = Cursor::new(&mut write_buf);
        let got = Header::read::<Maxis2>(&mut read_cur).unwrap();

        prop_assert_eq!(header, got);
    }
}
