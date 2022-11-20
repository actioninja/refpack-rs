////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

mod maxis;
mod reference;

use std::io::{Read, Seek, Write};

pub use maxis::Maxis;
pub use reference::Reference;

use crate::header::Header;
use crate::RefPackResult;

/// Represents a read and write format for a Header
///
pub trait Mode {
    const LENGTH: usize;

    /// Function for reading from a reader and parsing in to a header
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header>;
    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()>;
}
