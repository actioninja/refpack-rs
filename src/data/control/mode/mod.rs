////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

pub mod reference;
pub mod simcity_4;

use std::io::{Read, Seek, Write};

pub use reference::Reference;
pub use simcity_4::Simcity4;

use crate::data::control::Command;
use crate::RefPackResult;

pub struct Sizes {
    position: (usize, usize),
}

/// Represents an encoding/decoding format for compression commands.
///
/// ## Key for description:
/// - Length: Length of the command in bytes
/// - Literal Range: Possible range of number of literal bytes to copy
/// - Literal Magic: Magic number offset for reading literal bytes
/// - Copy Length Range: Possible range of copy length
/// - Copy Length Magic: Magic number offset for reading copy length
/// - Position Range: Possible range of positions
/// - Position Magic: Magic number offset for reading position
/// - Layout: Bit layout of the command bytes
///
/// ## Key for layout
/// - 0 or 1: header
/// - P: Position
/// - L: Length
/// - B: Literal bytes Length
/// - -: Nibble Separator
/// - |: Byte Separator
///
/// Numbers are always "smashed" together into as small of a space as possible
/// EX: Getting the position from "0PPL-LLBB--PPPP-PPPP"
/// 1. mask first byte: `(byte0 & 0b0110_0000)` = 0PP0-0000
/// 2. shift left by 3: `(0PP0-0000 << 3)` = 0000-00PP--0000-0000
/// 3. OR with second:  `(0000-00PP--0000-0000 | 0000-0000--PPPP-PPPP)` = 0000-00PP--PPPP-PPPP
/// Another way to do this would be to first shift right by 5 and so on
/// To implement your own commands, do something TODO
pub trait Mode {
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Command>;
    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> RefPackResult<()>;
}
