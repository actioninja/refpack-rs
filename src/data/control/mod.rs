////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! control codes utilized by compression and decompression

#[cfg(test)]
mod iterator;

use std::io::{Read, Seek, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};
#[cfg(test)]
use proptest::collection::{size_range, vec};
#[cfg(test)]
use proptest::prelude::*;

use crate::{RefPackError, RefPackResult};

/// minimum value of the literal length in a literal command
pub const LITERAL_MIN: u8 = 4;

/// maximum value of the literal length in a literal command
pub const LITERAL_MAX: u8 = 112;

/// "Real" maximum of literal value in a literal command once encoded
///
/// Literal commands encode their value in a special limited precision
/// format
///
/// Equivalent to `0`, written as an expression to convey the relation
pub const LITERAL_EFFECTIVE_MIN: u8 = (LITERAL_MIN - 4) / 4;

/// "Real" maximum of literal value in a literal command once encoded
///
/// Literal commands encode their value in a special limited precision
/// format
///
/// Equivalent to `27`, written as an expression to convey the relation
pub const LITERAL_EFFECTIVE_MAX: u8 = (LITERAL_MAX - 4) / 4;

/// minimum value of the literal length in a non-literal command
pub const COPY_LITERAL_MIN: u8 = 0;

/// maximum value of the literal length in a non-literal command
pub const COPY_LITERAL_MAX: u8 = 3;

/// minimum offset distance for a short command
pub const SHORT_OFFSET_MIN: u32 = 1;

/// maximum offset distance for a short command
pub const SHORT_OFFSET_MAX: u32 = 1_024;

/// minimum length for a short command
pub const SHORT_LENGTH_MIN: u16 = 3;

/// maximum length for a short command
pub const SHORT_LENGTH_MAX: u16 = 10;

/// minimum offset distance for a medium command
pub const MEDIUM_OFFSET_MIN: u32 = 1;

/// maximum offset distance for a medium command
pub const MEDIUM_OFFSET_MAX: u32 = 16_384;

/// minimum length for a medium command
pub const MEDIUM_LENGTH_MIN: u16 = 4;

/// maximum length for a medium command
pub const MEDIUM_LENGTH_MAX: u16 = 67;

/// minimum offset distance for a long command
pub const LONG_OFFSET_MIN: u32 = 1;

/// maximum offset distance for a long command
pub const LONG_OFFSET_MAX: u32 = 131_072;

/// minimum length for a long command
pub const LONG_LENGTH_MIN: u16 = 5;

/// maximum length for a long command
pub const LONG_LENGTH_MAX: u16 = 1_028;

/// Possible actual control code values
///
/// ## Split Numbers
/// Numbers are always "smashed" together into as small of a space as possible
/// EX: Getting the position from "`0PPL-LLBB--PPPP-PPPP`"
/// 1. mask first byte: `(byte0 & 0b0110_0000)` = `0PP0-0000`
/// 2. shift left by 3: `(0PP0-0000 << 3)` = `0000-00PP--0000-0000`
/// 3. OR with second:  `(0000-00PP--0000-0000 | 0000-0000--PPPP-PPPP)` =
///    `0000-00PP--PPPP-PPPP` Another way to do this would be to first shift right
///    by 5 and so on
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
/// | Command | Len | Literal      | Length        | Position        | Layout                                    |
/// |---------|-----|--------------|---------------|-----------------|-------------------------------------------|
/// | Short   | 2   | (0..=3) +0   | (3..=10) +3   | (1..=1024) +1   | `0PPL-LLBB:PPPP-PPPP`                     |
/// | Medium  | 3   | (0..=3) +0   | (4..=67) +4   | (1..=16384) +1  | `10LL-LLLL:BBPP-PPPP:PPPP-PPPP`           |
/// | Long    | 4   | (0..=3) +0   | (5..=1028) +5 | (1..=131072) +1 | `110P-LLBB:PPPP-PPPP:PPPP-PPPP:LLLL-LLLL` |
/// | Literal | 1   | (4..=112) +4 | 0             | 0               | `111B-BBBB`                               |
/// | Stop    | 1   | (0..=3) +0   | 0             | 0               | `1111-11BB`                               |
///
/// ### Extra Note on Literal Commands
///
/// Literal is a special command that has differently encoded values.
///
/// While the practical range is 4-112, literal values must always be an even
/// multiple of 4. Before being encoded, the value is first decreased by 4 then
/// shifted right by 2
///
/// #### Why
///
/// Because all other codes can have an up to 3 byte literal payload, this means
/// that the number of literals can be stored as (length / 4) + (length % 4).
/// When a number is an even multiple of a power of 2, it can be encoded in less
/// bits by bitshifting it before encoding and decoding. This lets an effective
/// range of 0-112 for the length of literal commands in only 5 bits of data,
/// since the first 3 bits are the huffman header.
///
/// If this is unclear, here's the process written out:
///
/// We want to encode a literal length of 97
///
/// 1. take `97 % 4` to get the "leftover" length - this will be used in next
///    command following the literal
/// 2. take `(97 - 4) >> 2` to get the value to encode into the literal value
/// 3. create a literal command with the result from 2, take that number of
///    literals from the current literal buffer and write to stream
/// 4. in the next command, encode the leftover literal value from 1
///
/// One extra unusual detail is that despite that it seems like te cap from the
/// bitshift should be 128, in practice it's limited to 112. The way the
/// original reference implementation worked was to read the huffman encoded
/// headers via just checking if the first byte read with within certain decimal
/// ranges. `refpack` implements this similarly for maximum compatibility. If
/// the first byte read is within `252..=255`, it's interpreted as a stopcode.
/// The highest allowed values of 112 is encoded as `0b1111_1011` which is `251`
/// exactly. Any higher of a value would start seeping in to the stopcode range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub offset: u32,
    pub length: u16,
    pub literal: u8,
    pub kind: CommandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CommandKind {
    Short,
    Medium,
    Long,
    Literal,
    Stop,
}

