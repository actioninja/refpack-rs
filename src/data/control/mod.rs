////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

pub(crate) mod iterator;
pub mod mode;

use std::io::{Read, Seek, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};
#[cfg(test)]
use proptest::collection::{size_range, vec};
#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use test_strategy::Arbitrary;

pub use crate::data::control::mode::Mode;
use crate::RefPackError;

pub const MAX_COPY_SHORT_OFFSET: u16 = 1_023;
pub const MAX_COPY_SHORT_LEN: u8 = 10;
#[allow(dead_code)] //Clippy is shitting the bed here, this is actually used
pub const MIN_COPY_SHORT_LEN: u8 = 3;

pub const MAX_COPY_MEDIUM_OFFSET: u16 = 16_383;
pub const MAX_COPY_MEDIUM_LEN: u8 = 67;
pub const MIN_COPY_MEDIUM_LEN: u8 = 4;

pub const MAX_COPY_LONG_OFFSET: u32 = 131_072;
pub const MAX_COPY_LONG_LEN: u16 = 1_028;
pub const MIN_COPY_LONG_LEN: u16 = 5;

pub const MIN_COPY_OFFSET: u32 = 1;
pub const MAX_COPY_LIT_LEN: u8 = 3;

pub const MAX_OFFSET_DISTANCE: usize = MAX_COPY_LONG_OFFSET as usize;
pub const MAX_COPY_LEN: usize = MAX_COPY_LONG_LEN as usize;

