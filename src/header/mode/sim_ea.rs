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
use crate::RefPackError::BadFlags;

/// Header used by many Maxis and SimEA games
/// The same as [Maxis](crate::header::mode::Maxis) but without the compressed length u32,
/// and the use of the flags field
///
/// ## Structure
/// - u8: Flags field
/// - Magic Number: 0xFB
/// - Big Endian u24/u32: Decompressed Length
pub struct SimEA {
    _private: (),
}

/// The header flags
/// Based on http://simswiki.info/wiki.php?title=Sims_3:DBPF/Compression#Compression_Types
/// and http://wiki.niotso.org/RefPack#Header
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
struct Flags {
    big_decompressed: bool,
    restricted: bool,
    compressed_size_present: bool,
}

impl Flags {
    fn read(data: u8) -> RefPackResult<Self> {
        if (data & 0b0010_1110) > 0 {
            Err(BadFlags(data))
        } else {
            Ok(Self {
                big_decompressed: ((data >> 7) & 1) > 0,
                restricted: ((data >> 6) & 1) > 0,
                compressed_size_present: (data & 1) > 0,
            })
        }
    }

    fn write(self) -> u8 {
        (self.big_decompressed as u8) << 7 |
            (self.restricted as u8) << 6 |
            (self.compressed_size_present as u8)
    }
}

impl Mode for SimEA {
    fn length(decompressed_size: usize) -> usize {
        if decompressed_size > 0xFF_FF_FF {
            6
        } else {
            5
        }
    }

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let flags = Flags::read(reader.read_u8()?)?;
        let magic = reader.read_u8()?;
        if magic != header::MAGIC {
            return Err(RefPackError::BadMagic(magic));
        }
        //Inexplicably this weird three byte number is stored Big Endian
        let decompressed_length = if flags.big_decompressed {
            reader.read_u32::<BigEndian>()?
        } else {
            reader.read_u24::<BigEndian>()?
        };
        Ok(Header {
            decompressed_length,
            compressed_length: None,
        })
    }

    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()> {
        let big_decompressed = header.decompressed_length > 0xFF_FF_FF;
        writer.write_u8(Flags {
            big_decompressed,
            restricted: false,
            compressed_size_present: false,
        }.write())?;
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
        header.write::<SimEA>(&mut write_cur).unwrap();

        prop_assert_eq!(
            write_buf.len(),
            SimEA::length(header.decompressed_length as usize)
        );

        let mut read_cur = Cursor::new(&mut write_buf);
        let got = Header::read::<SimEA>(&mut read_cur).unwrap();

        prop_assert_eq!(header, got);
    }
}
