////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

mod reference;
mod simcity_4;

use std::io::{Read, Seek, Write};

pub use reference::Reference;
pub use simcity_4::Simcity4;

use crate::data::control::Command;
use crate::RefPackResult;

pub struct Sizes {
    position: (usize, usize),
}

/// Represents an encoding/decoding format for compression commands. TODO: Explanation of how this is a marker trait for static dispatch functions
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
/// To implement your own commands, do something TODO
pub trait Mode {
    /// Reads from a `Read + Seek` reader and attempts to parse a command at the current position.
    /// # Errors
    /// Returns [RefPackError::Io](crate::RefPackError::Io) if a generic IO Error occurs while
    /// attempting to read data
    fn read<R: Read + Seek>(reader: &mut R) -> RefPackResult<Command>;
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
        ($in_ty: path, $in_ident: ident, $error_msg: expr) => {
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
