////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use binrw::{binrw, BinRead, BinResult, BinWrite, ReadOptions, WriteOptions};
use bitvec::prelude::*;
use byteorder::ReadBytesExt;
#[cfg(test)]
use proptest::collection::{size_range, vec};
#[cfg(test)]
use proptest::prelude::*;
use std::io::{Read, Seek, Write};
#[cfg(test)]
use test_strategy::Arbitrary;

pub const MAX_COPY_LEN: usize = 1028;
pub const MAX_OFFSET_DISTANCE: usize = 131_071;
pub const MAX_LITERAL_LEN: usize = 112;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum Command {
    /// - Length: 2
    /// - Literal Range: 0-3
    /// - Literal Magic: 0
    /// - Length Range: 3-10
    /// - Length Magic: +3
    /// - Position Range: 1-1023
    /// - Position Magic: +1
    /// - Layout: 0PPL-LLBB|PPPP-PPPP
    Short {
        #[cfg_attr(test, strategy(1..=1023_u16))]
        offset: u16,
        #[cfg_attr(test, strategy(3..=10_u8))]
        length: u8,
        #[cfg_attr(test, strategy(0..=3_u8))]
        literal: u8,
    },
    /// - Length: 3
    /// - Literal Range: 0-3
    /// - Literal Magic: 0
    /// - Length Range: 4-67
    /// - Length Magic: +4
    /// - Position Range: 1-16383
    /// - Position Magic: +1
    /// - Layout: 10LL-LLLL|BBPP-PPPP|PPPP-PPPP
    Medium {
        #[cfg_attr(test, strategy(1..=16383_u16))]
        offset: u16,
        #[cfg_attr(test, strategy(4..=67_u8))]
        length: u8,
        #[cfg_attr(test, strategy(0..=3_u8))]
        literal: u8,
    },
    /// - Length: 4
    /// - Literal Range: 0-3
    /// - Literal Magic: 0
    /// - Length Range: 5-1028
    /// - Length Magic: +5
    /// - Position Range: 1-131072
    /// - Position Magic: +1
    /// - Layout: 110P-LLBB|PPPP-PPPP|PPPP-PPPP|LLLL-LLLL
    Long {
        #[cfg_attr(test, strategy(1..=131_072_u32))]
        offset: u32,
        #[cfg_attr(test, strategy(5..=1028_u16))]
        length: u16,
        #[cfg_attr(test, strategy(0..=3_u8))]
        literal: u8,
    },
    /// - Length: 1
    /// - Literal Range: 4-112; limited precision
    /// - Literal Magic: +4
    /// - Length Range: 0
    /// - Length Magic: 0
    /// - Position Range: 0
    /// - Position Magic: 0
    /// - Layout: 111B-BBBB
    /// - Notes: Magic bit shift happens here for unclear reasons, effectively multiplying
    ///        stored number by 4.
    /// - Weird detail of how it's read; range is in fact capped at 112 even though it seems like
    ///        it could be higher. The original program read by range of control as an absolute
    ///        number, meaning that if the number was higher than 27, it would instead be read as a
    ///        stopcode. Don't ask me.
    Literal(#[cfg_attr(test, strategy((0..=27_u8).prop_map(|x| (x * 4) + 4)))] u8),
    /// - Length: 1
    /// - Literal Range: 0-3
    /// - Literal Magic: 0
    /// - Length Range: 0
    /// - Length Magic: 0
    /// - Position Range: 0
    /// - Position Magic: 0
    /// - Layout: 1111-11PP
    Stop(#[cfg_attr(test, strategy(0..=3_u8))] u8),
}

impl Command {
    pub fn new(offset: usize, length: usize, literal: usize) -> Self {
        if literal > 3 {
            panic!("Literal length must be less than 3 (got {})", literal);
        }

        if offset > 131_072 || length > 1028 {
            panic!("Invalid offset or length (Maximum offset 131072, got {}) (Maximum length 1028, got {})", offset, length);
        } else if offset > 16383 || length > 67 {
            Self::Long {
                offset: offset as u32,
                length: length as u16,
                literal: literal as u8,
            }
        } else if offset > 1023 || length > 10 {
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
        if length > 112 {
            panic!(
                "Literal received too long of a literal length (max 112, got {})",
                length
            );
        } else {
            Self::Literal(length as u8)
        }
    }

    pub fn new_stop(literal_length: usize) -> Self {
        if literal_length > 3 {
            panic!(
                "Stopcode received too long of a literal length (max 3, got {})",
                literal_length
            )
        } else {
            Self::Stop(literal_length as u8)
        }
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
        match self {
            Command::Stop(_) => true,
            _ => false,
        }
    }
}

impl BinRead for Command {
    type Args = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _: &ReadOptions,
        _: Self::Args,
    ) -> BinResult<Self> {
        let first = reader.read_u8()?;

        match first {
            0x00..=0x7F => {
                let byte1 = first as usize;
                let byte2: usize = reader.read_u8()?.into();

                let offset = ((((byte1 & 0b0110_0000) << 3) | byte2) + 1) as u16;
                let length = (((byte1 & 0b0001_1100) >> 2) + 3) as u8;
                let literal = (byte1 & 0b0000_0011) as u8;

                Ok(Self::Short {
                    offset,
                    length,
                    literal,
                })
            }
            0x80..=0xBF => {
                let byte1: usize = first as usize;
                let byte2: usize = reader.read_u8()?.into();
                let byte3: usize = reader.read_u8()?.into();

                let offset = ((((byte2 & 0b0011_1111) << 8) | byte3) + 1) as u16;
                let length = ((byte1 & 0b0011_1111) + 4) as u8;
                let literal = ((byte2 & 0b1100_0000) >> 6) as u8;

                Ok(Self::Medium {
                    offset,
                    length,
                    literal,
                })
            }
            0xC0..=0xDF => {
                let byte1: usize = first as usize;
                let byte2: usize = reader.read_u8()?.into();
                let byte3: usize = reader.read_u8()?.into();
                let byte4: usize = reader.read_u8()?.into();

                let offset = ((((byte1 & 0b0001_0000) << 12) | (byte2 << 8) | byte3) + 1) as u32;
                let length = ((((byte1 & 0b0000_1100) << 6) | byte4) + 5) as u16;

                let literal = (byte1 & 0b0000_0011) as u8;

                Ok(Self::Long {
                    offset,
                    length,
                    literal,
                })
            }
            0xE0..=0xFB => Ok(Self::Literal(((first & 0b0001_1111) << 2) + 4)),
            0xFC..=0xFF => Ok(Self::Stop(first & 0b0000_0011)),
        }
    }
}

impl BinWrite for Command {
    type Args = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        options: &WriteOptions,
        _: Self::Args,
    ) -> BinResult<()> {
        match self {
            Command::Short {
                offset,
                length,
                literal,
            } => {
                let mut out = bitvec![Msb0, u8; 0; 16];

                let length_adjusted = *length - 3;
                let offset_adjusted = *offset - 1;

                let offset_bitview = offset_adjusted.view_bits::<Msb0>();
                let length_bitview = length_adjusted.view_bits::<Msb0>();
                let literal_bitview = literal.view_bits::<Msb0>();

                out[1..=2].clone_from_bitslice(&offset_bitview[6..=7]);
                out[3..=5].copy_from_bitslice(&length_bitview[5..=7]);
                out[6..=7].copy_from_bitslice(&literal_bitview[6..=7]);
                out[8..=15].clone_from_bitslice(&offset_bitview[8..=15]);

                writer.write_all(&out.into_vec())?;
                Ok(())
            }
            Command::Medium {
                offset,
                length,
                literal,
            } => {
                let mut out = bitvec![Msb0, u8; 0; 24];

                let length_adjusted = *length - 4;
                let offset_adjusted = *offset - 1;

                let offset_bitview = offset_adjusted.view_bits::<Msb0>();
                let length_bitview = length_adjusted.view_bits::<Msb0>();
                let literal_bitview = literal.view_bits::<Msb0>();

                out[0..=1].copy_from_bitslice(&bitvec![Msb0, u8; 1, 0][..]);
                out[2..=7].copy_from_bitslice(&length_bitview[2..=7]);
                out[8..=9].copy_from_bitslice(&literal_bitview[6..=7]);
                out[10..=23].clone_from_bitslice(&offset_bitview[2..=15]);

                writer.write_all(&out.into_vec())?;
                Ok(())
            }
            Command::Long {
                offset,
                length,
                literal,
            } => {
                let mut out = bitvec![Msb0, u8; 0; 32];

                let length_adjusted = *length - 5;
                let offset_adjusted = *offset - 1;

                let offset_bitview = offset_adjusted.view_bits::<Msb0>();
                let length_bitview = length_adjusted.view_bits::<Msb0>();
                let literal_bitview = literal.view_bits::<Msb0>();

                out[0..=2].copy_from_bitslice(&bitvec![Msb0, u8; 1, 1, 0][..]);
                out[3..=3].clone_from_bitslice(&offset_bitview[15..=15]);
                out[4..=5].clone_from_bitslice(&length_bitview[6..=7]);
                out[6..=7].copy_from_bitslice(&literal_bitview[6..=7]);
                out[8..=23].clone_from_bitslice(&offset_bitview[16..=31]);
                out[24..=31].clone_from_bitslice(&length_bitview[8..=15]);

                writer.write_all(&out.into_vec())?;
                Ok(())
            }
            Command::Literal(number) => {
                let adjusted = (*number - 4) >> 2;
                let out = 0b1110_0000 | (adjusted & 0b0001_1111);
                u8::write_options(&out, writer, options, ())?;
                Ok(())
            }
            Command::Stop(number) => {
                let out = 0b1111_1100 | (*number & 0b0000_0011);
                u8::write_options(&out, writer, options, ())?;
                Ok(())
            }
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

/// Full control block of command + literal bytes
#[binrw]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Control {
    pub command: Command,
    #[br(count(command.num_of_literal().unwrap_or(0)))]
    #[cfg_attr(test, strategy(bytes_strategy(#command.num_of_literal().unwrap_or(0))))]
    pub bytes: Vec<u8>,
}

impl Control {
    pub fn new(command: Command, bytes: Vec<u8>) -> Self {
        Self { command, bytes }
    }

    pub fn new_literal_block(bytes: &[u8]) -> Self {
        Self {
            command: Command::Literal(bytes.len() as u8),
            bytes: bytes.to_vec(),
        }
    }

    pub fn new_stop(bytes: &[u8]) -> Self {
        Self {
            command: Command::Stop(bytes.len() as u8),
            bytes: bytes.to_vec(),
        }
    }
}

/// Iterator to to read a byte reader into a sequence of controls that can be iterated through
pub struct Iter<'a, R: Read + Seek> {
    reader: &'a mut R,
    reached_stop: bool,
}

impl<'a, R: Read + Seek> Iter<'a, R> {
    pub fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            reached_stop: false,
        }
    }
}

