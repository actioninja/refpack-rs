////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! control codes utilized by compression and decompression

#[cfg(test)]
mod iterator;
pub mod mode;

use std::io::{Read, Seek, Write};

#[cfg(test)]
use proptest::collection::{size_range, vec};
#[cfg(test)]
use proptest::prelude::*;

pub use crate::data::control::mode::Mode;
use crate::{RefPackError, RefPackResult};

/// The instruction part of a control block that dictates to the compression algorithm what
/// operations should be executed to decompress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Represents a two byte copy command
    Short {
        literal: u8,
        length: u8,
        offset: u16,
    },
    /// Represents a three byte copy command
    Medium {
        literal: u8,
        length: u8,
        offset: u16,
    },
    /// Represents a four byte copy command
    Long {
        literal: u8,
        length: u16,
        offset: u32,
    },
    /// Represents exclusively writing literal bytes from the stream
    ///
    /// u8: number of literal bytes following the command to write to the stream
    Literal(u8),
    /// Represents an end of stream, when this command is encountered during decompression it's
    /// evaluated and then decompression halts
    ///
    /// u8: Number of literal bytes to write to the stream before ending decompression
    Stop(u8),
}

impl Command {
    /// Create a new copy type `Command` struct.
    /// # Panics
    /// Panics if you attempt to create an invalid Command in some way
    #[must_use]
    pub fn new<M: Mode>(offset: usize, length: usize, literal: usize) -> Self {
        assert!(
            literal <= M::SIZES.copy_literal_max() as usize,
            "Literal length must be less than or equal to {} for commands ({})",
            M::SIZES.copy_literal_max(),
            literal
        );

        if offset > M::SIZES.long_offset_max() as usize
            || length > M::SIZES.long_length_max() as usize
        {
            panic!(
                "Invalid offset or length (Maximum offset {}, got {}) (Maximum length {}, got {})",
                M::SIZES.long_offset_max(),
                offset,
                M::SIZES.long_length_max(),
                length
            );
        } else if offset > M::SIZES.medium_offset_max() as usize
            || length > M::SIZES.medium_length_max() as usize
        {
            assert!(
                length >= M::SIZES.long_length_min() as usize,
                "Length must be greater than or equal to {} for long commands (Length: {}) (Offset: {})",
                M::SIZES.long_length_min(),
                length,
                offset
            );
            Self::Long {
                offset: offset as u32,
                length: length as u16,
                literal: literal as u8,
            }
        } else if offset > M::SIZES.short_offset_max() as usize
            || length > M::SIZES.short_length_max() as usize
        {
            assert!(
                length >= M::SIZES.medium_length_min() as usize,
                "Length must be greater than or equal to {} for medium commands (Length: {}) (Offset: {})",
                M::SIZES.medium_length_min(),
                length,
                offset
            );
            Self::Medium {
                offset: offset as u16,
                length: length as u8,
                literal: literal as u8,
            }
        } else {
            Self::Short {
                offset: offset as u16,
                length: length as u8,
                literal: literal as u8,
            }
        }
    }

    /// Creates a new literal command block
    /// # Panics
    /// Panics if you attempt to create too long of a literal command. This depends on control mode
    /// used.
    #[must_use]
    pub fn new_literal<M: Mode>(length: usize) -> Self {
        assert!(
            length <= M::SIZES.literal_max() as usize,
            "Literal received too long of a literal length (max {}, got {})",
            M::SIZES.literal_max(),
            length
        );
        Self::Literal(length as u8)
    }

    /// Creates a new stopcode command block
    /// # Panics
    /// Panics if you attempt to create too long of a stop code. This depends on control mode used.
    #[must_use]
    pub fn new_stop<M: Mode>(literal_length: usize) -> Self {
        assert!(
            literal_length <= 3,
            "Stopcode recieved too long of a literal length (max {}, got {})",
            M::SIZES.copy_literal_max(),
            literal_length
        );
        Self::Stop(literal_length as u8)
    }

    /// Get number of literal bytes on the command, if they have any
    /// Returns `None` if the length is 0
    #[must_use]
    pub fn num_of_literal(self) -> Option<usize> {
        let num = match self {
            Command::Short { literal, .. }
            | Command::Medium { literal, .. }
            | Command::Long { literal, .. }
            | Command::Literal(literal)
            | Command::Stop(literal) => literal,
        };
        if num == 0 {
            None
        } else {
            Some(num as usize)
        }
    }

    /// Get the offset and length of a copy command as a `(usize, usize)` tuple.
    ///
    /// Returns `None` if `self` is not a copy command.
    #[must_use]
    pub fn offset_copy(self) -> Option<(usize, usize)> {
        match self {
            Command::Short { offset, length, .. } | Command::Medium { offset, length, .. } => {
                Some((offset as usize, length as usize))
            }
            Command::Long { offset, length, .. } => Some((offset as usize, length as usize)),
            _ => None,
        }
    }

    /// Returns true if the command is a stopcode, false if it is not.
    #[must_use]
    pub fn is_stop(self) -> bool {
        matches!(self, Command::Stop(_))
    }

    /// Reads and decodes a command from a `Read + Seek` reader.
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to read data
    #[inline(always)]
    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> RefPackResult<Self> {
        M::read(reader)
    }