impl Command {
    /// Create a new copy type `Command` struct.
    /// # Panics
    /// Panics if you attempt to create an invalid Command in some way
    #[must_use]
    pub fn new(offset: u32, length: u16, literal: u8) -> Self {
        assert!(
            literal <= COPY_LITERAL_MAX,
            "Literal length must be less than or equal to {COPY_LITERAL_MAX} for commands \
             ({literal})"
        );

        if offset > LONG_OFFSET_MAX || length > LONG_LENGTH_MAX {
            panic!(
                "Invalid offset or length (Maximum offset {LONG_OFFSET_MAX}, got {offset}) \
                 (Maximum length {LONG_LENGTH_MAX}, got {length})"
            );
        } else if offset > MEDIUM_OFFSET_MAX || length > MEDIUM_LENGTH_MAX {
            assert!(
                length >= LONG_LENGTH_MIN,
                "Length must be greater than or equal to {LONG_LENGTH_MIN} for long commands \
                 (Length: {length}) (Offset: {offset})"
            );
            Self {
                offset,
                length,
                literal,
                kind: CommandKind::Long,
            }
        } else if offset > SHORT_OFFSET_MAX || length > SHORT_LENGTH_MAX {
            assert!(
                length >= MEDIUM_LENGTH_MIN,
                "Length must be greater than or equal to {MEDIUM_LENGTH_MIN} for medium commands \
                 (Length: {length}) (Offset: {offset})"
            );
            Self {
                offset,
                length,
                literal,
                kind: CommandKind::Medium,
            }
        } else {
            Self {
                offset,
                length,
                literal,
                kind: CommandKind::Short,
            }
        }
    }

    /// Creates a new literal command block
    /// # Panics
    /// Panics if you attempt to create too long of a literal command. This
    /// depends on control mode used.
    #[must_use]
    pub fn new_literal(length: u8) -> Self {
        assert!(
            length <= LITERAL_MAX,
            "Literal received too long of a literal length (max {LITERAL_MAX}, got {length})"
        );
        Self {
            offset: 0,
            length: 0,
            literal: length,
            kind: CommandKind::Literal,
        }
    }

    /// Creates a new stopcode command block
    /// # Panics
    /// Panics if you attempt to create too long of a stop code. This depends on
    /// control mode used.
    #[must_use]
    pub fn new_stop(literal_length: usize) -> Self {
        assert!(
            literal_length <= COPY_LITERAL_MAX as usize,
            "Stopcode recieved too long of a literal length (max {COPY_LITERAL_MAX}, got \
             {literal_length})"
        );
        Self {
            offset: 0,
            length: 0,
            literal: literal_length as u8,
            kind: CommandKind::Stop,
        }
    }

    #[inline(always)]
    pub fn new_stop_unchecked(literal_length: u8) -> Self {
        Self {
            offset: 0,
            length: 0,
            literal: literal_length,
            kind: CommandKind::Stop,
        }
    }

    /// Get number of literal bytes on the command, if they have any
    /// Returns `None` if the length is 0
    #[must_use]
    pub fn num_of_literal(self) -> Option<usize> {
        if self.literal == 0 {
            None
        } else {
            Some(self.literal as usize)
        }
    }

