////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////
use std::io::{Read, Seek, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::data::control::mode::{Mode, Sizes};
use crate::data::control::Command;
use crate::RefPackResult;

/// Reference encode/decode format used by the vast majority of RefPack implementations.
/// Dates back to the original reference implementation by Frank Barchard
///
/// ## Split Numbers
/// Numbers are always "smashed" together into as small of a space as possible
/// EX: Getting the position from "`0PPL-LLBB--PPPP-PPPP`"
/// 1. mask first byte: `(byte0 & 0b0110_0000)` = `0PP0-0000`
/// 2. shift left by 3: `(0PP0-0000 << 3)` = `0000-00PP--0000-0000`
/// 3. OR with second:  `(0000-00PP--0000-0000 | 0000-0000--PPPP-PPPP)` = `0000-00PP--PPPP-PPPP`
/// Another way to do this would be to first shift right by 5 and so on
///
/// ## Key for description:
/// - Len: Length of the command in bytes
/// - Literal: Possible range of number of literal bytes to copy
/// - Length: Possible range of copy length
/// - Position Range: Possible range of positions
/// - Layout: Bit layout of the command bytes
///
/// ### Key for layout
/// - `0` or `1`: header
/// - `P`: Position
/// - `L`: Length
/// - `B`: Literal bytes Length
/// - `-`: Nibble Separator
/// - `:`: Byte Separator
///
/// ## Commands
///
/// | Command | Len | Literal | Length | Position | Layout |
/// |---------|-----|---------|--------|----------|--------|
/// | Short   | 2   | (0-3) +0 | (3-10) +3 | (1-1023) +1 | `0PPL-LLBB:PPPP-PPPP` |
/// | Medium  | 3   | (0-3) +0 | (4-67) +4 | (1-16383) +1 | `10LL-LLLL:BBPP-PPPP:PPPP-PPPP` |
/// | Long    | 4   | (0-3) +0 | (5-1028) +5 | (1-131072) +1 | `110P-LLBB:PPPP-PPPP:PPPP-PPPP:LLLL-LLLL` |
/// | Literal | 1   | (4-112) +4 | 0 | 0 | `111B-BBBB` |
/// | Stop    | 1   | (0-3) +0 | 0 | 0 | `1111-11PP` |
///
/// ### Extra Note on Literal Commands
///
/// Literal is a special command that has differently encoded values.
/// Due to that other commands can store up to 3 literal bytes, you can encode any sequence of literal
/// bytes by writing a multiple of 4 bytes via a literal command and then split off the remainder
/// in to the next copy command.
/// This means that literal commands actually only _need_ to encode multiples of 4. As a result,
/// literal commands first shift their literal length to the right by 2, making the range of 4-112
/// stored in the space of 0-27, only 5 bits for close to what you would ordinarily get from 7.
///
/// Clever, huh?
///
/// One extra unusual detail is that despite that it seems like the cap from the bitshift should be 128,
/// in practice it's limited to 112. The way the original reference implementation worked was to
/// read the huffman encoded headers via just checking if the first byte read with within certain
/// decimal ranges. `refpack` implements this similarly for maximum compatibility. If the first byte
/// read is within `252..=255`, it's interpreted as a stopcode. The highest allowed values of 112
/// is encoded as `0b1111_1011` which is `251` exactly. Any higher of a value would start seeping
/// in to the stopcode range.
pub struct Reference;

impl Reference {
    /// Reference read implementation of long codes. See [Reference] for specification
    ///
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if it fails to get the remaining one byte from the `reader`.
    #[inline(always)]
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

    /// Reference read implementation of medium copy commands. See [Reference] for specification
    ///
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if it fails to get the remaining two bytes from the `reader`.
    #[inline(always)]
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

    /// Reference read implementation of long copy commands. See [Reference] for specification
    ///
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if it fails to get the remaining three bytes from the `reader`.
    #[inline(always)]
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

    /// Reference read implementation of literal commands. See [Reference] for specification
    #[inline(always)]
    #[must_use]
    pub fn read_literal(first: u8) -> Command {
        Command::Literal(((first & 0b0001_1111) << 2) + 4)
    }

    /// Reference read implementation of stopcodes. See [Reference] for specification
    #[inline(always)]
    #[must_use]
    pub fn read_stop(first: u8) -> Command {
        Command::Stop(first & 0b0000_0011)
    }

    /// Reference write implementation of short copy commands. See [Reference] for specification
    /// # Errors
    /// returns [RefPackError::Io](crate::RefPackError::Io) if it fails to write to the writer stream
    #[inline]
    pub fn write_short(
        offset: u16,
        length: u8,
        literal: u8,
        writer: &mut (impl Write + Seek),
    ) -> RefPackResult<()> {
        let length_adjusted = length - 3;
        let offset_adjusted = offset - 1;

        let first = ((offset_adjusted & 0b0000_0011_0000_0000) >> 3) as u8
            | (length_adjusted & 0b0000_0111) << 2
            | literal & 0b0000_0011;
        let second = (offset_adjusted & 0b0000_0000_1111_1111) as u8;

        writer.write_u8(first)?;
        writer.write_u8(second)?;
        Ok(())
    }

    /// Reference write implementation of medium copy commands. See [Reference] for specification
    /// # Errors
    /// returns [RefPackError::Io](crate::RefPackError::Io) if it fails to write to the writer stream
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

    /// Reference write implementation of long copy commands. See [Reference] for specification
    /// # Errors
    /// returns [RefPackError::Io](crate::RefPackError::Io) if it fails to write to the writer stream
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

    /// Reference write implementation of literal commands. See [Reference] for specification
    /// # Errors
    /// returns [RefPackError::Io](crate::RefPackError::Io) if it fails to write to the writer stream
    #[inline]
    pub fn write_literal(literal: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        let adjusted = (literal - 4) >> 2;
        let out = 0b1110_0000 | (adjusted & 0b0001_1111);
        writer.write_u8(out)?;
        Ok(())
    }

    /// Reference write implementation of stopcode. See [Reference] for specification
    /// # Errors
    /// returns [RefPackError::Io](crate::RefPackError::Io) if it fails to write to the writer stream
    #[inline]
    pub fn write_stop(number: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        let out = 0b1111_1100 | (number & 0b0000_0011);
        writer.write_u8(out)?;
        Ok(())
    }
}

impl Mode for Reference {
    const SIZES: Sizes = Sizes {
        literal: (4, 112),
        copy_literal: (0, 3),
        short_offset: (1, 1_023),
        short_length: (3, 10),
        medium_offset: (1, 16_383),
        medium_length: (4, 67),
        long_offset: (1, 131_072),
        long_length: (5, 1028),
    };

    #[inline(always)]
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Command> {
        let first = reader.read_u8()?;

        match first {
            0x00..=0x7F => Reference::read_short(first, reader),
            0x80..=0xBF => Reference::read_medium(first, reader),
            0xC0..=0xDF => Reference::read_long(first, reader),
            0xE0..=0xFB => Ok(Reference::read_literal(first)),
            0xFC..=0xFF => Ok(Reference::read_stop(first)),
        }
    }

    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> RefPackResult<()> {
        match command {
            Command::Short {
                offset,
                length,
                literal,
            } => Reference::write_short(offset, length, literal, writer),
            Command::Medium {
                offset,
                length,
                literal,
            } => Reference::write_medium(offset, length, literal, writer),
            Command::Long {
                offset,
                length,
                literal,
            } => Reference::write_long(offset, length, literal, writer),
            Command::Literal(literal) => Reference::write_literal(literal, writer),
            Command::Stop(literal) => Reference::write_stop(literal, writer),
        }
    }
}

#[cfg(test)]
mod test {
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
        let result =
            read_write_mode::<Reference>(short_in, medium_in, long_in, literal_in, stop_in);
        prop_assert!(result.is_ok(), "\nInner: {}\n", result.unwrap_err());
    }
}
