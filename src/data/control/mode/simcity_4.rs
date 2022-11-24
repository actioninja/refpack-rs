////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::{Read, Seek, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::data::control::mode::reference::{
    read_literal, read_medium, read_short, read_stop, write_literal, write_medium, write_short,
    write_stop,
};
use crate::data::control::mode::Mode;
use crate::data::control::Command;
use crate::RefPackError;

/// Simcity 4 uses a nonstandard bit layout for long copy commands.
///
/// ## Commands
/// ### Long
/// - Length: 4
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 5-2047
/// - Length Magic: +5
/// - Position Range: 0-65535
/// - Position Magic: 0
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
            0xC0..=0xDF => {
                let second = reader.read_u8()?;
                let third = reader.read_u8()?;
                let fourth = reader.read_u8()?;

                let offset = (second as u32) << 8 | third as u32;
                let length = (((first & 0b0001_1100) as u16) << 6 | fourth as u16) + 5;
                let literal = first & 0b0000_0011;
                Ok(Command::Long {
                    offset,
                    length,
                    literal,
                })
            }
            0xE0..=0xFB => Ok(read_literal(first)),
            0xFC..=0xFF => Ok(read_stop(first)),
        }
    }

    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> Result<(), RefPackError> {
        match command {
            Command::Short {
                offset,
                length,
                literal,
            } => write_short(offset, length, literal, writer),
            Command::Medium {
                offset,
                length,
                literal,
            } => write_medium(offset, length, literal, writer),
            Command::Long {
                offset,
                length,
                literal,
            } => {
                let length_adjusted = length - 5;

                let first = 0b1100_0000u8
                    | ((length_adjusted >> 6) & 0b0001_1100) as u8
                    | literal & 0b0000_0011;
                let second = ((offset >> 8) & 0b1111_1111) as u8;
                let third = (offset & 0b1111_1111) as u8;
                let fourth = (length_adjusted & 0b1111_1111) as u8;

                writer.write_u8(first)?;
                writer.write_u8(second)?;
                writer.write_u8(third)?;
                writer.write_u8(fourth)?;
                Ok(())
            }
            Command::Literal(literal) => write_literal(literal, writer),
            Command::Stop(literal) => write_stop(literal, writer),
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::super::test::{
        generate_decoder_input, generate_decoder_input_with_ceiling, read_write_mode,
    };
    use super::*;

    #[proptest]
    fn symmetrical_read_write(
        #[strategy(generate_decoder_input(0b0000_0000, 0b1000_0000, 2))] short_in: Vec<u8>,
        #[strategy(generate_decoder_input(0b1000_0000, 0b1100_0000, 3))] medium_in: Vec<u8>,
        #[strategy(generate_decoder_input(0b1100_0000, 0b1110_0000, 4))] long_in: Vec<u8>,
        #[strategy(generate_decoder_input_with_ceiling(0b1110_0000, 0b1110_0000, 1, 27))]
        literal_in: Vec<u8>,
        #[strategy(generate_decoder_input(0b1111_1100, 0b1111_1100, 1))] stop_in: Vec<u8>,
    ) {
        let result = read_write_mode::<Simcity4>(short_in, medium_in, long_in, literal_in, stop_in);
        prop_assert!(result.is_ok(), "Inner: {}", result.unwrap_err());
    }
}