    /// Get the offset and length of a copy command as a `(usize, usize)` tuple.
    ///
    /// Returns `None` if `self` is not a copy command.
    #[must_use]
    pub fn offset_copy(self) -> Option<(usize, usize)> {
        match self.kind {
            CommandKind::Short | CommandKind::Medium | CommandKind::Long => {
                Some((self.offset as usize, self.length as usize))
            }
            _ => None,
        }
    }

    /// Returns true if the command is a stopcode, false if it is not.
    #[must_use]
    pub fn is_stop(self) -> bool {
        self.kind == CommandKind::Stop
    }

    /// Reference read implementation of short copy commands. See structure
    /// definition for documentation
    ///
    /// # Errors
    /// - [RefPackError::Io]: Failed to get remaining single byte from reader
    #[inline(always)]
    pub fn read_short(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Self> {
        let byte1 = first as usize;
        let byte2: usize = reader.read_u8()?.into();

        let offset = ((((byte1 & 0b0110_0000) << 3) | byte2) + 1) as u32;
        let length = (((byte1 & 0b0001_1100) >> 2) + 3) as u16;
        let literal = (byte1 & 0b0000_0011) as u8;

        Ok(Self {
            offset,
            length,
            literal,
            kind: CommandKind::Short,
        })
    }

    /// Reference read implementation of medium copy commands. See struct
    /// definition for documentation
    ///
    /// # Errors
    /// - [RefPackError::Io]: Failed to get remaining two bytes from reader
    #[inline(always)]
    pub fn read_medium(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Self> {
        let byte1: usize = first as usize;
        let byte2: usize = reader.read_u8()?.into();
        let byte3: usize = reader.read_u8()?.into();

        let offset = ((((byte2 & 0b0011_1111) << 8) | byte3) + 1) as u32;
        let length = ((byte1 & 0b0011_1111) + 4) as u16;
        let literal = ((byte2 & 0b1100_0000) >> 6) as u8;

        Ok(Self {
            offset,
            length,
            literal,
            kind: CommandKind::Medium,
        })
    }

    /// Reference read implementation of long commands. See struct definition
    /// for documentation
    ///
    /// # Errors
    /// - [RefPackError::Io]: Failed to get remaining three bytes from the reader
    #[inline(always)]
    pub fn read_long(first: u8, reader: &mut (impl Read + Seek)) -> RefPackResult<Self> {
        let byte1: usize = first as usize;
        let byte2: usize = reader.read_u8()?.into();
        let byte3: usize = reader.read_u8()?.into();
        let byte4: usize = reader.read_u8()?.into();

        let offset = ((((byte1 & 0b0001_0000) << 12) | (byte2 << 8) | byte3) + 1) as u32;
        let length = ((((byte1 & 0b0000_1100) << 6) | byte4) + 5) as u16;

        let literal = (byte1 & 0b0000_0011) as u8;

        Ok(Self {
            offset,
            length,
            literal,
            kind: CommandKind::Long,
        })
    }

    /// Reference read implementation of literal commands. See struct definition
    /// for documentation
    #[inline(always)]
    #[must_use]
    pub fn read_literal(first: u8) -> Self {
        Self {
            offset: 0,
            length: 0,
            literal: ((first & 0b0001_1111) << 2) + 4,
            kind: CommandKind::Literal,
        }
    }

    /// Reference read implementation of literal commands. See struct definition
    /// for documentation
    #[inline(always)]
    #[must_use]
    pub fn read_stop(first: u8) -> Self {
        Self::new_stop_unchecked(first & 0b0000_0011)
    }

    /// Reads and decodes a command from a `Read + Seek` reader.
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to read
    ///   data
    #[inline(always)]
    pub fn read(reader: &mut (impl Read + Seek)) -> RefPackResult<Self> {
        let first = reader.read_u8()?;

        match first {
            0x00..=0x7F => Self::read_short(first, reader),
            0x80..=0xBF => Self::read_medium(first, reader),
            0xC0..=0xDF => Self::read_long(first, reader),
            0xE0..=0xFB => Ok(Self::read_literal(first)),
            0xFC..=0xFF => Ok(Self::read_stop(first)),
        }
    }

    /// Reference write implementation of short copy commands. See struct
    /// definition for specification
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
    #[inline]
    pub fn write_short(
        offset: u32,
        length: u16,
        literal: u8,
        writer: &mut (impl Write + Seek),
    ) -> RefPackResult<()> {
        let length_adjusted = length - 3;
        let offset_adjusted = offset - 1;

        let first = ((offset_adjusted & 0b0000_0011_0000_0000) >> 3) as u8
            | ((length_adjusted & 0b0000_0111) << 2) as u8
            | literal & 0b0000_0011;
        let second = (offset_adjusted & 0b0000_0000_1111_1111) as u8;

        writer.write_u8(first)?;
        writer.write_u8(second)?;
        Ok(())
    }

    /// Reference write implementation of medium copy commands. See struct
    /// definition for specification
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
    #[inline]
    pub fn write_medium(
        offset: u32,
        length: u16,
        literal: u8,
        writer: &mut (impl Write + Seek),
    ) -> RefPackResult<()> {
        let length_adjusted = length - 4;
        let offset_adjusted = offset - 1;

        let first = (0b1000_0000 | length_adjusted & 0b0011_1111) as u8;
        let second = ((literal & 0b0000_0011) << 6) | (offset_adjusted >> 8) as u8;
        let third = (offset_adjusted & 0b0000_0000_1111_1111) as u8;

        writer.write_u8(first)?;
        writer.write_u8(second)?;
        writer.write_u8(third)?;

        Ok(())
    }

    /// Reference write implementation of long copy commands. See struct
    /// definition for specification
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
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

    /// Reference write implementation of literal commands. See struct
    /// definition for specification
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
    #[inline]
    pub fn write_literal(literal: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        let adjusted = (literal - 4) >> 2;
        let out = 0b1110_0000 | (adjusted & 0b0001_1111);
        writer.write_u8(out)?;
        Ok(())
    }

    /// Reference write implementation of stopcode. See struct definition for
    /// specification
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
    #[inline]
    pub fn write_stop(number: u8, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        let out = 0b1111_1100 | (number & 0b0000_0011);
        writer.write_u8(out)?;
        Ok(())
    }

    /// Encodes and writes a command to a `Write + Seek` writer
    ///
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to
    ///   write data
    pub fn write(self, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        match self.kind {
            CommandKind::Short => Self::write_short(self.offset, self.length, self.literal, writer),
            CommandKind::Medium => {
                Self::write_medium(self.offset, self.length, self.literal, writer)
            }
            CommandKind::Long => Self::write_long(self.offset, self.length, self.literal, writer),
            CommandKind::Literal => Self::write_literal(self.literal, writer),
            CommandKind::Stop => Self::write_stop(self.literal, writer),
        }
    }
}

#[cfg(test)]
prop_compose! {
    fn bytes_strategy(
        length: usize,
    )(
        vec in vec(any::<u8>(), size_range(length)),
    ) -> Vec<u8> {
        vec
    }
}

/// Full control block of command + literal bytes following it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The command code
    pub command: Command,
    /// the literal bytes to write to the stream
    pub bytes: Vec<u8>,
}

impl Control {
    /// Create a new Control given a command and bytes
    #[must_use]
    pub fn new(command: Command, bytes: Vec<u8>) -> Self {
        Self { command, bytes }
    }

