////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::header::mode::Mode;
use crate::header::Header;
use crate::RefPackResult;

/// Earliest "Reference" implementation of header
///
/// ## Structure
/// - Little Endian u32: decompressed length
///
/// Nothing else
pub struct Reference {
    _private: (),
}

impl Mode for Reference {
    fn length(_decompressed_size: usize) -> usize {
        4
    }

    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header> {
        let decompressed_length = reader.read_u32::<LittleEndian>()?;
        Ok(Header {
            decompressed_length,
            compressed_length: None,
        })
    }

    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()> {
        writer.write_u32::<LittleEndian>(header.decompressed_length)?;
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
    fn symmetrical_read_write(header: Header) {
        let expected = Header {
            decompressed_length: header.decompressed_length,
            compressed_length: None,
        };

        let mut write_buf = vec![];
        let mut write_cur = Cursor::new(&mut write_buf);
        header.write::<Reference>(&mut write_cur).unwrap();
        let mut read_cur = Cursor::new(&mut write_buf);
        let got = Header::read::<Reference>(&mut read_cur).unwrap();

        prop_assert_eq!(expected, got);
    }

    #[test]
    fn reads_correctly() {
        let mut buf = vec![255u8, 0x00, 0x00, 0x00];
        let mut cur = Cursor::new(&mut buf);
        let header = Header::read::<Reference>(&mut cur).unwrap();
        assert_eq!(header.decompressed_length, 255);
    }

    #[test]
    fn writes_correctly() {
        let header = Header {
            decompressed_length: 255,
            compressed_length: None,
        };
        let mut buf = vec![];
        let mut cur = Cursor::new(&mut buf);
        header.write::<Reference>(&mut cur).unwrap();
        assert_eq!(buf, vec![255u8, 0x00, 0x00, 0x00]);
    }
}
