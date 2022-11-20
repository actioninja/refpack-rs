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

pub struct Reference;

impl Mode for Reference {
    const LENGTH: usize = 4;

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