impl<'a, R: Read + Seek> Iterator for Iter<'a, R> {
    type Item = Control;

    fn next(&mut self) -> Option<Self::Item> {
        if self.reached_stop {
            None
        } else {
            Control::read_options(self.reader, &ReadOptions::default(), ())
                .ok()
                .map(|control| {
                    if control.command.is_stop() {
                        self.reached_stop = true;
                    }
                    control
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prop_assert_eq;
    use std::io::Cursor;
    use std::io::SeekFrom;
    use test_strategy::proptest;

    #[proptest]
    fn symmetrical_command_copy(
        #[strategy(1..=131_071_usize)] offset: usize,
        #[strategy(5..=1028_usize)] length: usize,
        #[strategy(0..=3_usize)] literal: usize,
    ) {
        let expected = Command::new(offset, length, literal);
        let mut buf = Cursor::new(vec![]);
        expected
            .write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_literal(#[strategy(0..=27_usize)] literal: usize) {
        let real_length = (literal * 4) + 4;

        let expected = Command::new_literal(real_length);
        let mut buf = Cursor::new(vec![]);
        expected
            .write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_command_stop(#[strategy(0..=3_usize)] input: usize) {
        let expected = Command::new_stop(input);
        let mut buf = Cursor::new(vec![]);
        expected
            .write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, expected);
    }

    #[proptest]
    fn symmetrical_any_command(input: Command) {
        let expected = input;
        let mut buf = Cursor::new(vec![]);
        expected
            .write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

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
        let _invalid = Command::new(500000, 0, 0);
    }

    #[test]
    #[should_panic]
    fn command_reject_new_invalid_high_length() {
        let _invalid = Command::new(0, 500000, 0);
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
        expected
            .write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Control = Control::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

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
                control.write_options(&mut buf, &WriteOptions::default(), ());
                buf.into_inner()
            })
            .fold(vec![], |mut acc, mut buf| {
                acc.append(&mut buf);
                acc
            });

        let mut cursor = Cursor::new(buf);
        let out: Vec<Control> = Iter::new(&mut cursor).collect();

        prop_assert_eq!(out, expected);
    }
}
