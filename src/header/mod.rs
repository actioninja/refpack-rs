////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use crate::header::mode::Mode;
use crate::RefPackResult;

pub mod mode;

pub const MAGIC: u16 = 0x10FB;

pub struct Header {
    pub decompressed_length: u32,
    pub compressed_length: Option<u32>,
}

impl Header {
    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> RefPackResult<Header> {
        M::read(reader)
    }

    pub fn write<M: Mode>(self, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        M::write(self, writer)
    }
}