    /// Create a new literal block given a slice of bytes.
    /// the `Command` is automatically generated from the length of the byte
    /// slice.
    #[must_use]
    pub fn new_literal_block(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_literal(bytes.len() as u8),
            bytes: bytes.to_vec(),
        }
    }

    /// Create a new stop control block given a slice of bytes
    /// the `Command` is automatically generated from the length of the byte
    /// slice.
    #[must_use]
    pub fn new_stop(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_stop(bytes.len()),
            bytes: bytes.to_vec(),
        }
    }

    /// Reads and decodes a control block from a `Read + Seek` reader
    /// # Errors
    /// - [RefPackError::Io]: Generic IO error occurred while attempting to read
    ///   data
    pub fn read(reader: &mut (impl Read + Seek)) -> Result<Self, RefPackError> {
        let command = Command::read(reader)?;
        let mut buf = vec![0u8; command.num_of_literal().unwrap_or(0)];
        reader.read_exact(&mut buf)?;
        Ok(Control {
            command,
            bytes: buf,
        })
    }

    /// Encodes and writes a control block to a `Write + Seek` writer
    /// # Errors
    /// - [RefPackError::Io]: Generic IO Error occurred while attempting to
    ///   write data
    pub fn write(&self, writer: &mut (impl Write + Seek)) -> Result<(), RefPackError> {
        self.command.write(writer)?;
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

use crate::data::control::{Command as OldCommand, Control as OldControl};

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{Cursor, SeekFrom};

    use test_strategy::proptest;

    use super::*;

    pub fn generate_random_valid_command() -> BoxedStrategy<Command> {
        let short_copy_strat = (
            SHORT_OFFSET_MIN..=SHORT_OFFSET_MAX,
            SHORT_LENGTH_MIN..=SHORT_LENGTH_MAX,
            COPY_LITERAL_MIN..=COPY_LITERAL_MAX,
        )
            .prop_map(|(offset, length, literal)| {
                Command {
                    offset,
                    length,
                    literal,
                    kind: CommandKind::Short,
                }
            });

        let medium_copy_strat = (
            MEDIUM_OFFSET_MIN..=MEDIUM_OFFSET_MAX,
            MEDIUM_LENGTH_MIN..=MEDIUM_LENGTH_MAX,
            COPY_LITERAL_MIN..=COPY_LITERAL_MAX,
        )
            .prop_map(|(offset, length, literal)| {
                Command {
                    offset,
                    length,
                    literal,
                    kind: CommandKind::Medium,
                }
            });

        let long_copy_strat = (
            LONG_OFFSET_MIN..=LONG_OFFSET_MAX,
            LONG_LENGTH_MIN..=LONG_LENGTH_MAX,
            COPY_LITERAL_MIN..=COPY_LITERAL_MAX,
        )
            .prop_map(|(offset, length, literal)| {
                Command {
                    offset,
                    length,
                    literal,
                    kind: CommandKind::Long,
                }
            });

        let literal_strat = LITERAL_EFFECTIVE_MIN..=LITERAL_EFFECTIVE_MAX;
        let literal = Strategy::prop_map(literal_strat, |literal| {
            Command::new_literal((literal * 4) + 4)
        });
        prop_oneof![
            short_copy_strat,
            medium_copy_strat,
            long_copy_strat,
            literal
        ]
        .boxed()
    }

    pub fn generate_stopcode() -> BoxedStrategy<Command> {
        (COPY_LITERAL_MIN..=COPY_LITERAL_MAX)
            .prop_map(Command::new_stop_unchecked)
            .boxed()
    }

    pub fn generate_control() -> BoxedStrategy<Control> {
        generate_random_valid_command()
            .prop_flat_map(|command| {
                (
                    Just(command),
                    vec(any::<u8>(), command.num_of_literal().unwrap_or(0)),
                )
            })
            .prop_map(|(command, bytes)| Control { command, bytes })
            .boxed()
    }

    pub fn generate_stop_control() -> BoxedStrategy<Control> {
        generate_stopcode()
            .prop_flat_map(|command| {
                (
                    Just(command),
                    vec(any::<u8>(), command.num_of_literal().unwrap_or(0)),
                )
            })
            .prop_map(|(command, bytes)| Control { command, bytes })
            .boxed()
    }

    pub fn generate_valid_control_sequence(max_length: usize) -> BoxedStrategy<Vec<Control>> {
        (
            vec(generate_control(), 0..(max_length - 1)),
            generate_stop_control(),
        )
            .prop_map(|(vec, stopcode)| {
                let mut vec = vec;
                vec.push(stopcode);
                vec
            })
            .boxed()
    }

    #[proptest]
    fn symmetrical_command_copy(
        #[strategy(1..=131_071_u32)] offset: u32,
        #[strategy(5..=1028_u16)] length: u16,
        #[strategy(0..=3_u8)] literal: u8,
    ) {
        let expected = Command::new(offset, length, literal);
        let mut buf = Cursor::new(vec![]);
        expected.write(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_literal(#[strategy(0..=27_u8)] literal: u8) {
        let real_length = (literal * 4) + 4;

        let expected = Command::new_literal(real_length);
        let mut buf = Cursor::new(vec![]);
        expected.write(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_stop(#[strategy(0..=3_usize)] input: usize) {
        let expected = Command::new_stop(input);
        let mut buf = Cursor::new(vec![]);
        expected.write(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_any_command(#[strategy(generate_random_valid_command())] input: Command) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected.write(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_stop_invalid() {
        let _invalid = Command::new_stop(8000);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_literal_invalid() {
        let _invalid = Command::new_literal(u8::MAX);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_offset() {
        let _invalid = Command::new(500_000, 0, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_length() {
        let _invalid = Command::new(0, u16::MAX, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_literal() {
        let _invalid = Command::new(0, 0, u8::MAX);
    }

    #[proptest]
    fn symmetrical_control(#[strategy(generate_control())] input: Control) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected.write(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Control = Control::read(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }
}
