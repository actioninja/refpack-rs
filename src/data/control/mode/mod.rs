////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! possible modes to use for encoding and decoding control blocks

mod reference;

use std::io::{Read, Seek, Write};

pub use reference::Reference;

use crate::data::control::Command;
use crate::RefPackResult;

/// Represents limits of values
///
/// All values are gotten through accessors to improve readability at usage
/// site, while making the definition site clearer by allowing directly defining
/// the struct
///
/// All accessors are `const`
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Sizes {
    literal: (u8, u8),
    copy_literal: (u8, u8),
    short_offset: (u16, u16),
    short_length: (u8, u8),
    medium_offset: (u16, u16),
    medium_length: (u8, u8),
    long_offset: (u32, u32),
    long_length: (u16, u16),
}

impl Sizes {
    /// minimum value of the literal length in a literal command
    #[must_use]
    pub const fn literal_min(self) -> u8 {
        self.literal.0
    }

    /// maximum value of the literal length in a literal command
    #[must_use]
    pub const fn literal_max(self) -> u8 {
        self.literal.1
    }

    /// minimum value of the literal length in a non-literal command
    #[must_use]
    pub const fn copy_literal_min(self) -> u8 {
        self.copy_literal.0
    }

    /// maximum value of the literal length in a non-literal command
    #[must_use]
    pub const fn copy_literal_max(self) -> u8 {
        self.copy_literal.1
    }

    /// minimum offset distance for a short command
    #[must_use]
    pub const fn short_offset_min(self) -> u16 {
        self.short_offset.0
    }

    /// maximum offset distance for a short command
    #[must_use]
    pub const fn short_offset_max(self) -> u16 {
        self.short_offset.1
    }

    /// minimum length for a short command
    #[must_use]
    pub const fn short_length_min(self) -> u8 {
        self.short_length.0
    }

    /// maximum length for a short command
    #[must_use]
    pub const fn short_length_max(self) -> u8 {
        self.short_length.1
    }

    /// minimum offset distance for a medium command
    #[must_use]
    pub const fn medium_offset_min(self) -> u16 {
        self.medium_offset.0
    }

    /// maximum offset distance for a medium command
    #[must_use]
    pub const fn medium_offset_max(self) -> u16 {
        self.medium_offset.1
    }

    /// minimum length for a medium command
    #[must_use]
    pub const fn medium_length_min(self) -> u8 {
        self.medium_length.0
    }

    /// maximum length for a medium command
    #[must_use]
    pub const fn medium_length_max(self) -> u8 {
        self.medium_length.1
    }

    /// minimum offset distance for a long command
    #[must_use]
    pub const fn long_offset_min(self) -> u32 {
        self.long_offset.0
    }

    /// maximum offset distance for a long command
    #[must_use]
    pub const fn long_offset_max(self) -> u32 {
        self.long_offset.1
    }

    /// minimum length for a long command
    #[must_use]
    pub const fn long_length_min(self) -> u16 {
        self.long_length.0
    }

    /// maximum length for a long command
    #[must_use]
    pub const fn long_length_max(self) -> u16 {
        self.long_length.1
    }

    /// "Real" minimum of literal value in a literal command once encoded
    ///
    /// Literal commands encode their value in a a special limit precision
    /// format
    ///
    /// See [Reference](crate::data::control::mode::Reference) for a more
    /// detailed writeup on this
    #[must_use]
    pub const fn literal_effective_min(self) -> u8 {
        (self.literal.0 - 4) / 4
    }

    /// "Real" maximum of literal value in a literal command once encoded
    ///
    /// Literal commands encode their value in a a special limit precision
    /// format
    ///
    /// See [Reference](crate::data::control::mode::Reference) for a more
    /// detailed writeup on this
    #[must_use]
    pub const fn literal_effective_max(self) -> u8 {
        (self.literal.1 - 4) / 4
    }
}

/// Represents an encoding/decoding format for compression commands.
///
/// This trait is entirely statically resolved and should only ever be
/// implemented on structs which cannot be constructed. It has only associated
/// functions, no methods, and only ever is referenced via generic arguments.
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
/// To implement your own commands, implement `Mode` on to a unit struct or
/// unconstructable struct with one private member and no new method.
/// #[Reference](crate::data::control::mode::Reference) has various associated
/// methods for common standard implementations that can be composed in.
/// `read` and `write` should be symmetrical, and a value fed in to read and
/// then back out of write should yield the same result.
pub trait Mode {
    const SIZES: Sizes;

    /// Reads from a `Read + Seek` reader and attempts to parse a command at the
    /// current position. # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO
    /// Error occurs while attempting to read data
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Command>;
    /// Writes to a `Write + Seek` writer and attempts to encode a command at
    /// the current position. # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO
    /// Error occurs while attempting to write data
    fn write<W: Write + Seek>(command: Command, writer: &mut W) -> RefPackResult<()>;
}

#[cfg(test)]
pub(crate) mod test {
    use std::io::Cursor;

    use proptest::prelude::*;

    use super::*;

    prop_compose! {
        pub fn generate_decoder_input
        (header: u8, header_mask: u8, length: usize)
        (vec in prop::collection::vec(any::<u8>(), length))
        -> Vec<u8> {
            let mut vec_mut = vec;
            vec_mut[0] = header | vec_mut[0] & !header_mask;
            vec_mut
        }
    }

    prop_compose! {
        pub fn generate_decoder_input_with_ceiling
            (header: u8, header_mask: u8, length: usize, limit: u8)
            (vec in prop::collection::vec(0..=limit, length))
            -> Vec<u8> {
            let mut vec_mut = vec;
            vec_mut[0] = header | vec_mut[0] & !header_mask;
            vec_mut
        }
    }

    macro_rules! symmetrical_rw {
        ($in_ty:path, $in_ident:ident, $error_msg:expr) => {
            let mut cursor = Cursor::new($in_ident.clone());
            let command_read = M::read(&mut cursor).unwrap();
            let does_match = matches!(command_read, $in_ty { .. });
            prop_assert!(does_match, $error_msg);
            let mut out_buf = Cursor::new(vec![]);
            M::write(command_read, &mut out_buf).unwrap();
            let result = out_buf.into_inner();
            prop_assert_eq!($in_ident, result);
        };
    }

    pub fn read_write_mode<M: Mode>(
        short: Vec<u8>,
        medium: Vec<u8>,
        long: Vec<u8>,
        literal: Vec<u8>,
        stop: Vec<u8>,
    ) -> Result<(), TestCaseError> {
        symmetrical_rw!(
            Command::Short,
            short,
            "Failed to parse short from short input"
        );

        symmetrical_rw!(
            Command::Medium,
            medium,
            "Failed to parse medium from medium input"
        );

        symmetrical_rw!(Command::Long, long, "Failed to parse long from long input");

        symmetrical_rw!(
            Command::Literal,
            literal,
            "Failed to parse literal from literal input"
        );

        symmetrical_rw!(Command::Stop, stop, "Failed to parse stop from stop input");

        Ok(())
    }
}
