////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! possible modes to use for header encoding and decoding
mod maxis;
mod reference;
mod sim_ea;

use std::io::{Read, Seek, Write};

pub use maxis::Maxis;
pub use reference::Reference;
pub use sim_ea::SimEA;

use crate::RefPackResult;
use crate::header::Header;

/// Represents a read and write format for a Header
///
/// This trait is entirely statically resolved and should only ever be
/// implemented on structs which cannot be constructed. It has only associated
/// functions, no methods, and only ever is referenced via generic arguments.
///
/// To implement your own commands, implement `Mode` on to a unit struct or
/// unconstructable struct with one private member and no new method. `read` and
/// `write` should be symmetrical, and a value fed in to read and then back out
/// of write should yield the same result.
pub trait Mode {
    /// Length of the header, used by some parsing
    fn length(decompressed_size: usize) -> usize;

    /// Reads from a `Read + Seek` reader and attempts to parse a header at the
    /// current position.
    ///
    /// # Errors
    /// - [RefPackError::BadMagic]: Magic number failed verification
    /// - [RefPackError::Io]: Generic IO Error occurred during read
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Header>;

    /// Writes to a `Write + Seek` writer and attempts to encode a header at the
    /// current position.
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO Error occurred during write
    fn write<W: Write + Seek>(header: Header, writer: &mut W) -> RefPackResult<()>;
}