    /// Encodes and writes a command to a `Write + Seek` writer
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to write data
    pub fn write<M: Mode>(self, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        M::write(self, writer)?;
        Ok(())
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
    /// the `Command` is automatically generated from the length of the byte slice.
    #[must_use]
    pub fn new_literal_block<M: Mode>(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_literal::<M>(bytes.len()),
            bytes: bytes.to_vec(),
        }
    }

    /// Create a new stop control block given a slice of bytes
    /// the `Command` is automatically generated from the length of the byte slice.
    #[must_use]
    pub fn new_stop<M: Mode>(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_stop::<M>(bytes.len()),
            bytes: bytes.to_vec(),
        }
    }

    /// Reads and decodes a control block from a `Read + Seek` reader
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to read data
    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> Result<Self, RefPackError> {
        let command = Command::read::<M>(reader)?;
        let mut buf = vec![0u8; command.num_of_literal().unwrap_or(0)];
        reader.read_exact(&mut buf)?;
        Ok(Control {
            command,
            bytes: buf,
        })
    }

    /// Encodes and writes a control block to a `Write + Seek` writer
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to write data
    pub fn write<M: Mode>(&self, writer: &mut (impl Write + Seek)) -> Result<(), RefPackError> {
        self.command.write::<M>(writer)?;
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{Cursor, SeekFrom};

    use test_strategy::proptest;

    use super::*;
    use crate::data::control::mode::Reference;

    pub fn generate_random_valid_command<M: Mode>() -> BoxedStrategy<Command> {
        let sizes = M::SIZES;
        let short_copy_strat = (
            sizes.short_offset_min()..=sizes.short_offset_max(),
            sizes.short_length_min()..=sizes.short_length_max(),
            sizes.copy_literal_min()..=sizes.copy_literal_max(),
        )
            .prop_map(|(offset, length, literal)| Command::Short {
                offset,
                length,
                literal,
            });

        let medium_copy_strat = (
            sizes.medium_offset_min()..=sizes.medium_offset_max(),
            sizes.medium_length_min()..=sizes.medium_length_max(),
            sizes.copy_literal_min()..=sizes.copy_literal_max(),
        )
            .prop_map(|(offset, length, literal)| Command::Medium {
                offset,
                length,
                literal,
            });

        let long_copy_strat = (
            sizes.long_offset_min()..=sizes.long_offset_max(),
            sizes.long_length_min()..=sizes.long_length_max(),
            sizes.copy_literal_min()..=sizes.copy_literal_max(),
        )
            .prop_map(|(offset, length, literal)| Command::Long {
                offset,
                length,
                literal,
            });

        let literal_strat = sizes.literal_effective_min()..=sizes.literal_effective_max();
        let literal =
            Strategy::prop_map(literal_strat, |literal| Command::Literal((literal * 4) + 4));
        prop_oneof![
            short_copy_strat,
            medium_copy_strat,
            long_copy_strat,
            literal
        ]
        .boxed()
    }

    pub fn generate_stopcode<M: Mode>() -> BoxedStrategy<Command> {
        let sizes = M::SIZES;

        (sizes.copy_literal_min()..=sizes.copy_literal_max())
            .prop_map(Command::Stop)
            .boxed()
    }

    pub fn generate_control<M: Mode>() -> BoxedStrategy<Control> {
        generate_random_valid_command::<M>()
            .prop_flat_map(|command| {
                (
                    Just(command),
                    vec(any::<u8>(), command.num_of_literal().unwrap_or(0)),
                )
            })
            .prop_map(|(command, bytes)| Control { command, bytes })
            .boxed()
    }

    pub fn generate_stop_control<M: Mode>() -> BoxedStrategy<Control> {
        generate_stopcode::<M>()
            .prop_flat_map(|command| {
                (
                    Just(command),
                    vec(any::<u8>(), command.num_of_literal().unwrap_or(0)),
                )
            })
            .prop_map(|(command, bytes)| Control { command, bytes })
            .boxed()
    }

    pub fn generate_valid_control_sequence<M: Mode>(
        max_length: usize,
    ) -> BoxedStrategy<Vec<Control>> {
        (
            vec(generate_control::<M>(), 0..(max_length - 1)),
            generate_stop_control::<M>(),
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
        #[strategy(1..=131_071_usize)] offset: usize,
        #[strategy(5..=1028_usize)] length: usize,
        #[strategy(0..=3_usize)] literal: usize,
    ) {
        let expected = Command::new::<Reference>(offset, length, literal);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_literal(#[strategy(0..=27_usize)] literal: usize) {
        let real_length = (literal * 4) + 4;

        let expected = Command::new_literal::<Reference>(real_length);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_stop(#[strategy(0..=3_usize)] input: usize) {
        let expected = Command::new_stop::<Reference>(input);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_any_command(
        #[strategy(generate_random_valid_command::<Reference>())] input: Command,
    ) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_stop_invalid() {
        let _invalid = Command::new_stop::<Reference>(8000);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_literal_invalid() {
        let _invalid = Command::new_literal::<Reference>(8000);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_offset() {
        let _invalid = Command::new::<Reference>(500_000, 0, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_length() {
        let _invalid = Command::new::<Reference>(0, 500_000, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_literal() {
        let _invalid = Command::new::<Reference>(0, 0, 6000);
    }

    #[proptest]
    fn symmetrical_control(#[strategy(generate_control::<Reference>())] input: Control) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Control = Control::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }
}
