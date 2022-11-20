////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use byteorder::ReadBytesExt;

use crate::data::control::mode::reference::{read_literal, read_medium, read_short, read_stop};
use crate::data::control::mode::Mode;
use crate::data::control::Command;
use crate::RefPackError;

/// Simcity 4 uses a nonstandard bit layout for long copy commands.
///
/// ## Commands
/// ### Short
/// - Length: 4
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 5-2047
/// - Length Magic: +3
/// - Position Range: 1-65535
/// - Position Magic: +1
/// - Layout: 110L-LLBB|PPPP-PPPP|PPPP-PPPP|LLLL-LLLL
///
/// All remaining formats are identical to [Reference], see [Reference] as well for the key for the
/// layout reference
pub struct Simcity4;

impl Mode for Simcity4 {
    fn read<R: Read + Seek>(reader: &mut R) -> Result<Command, RefPackError> {
        let first = reader.read_u8()?;

        match first {
            0x00..=0x7F => read_short(first, reader),
            0x80..=0xBF => read_medium(first, reader),
            0xC0..=0xDF => todo!(),
            0xE0..=0xFB => Ok(read_literal(first)),
            0xFC..=0xFF => Ok(read_stop(first)),
        }
    }

    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> Result<(), RefPackError> {
        todo!()
    }
}
