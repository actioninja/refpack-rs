////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! possible modes to use for header encoding and decoding
mod maxis;
mod reference;

use std::io::{Read, Seek, Write};

pub use maxis::Maxis;
pub use reference::Reference;

use crate::header::Header;
use crate::RefPackResult;

/// Represents a read and write format for a Header
/// todo: document this better
pub trait Mode {
    /// Length of the header, used by some parsing
    const LENGTH: usize;

    /// Reads from a `Read + Seek` reader and attempts to parse a header at the current position.
    /// # Errors
    /// Returns [RefPackError::BadMagic](crate::RefPackError::BadMagic) if the in data has invalid
    /// magic numbers
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to read data
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header>;
    /// Writes to a `Write + Seek` writer and attempts to encode a header at the current position.
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to write data
    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()>;
}
