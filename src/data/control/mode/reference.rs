////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::hash::Hasher;
use std::io::{Read, Seek, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::data::control::mode::Mode;
use crate::data::control::Command;
use crate::RefPackResult;

/// Standard encode/decode format used by the vast majority of RefPack implementations
/// Dates back to the original reference implementation by Frank Barchard
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
///
/// ## Commands
/// ### Short
/// - Length: 2
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 3-10
/// - Length Magic: +3
/// - Position Range: 1-1023
/// - Position Magic: +1
/// - Layout: 0PPL-LLBB|PPPP-PPPP
/// ### Medium
/// - Length: 3
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 4-67
/// - Length Magic: +4
/// - Position Range: 1-16383
/// - Position Magic: +1
/// - Layout: 10LL-LLLL|BBPP-PPPP|PPPP-PPPP
/// ### Long
/// - Length: 4
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 5-1028
/// - Length Magic: +5
/// - Position Range: 1-131072
/// - Position Magic: +1
/// - Layout: 110P-LLBB|PPPP-PPPP|PPPP-PPPP|LLLL-LLLL
/// ### Literal
/// - Length: 1
/// - Literal Range: 4-112; limited precision
/// - Literal Magic: +4
/// - Length Range: 0
/// - Length Magic: 0
/// - Position Range: 0
/// - Position Magic: 0
/// - Layout: 111B-BBBB
/// - Notes: Magic bit shift happens here for unclear reasons, effectively multiplying
///   stored number by 4.
/// - Weird detail of how it's read; range is in fact capped at 112 even though it seems like
///   it could be higher. The original program read by range of control as an absolute
///   number, meaning that if the number was higher than 27, it would instead be read as a
///   stopcode. Don't ask me, it's in the reference implementation and persisted.
/// ### Stop
/// - Length: 1
/// - Literal Range: 0-3
/// - Literal Magic: 0
/// - Length Range: 0
/// - Length Magic: 0
/// - Position Range: 0
/// - Position Magic: 0
/// - Layout: 1111-11PP
pub struct Reference;

/// Standard read implementation of long codes. See [Standard] for specification
///
/// # Errors
/// Returns `std::io::Error` if it fails to get the remaining one byte from the `reader`.
#[inline]
pub fn read_short(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Command> {
    let byte1 = first as usize;
    let byte2: usize = reader.read_u8()?.into();

    let offset = ((((byte1 & 0b0110_0000) << 3) | byte2) + 1) as u16;
    let length = (((byte1 & 0b0001_1100) >> 2) + 3) as u8;
    let literal = (byte1 & 0b0000_0011) as u8;

    Ok(Command::Short {
        offset,
        length,
        literal,
    })
}

/// Standard read implementation of medium codes. See [Standard] for specification
///
/// # Errors
/// Returns `std::io::Error` if it fails to get the remaining two bytes from the `reader`.
#[inline]
pub fn read_medium(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Command> {
    let byte1: usize = first as usize;
    let byte2: usize = reader.read_u8()?.into();
    let byte3: usize = reader.read_u8()?.into();

    let offset = ((((byte2 & 0b0011_1111) << 8) | byte3) + 1) as u16;
    let length = ((byte1 & 0b0011_1111) + 4) as u8;
    let literal = ((byte2 & 0b1100_0000) >> 6) as u8;

    Ok(Command::Medium {
        offset,
        length,
        literal,
    })
}

/// Standard read implementation of long codes. See [Standard] for specification
///
/// # Errors
/// Returns `std::io::Error` if it fails to get the remaining three bytes from the `reader`.
#[inline]
pub fn read_long(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Command> {
    let byte1: usize = first as usize;
    let byte2: usize = reader.read_u8()?.into();
    let byte3: usize = reader.read_u8()?.into();
    let byte4: usize = reader.read_u8()?.into();

    let offset = ((((byte1 & 0b0001_0000) << 12) | (byte2 << 8) | byte3) + 1) as u32;
    let length = ((((byte1 & 0b0000_1100) << 6) | byte4) + 5) as u16;

    let literal = (byte1 & 0b0000_0011) as u8;

    Ok(Command::Long {
        offset,
        length,
        literal,
    })
}

/// Standard read implementation of literals. See [Standard] for specification
#[inline]
#[must_use]
pub fn read_literal(first: u8) -> Command {
    Command::Literal(((first & 0b0001_1111) << 2) + 4)
}

/// Standard read implementation of stopcodes. See [Standard] for specification
#[inline]
#[must_use]
pub fn read_stop(first: u8) -> Command {
    Command::Stop(first & 0b0000_0011)
}

#[inline]
pub fn write_short(
    offset: u16,
    length: u8,
    literal: u8,
    writer: &mut (impl Write + Seek),
) -> RefPackResult<()> {
    let length_adjusted = length - 3;
    let offset_adjusted = offset - 1;

    let first = (((offset_adjusted & 0b0000_0011_0000_0000) >> 3) as u8
        | (length_adjusted & 0b0000_0111) << 2
        | literal & 0b0000_0011) as u8;
    let second = (offset_adjusted & 0b0000_0000_1111_1111) as u8;

    writer.write_u8(first)?;
    writer.write_u8(second)?;
    Ok(())
}

#[inline]
pub fn write_medium(
    offset: u16,
    length: u8,
    literal: u8,
    writer: &mut (impl Write + Seek),
) -> RefPackResult<()> {
    let length_adjusted = length - 4;
    let offset_adjusted = offset - 1;

    let first = 0b1000_0000 | length_adjusted & 0b0011_1111;
    let second = (literal & 0b0000_0011) << 6 | (offset_adjusted >> 8) as u8;
    let third = (offset_adjusted & 0b0000_0000_1111_1111) as u8;

    writer.write_u8(first)?;
    writer.write_u8(second)?;
    writer.write_u8(third)?;

    Ok(())
}

#[inline]
pub fn write_long(
    offset: u32,
    length: u16,
    literal: u8,
    writer: &mut (impl Write + Seek),
) -> RefPackResult<()> {
    let length_adjusted = length - 5;
    let offset_adjusted = offset - 1;

    let first = 0b1100_0000u8
        | ((offset_adjusted >> 12) & 0b0001_0000) as u8
        | ((length_adjusted >> 6) & 0b0000_1100) as u8
        | literal & 0b0000_0011;
    let second = ((offset_adjusted >> 8) & 0b1111_1111) as u8;
    let third = (offset_adjusted & 0b1111_1111) as u8;
    let fourth = (length_adjusted & 0b1111_1111) as u8;

    writer.write_u8(first)?;
    writer.write_u8(second)?;
    writer.write_u8(third)?;
    writer.write_u8(fourth)?;

    Ok(())
}

#[inline]
pub fn write_literal(literal: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
    let adjusted = (literal - 4) >> 2;
    let out = 0b1110_0000 | (adjusted & 0b0001_1111);
    writer.write_u8(out)?;
    Ok(())
}

#[inline]
pub fn write_stop(number: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
    let out = 0b1111_1100 | (number & 0b0000_0011);
    writer.write_u8(out)?;
    Ok(())
}

impl Mode for Reference {
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Command> {
        let first = reader.read_u8()?;

        match first {
            0x00..=0x7F => read_short(first, reader),
            0x80..=0xBF => read_medium(first, reader),
            0xC0..=0xDF => read_long(first, reader),
            0xE0..=0xFB => Ok(read_literal(first)),
            0xFC..=0xFF => Ok(read_stop(first)),
        }
    }

    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> RefPackResult<()> {
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
            } => write_long(offset, length, literal, writer),
            Command::Literal(literal) => write_literal(literal, writer),
            Command::Stop(literal) => write_stop(literal, writer),
        }
    }
}
