////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use binrw::{binrw, BinRead, BinResult, BinWrite, ReadOptions, WriteOptions};
use bitvec::prelude::*;
use byteorder::ReadBytesExt;
use std::io::{Read, Seek, Write};

/// ## Key for description:
/// - Length: Length of the control in bytes
/// - Plain Text Range: Possible range of values of plain text
/// - Plain Text Magic: Magic number added to the number of plain text to copy
/// - Copy Range: Possible range of values of number to copy
/// - Copy Magic: Magic number added to the number to copy
/// - Offset Range: Possible range of offsets
/// - Offset Magic: Magic number added to the offset
/// - Layout: Bit layout of the control bytes

/// ## Key for layout
/// - 0 or 1: header
/// - F: oFfset (F to not be confused with 0)
/// - N: Number to Copy
/// - P: Plaintext
/// - -: Nibble Separator
/// - --: Byte Separator

/// Numbers are always "smashed" together into as small of a space as possible
/// EX: Getting the offset from "0FFN-NNPP--FFFF-FFFF"
/// 1. mask first byte: `(byte0 & 0b0110_0000)` = 0FF0-0000
/// 2. shift left by 3: `(0FF0-0000 << 3)` = 0000-00FF--0000-0000
/// 3. OR with second:  `(0000-00FF--0000-0000 | 0000-0000--FFFF-FFFF)` = 0000-00FF--FFFF-FFFF
/// Another way to do this would be to first shift right by 5 and so on
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// - Length: 2
    /// - Plain Text Range: 0-3
    /// - Plain Text Magic: 0
    /// - Copy Range: 3-10
    /// - Copy Magic: +3
    /// - Offset Range: 1-1023
    /// - Offset Magic: +1
    /// - Layout: 0FFN-NNPP|FFFF-FFFF
    Short {
        offset: u16,
        length: u8,
        literal: u8,
    },
    /// - Length: 3
    /// - Plain Text Range: 0-3
    /// - Plain Text Magic: 0
    /// - Copy Range: 4-67
    /// - Copy Magic: +4
    /// - Offset Range: 1-16383
    /// - Offset Magic: +1
    /// - Layout: 10NN-NNNN|PPFF-FFFF|FFFF-FFFF
    Medium {
        offset: u16,
        length: u8,
        literal: u8,
    },
    /// - Length: 4
    /// - Plain Text Range: 0-3
    /// - Plain Text Magic: 0
    /// - Copy Range: 5-1028
    /// - Copy Magic: +5
    /// - Offset Range: 1-131072
    /// - Offset Magic: +1
    /// - Layout: 110F-NNPP|FFFF-FFFF|FFFF-FFFF|NNNN-NNNN
    Long {
        offset: u32,
        length: u16,
        literal: u8,
    },
    /// - Length: 1
    /// - Plain Text Range: 4-112; limited precision
    /// - Plain Text Magic: +4
    /// - Copy Range: 0
    /// - Copy Magic: 0
    /// - Offset Range: 0
    /// - Offset Magic: 0
    /// - Layout: 111P-PPPP
    /// - Notes: Magic bit shift happens here for unclear reasons, effectively multiplying
    ///        stored number by 4.
    Literal(u8),
    /// - Length: 1
    /// - Plain Text Range: 0-3
    /// - Plain Text Magic: 0
    /// - Copy Range: 0
    /// - Copy Magic: 0
    /// - Offset Range: 0
    /// - Offset Magic: 0
    /// - Layout: 1111-11PP
    Stop(u8),
}

impl Command {
    pub fn new(offset: usize, length: usize, literal: usize) -> Self {
        if literal > 3 {
            panic!("Literal length must be less than 3 (got {})", literal);
        }

        if offset > 13_1072 || length > 1028 {
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

#[binrw]
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub command: Command,
    #[br(if(command.num_of_literal().is_some()))]
    #[br(args { count: command.num_of_literal().unwrap() })]
    pub bytes: Option<Vec<u8>>,
}

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
                    if let Command::Stop(_) = control.command {
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
        let stop = Command::new(offset, length, literal);
        let mut buf = Cursor::new(vec![]);
        stop.write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, stop);
    }

    #[proptest]
    fn symmetrical_command_literal(#[strategy(0..=27_usize)] literal: usize) {
        let real_length = (literal * 4) + 4;

        let stop = Command::new_literal(real_length);
        let mut buf = Cursor::new(vec![]);
        stop.write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, stop);
    }

    #[proptest]
    fn symmetrical_command_stop(#[strategy(0..=3_usize)] input: usize) {
        let stop = Command::new_stop(input);
        let mut buf = Cursor::new(vec![]);
        stop.write_options(&mut buf, &WriteOptions::default(), ())
            .unwrap();
        buf.seek(SeekFrom::Start(0)).unwrap();
        let out: Command = Command::read_options(&mut buf, &ReadOptions::default(), ()).unwrap();

        prop_assert_eq!(out, stop);
    }
}
