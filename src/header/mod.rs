////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Module for things relating to the header of the data which include decompressed length,
//! sometimes flags or a magic number, and sometimes compressed length.

use std::io::{Read, Seek, Write};

#[cfg(test)]
use test_strategy::Arbitrary;

use crate::header::mode::Mode;
use crate::RefPackResult;

pub mod mode;

/// Magic number in the header. Literal `10 FB` sequence in stream, read and write as Big Endian.
/// There seems to be some debate as to whether this is intended to be `0xFB10` and the number is
/// stored in Little Endian, but this is an encoding difference and I decided to keep it as the
/// literal stream sequence
///
/// Some implementations seem to use these two bytes as a flags field
pub const MAGIC: u16 = 0x10FB;

/// Internal struct to represent a decoded header
#[derive(Eq, PartialEq, Debug, Default, Copy, Clone)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Header {
    pub decompressed_length: u32,
    pub compressed_length: Option<u32>,
}

impl Header {
    /// # Errors
    /// Returns [RefPackError::BadMagic](crate::RefPackError::BadMagic) if the read failed due to
    /// an invalid magic number in the header
    /// Returns [RefPackError::Io](crate::RefPackError::BadMagic) if the read failed due to a
    /// generic IO Error
    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> RefPackResult<Header> {
        M::read(reader)
    }

    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::BadMagic) if the write failed due to a
    /// generic IO Error
    pub fn write<M: Mode>(self, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        M::write(self, writer)
    }
}
