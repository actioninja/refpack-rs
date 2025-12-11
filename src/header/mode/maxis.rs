////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::cmp::min;
use std::io::{Read, Seek, Write};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::header::Header;
use crate::header::mode::Mode;
use crate::{RefPackError, RefPackResult, header};

/// Header used by many Maxis and SimEA games
///
/// ## Structure
/// - Little Endian u32: Compressed length
/// - u8: Flags field; flags are unknown, and in all known cases is `0x10`
/// - Magic Number: 0xFB
/// - Big Endian u24/u32: Decompressed Length
pub enum Maxis {}

pub const FLAGS: u8 = 0x10;

impl Mode for Maxis {
    fn length(_decompressed_size: usize) -> usize {
        9
    }

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let compressed_length_prewrap = reader.read_u32::<LittleEndian>()?;
        let compressed_length = if compressed_length_prewrap == 0 {
            None
        } else {
            Some(compressed_length_prewrap)
        };
        let flags = reader.read_u8()?;
        if flags != FLAGS {
            return Err(RefPackError::BadFlags(flags));
        }
        let magic = reader.read_u8()?;
        if magic != header::MAGIC {
            return Err(RefPackError::BadMagic(magic));
        }
        // Inexplicably this weird three byte number is stored Big Endian
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
        // This is only ever used to create a default size for the decompression buffer,
        // so I believe this won't cause issues? Even official decompression seems to just ignore this
        writer.write_u24::<BigEndian>(min(
            header.decompressed_length,
            0b1111_1111_1111_1111_1111_1111,
        ))?;
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

    #[test]
    fn reads_correctly() {
        let mut buf = vec![255, 0, 0, 0, FLAGS, header::MAGIC, 0, 0, 255];
        let mut cur = Cursor::new(&mut buf);
        let got = Header::read::<Maxis>(&mut cur).unwrap();
        let want = Header {
            decompressed_length: 255,
            compressed_length: Some(255),
        };
        assert_eq!(got, want);
    }

    #[test]
    fn writes_correctly() {
        let header = Header {
            decompressed_length: 255,
            compressed_length: Some(255),
        };
        let mut buf = vec![];
        let mut cur = Cursor::new(&mut buf);
        header.write::<Maxis>(&mut cur).unwrap();
        let want = vec![255, 0, 0, 0, FLAGS, header::MAGIC, 0, 0, 255];
        assert_eq!(buf, want);
    }

    #[test]
    fn rejects_bad_flags() {
        let mut buf = vec![0, 0, 0, 0, 0x50, 0, 0, 0, 0];
        let mut cur = Cursor::new(&mut buf);
        let err = Header::read::<Maxis>(&mut cur).unwrap_err();
        assert_eq!(err.to_string(), RefPackError::BadFlags(0x50).to_string());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = vec![0, 0, 0, 0, FLAGS, 0x50, 0, 0, 0];
        let mut cur = Cursor::new(&mut buf);
        let err = Header::read::<Maxis>(&mut cur).unwrap_err();
        assert_eq!(err.to_string(), RefPackError::BadMagic(0x50).to_string());
    }
}