pub const MAX_LITERAL_LEN: usize = 112;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum Command {
    Short {
        #[cfg_attr(test, strategy((MIN_COPY_OFFSET as u16)..=MAX_COPY_SHORT_OFFSET))]
        offset: u16,
        #[cfg_attr(test, strategy(MIN_COPY_SHORT_LEN..=MAX_COPY_SHORT_LEN))]
        length: u8,
        #[cfg_attr(test, strategy(0..=MAX_COPY_LIT_LEN))]
        literal: u8,
    },
    Medium {
        #[cfg_attr(test, strategy((MIN_COPY_OFFSET as u16)..=MAX_COPY_MEDIUM_OFFSET))]
        offset: u16,
        #[cfg_attr(test, strategy(MIN_COPY_MEDIUM_LEN..=MAX_COPY_MEDIUM_LEN))]
        length: u8,
        #[cfg_attr(test, strategy(0..=MAX_COPY_LIT_LEN))]
        literal: u8,
    },
    Long {
        #[cfg_attr(test, strategy(MIN_COPY_OFFSET..=MAX_COPY_LONG_OFFSET))]
        offset: u32,
        #[cfg_attr(test, strategy(MIN_COPY_LONG_LEN..=MAX_COPY_LONG_LEN))]
        length: u16,
        #[cfg_attr(test, strategy(0..=MAX_COPY_LIT_LEN))]
        literal: u8,
    },
    Literal(#[cfg_attr(test, strategy((0..=27_u8).prop_map(|x| (x * 4) + 4)))] u8),
    Stop(#[cfg_attr(test, strategy(0..=MAX_COPY_LIT_LEN))] u8),
}

impl Command {
    pub fn new(offset: usize, length: usize, literal: usize) -> Self {
        assert!(
            literal <= MAX_COPY_LIT_LEN as usize,
            "Literal length must be less than or equal to 3 for commands (got {literal})"
        );

        if offset > MAX_OFFSET_DISTANCE || length > MAX_COPY_LEN {
            panic!("Invalid offset or length (Maximum offset 131072, got {offset}) (Maximum length 1028, got {length})");
        } else if offset > MAX_COPY_MEDIUM_OFFSET as usize || length > MAX_COPY_MEDIUM_LEN as usize
        {
            assert!(
                length >= MIN_COPY_LONG_LEN as usize,
                "Length must be greater than or equal to 5 for long commands (Length: {length}) (Offset: {offset})"
            );
            Self::Long {
                offset: offset as u32,
                length: length as u16,
                literal: literal as u8,
            }
        } else if offset > MAX_COPY_SHORT_OFFSET as usize || length > MAX_COPY_SHORT_LEN as usize {
            assert!(
                length >= MIN_COPY_MEDIUM_LEN as usize,
                "Length must be greater than or equal to 4 for medium commands (Length: {length}) (Offset: {offset})"
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

    pub fn new_literal(length: usize) -> Self {
        assert!(
            length <= 112,
            "Literal received too long of a literal length (max 112, got {length})"
        );
        Self::Literal(length as u8)
    }

    pub fn new_stop(literal_length: usize) -> Self {
        assert!(
            literal_length <= 3,
            "Stopcode recieved too long of a literal length (max 3, got {literal_length})"
        );
        Self::Stop(literal_length as u8)
    }

    pub fn num_of_literal(self) -> Option<usize> {
        match self {
            Command::Short { literal, .. }
            | Command::Medium { literal, .. }
            | Command::Long { literal, .. } => {
                if literal == 0 {
                    None
                } else {
                    Some(literal as usize)
                }
            }
            Command::Literal(number) => Some(number as usize),
            Command::Stop(number) => {
                if number == 0 {
                    None
                } else {
                    Some(number as usize)
                }
            }
        }
    }

    pub fn offset_copy(self) -> Option<(usize, usize)> {
        match self {
            Command::Short { offset, length, .. } | Command::Medium { offset, length, .. } => {
                Some((offset as usize, length as usize))
            }
            Command::Long { offset, length, .. } => Some((offset as usize, length as usize)),
            _ => None,
        }
    }

    pub fn is_stop(self) -> bool {
        matches!(self, Command::Stop(_))
    }

    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> Result<Self, RefPackError> {
        M::read(reader)
    }

    pub fn write<M: Mode>(self, writer: &mut (impl Write + Seek)) -> Result<(), RefPackError> {
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

/// Full control block of command + literal bytes
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Control {
    pub command: Command,
    #[cfg_attr(test, strategy(bytes_strategy(#command.num_of_literal().unwrap_or(0))))]
    pub bytes: Vec<u8>,
}

impl Control {
    pub fn new(command: Command, bytes: Vec<u8>) -> Self {
        Self { command, bytes }
    }

    pub fn new_literal_block(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_literal(bytes.len()),
            bytes: bytes.to_vec(),
        }
    }

    pub fn new_stop(bytes: &[u8]) -> Self {
        Self {
            command: Command::new_stop(bytes.len()),
            bytes: bytes.to_vec(),
        }
    }

    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> Result<Self, RefPackError> {
        let command = Command::read::<M>(reader)?;
        let mut buf = vec![0u8; command.num_of_literal().unwrap_or(0)];
        reader.read_exact(&mut buf)?;
        Ok(Control {
            command,
            bytes: buf,
        })
    }

    pub fn write<M: Mode>(&self, writer: &mut (impl Write + Seek)) -> Result<(), RefPackError> {
        self.command.write::<M>(writer)?;
        writer.write_all(&self.bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, SeekFrom};

    use proptest::prop_assert_eq;
    use test_strategy::proptest;

    use super::*;
    use crate::data::control::iterator::Iter;
    use crate::data::control::mode::Reference;

    #[proptest]
    fn symmetrical_command_copy(
        #[strategy(1..=131_071_usize)] offset: usize,
        #[strategy(5..=1028_usize)] length: usize,
        #[strategy(0..=3_usize)] literal: usize,
    ) {
        let expected = Command::new(offset, length, literal);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_literal(#[strategy(0..=27_usize)] literal: usize) {
        let real_length = (literal * 4) + 4;

        let expected = Command::new_literal(real_length);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_stop(#[strategy(0..=3_usize)] input: usize) {
        let expected = Command::new_stop(input);
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_any_command(input: Command) {
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
        let _invalid = Command::new_stop(8000);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_literal_invalid() {
        let _invalid = Command::new_literal(8000);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_offset() {
        let _invalid = Command::new(500_000, 0, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_length() {
        let _invalid = Command::new(0, 500_000, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_literal() {
        let _invalid = Command::new(0, 0, 6000);
    }

    #[proptest]
    fn symmetrical_control(input: Control) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected.write::<Reference>(&mut buf).unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Control = Control::read::<Reference>(&mut buf).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn test_control_iterator(input: Vec<Control>) {
        //todo: make this not a stupid hack
        let mut input: Vec<Control> = input
            .iter()
            .filter(|c| !c.command.is_stop())
            .cloned()
            .collect();
        input.push(Control {
            command: Command::new_stop(0),
            bytes: vec![],
        });
        let expected = input.clone();
        let buf = input
            .iter()
            .map(|control: &Control| -> Vec<u8> {
                let mut buf = Cursor::new(vec![]);
                control.write::<Reference>(&mut buf).unwrap();
                buf.into_inner()
            })
            .fold(vec![], |mut acc, mut buf| {
                acc.append(&mut buf);
                acc
            });

        let mut cursor = Cursor::new(buf);
        let out: Vec<Control> = Iter::<_, Reference>::new(&mut cursor).collect();

        prop_assert_eq!(out, expected);
    }
}
