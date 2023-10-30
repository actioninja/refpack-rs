////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::fmt::{Display, Formatter};

use crate::data::DecodeError;

/// Possible errors returned by compression and decompression functions
#[derive(Debug)]
pub enum Error {
    /// Error for when no input is provided to a compressor function
    EmptyInput,
    /// Error that occurs when a flag was set in the header flags that is not
    /// supported
    ///
    /// ### Fields
    /// - u8: What was read instead of the expected flags
    BadFlags(u8),
    /// Error indicating that the header failed to read the magic where it
    /// expected it. Location depends on the exact implementation.
    ///
    /// ### Fields
    /// - u8: What was read instead of the magic value
    BadMagic(u8),
    /// Indicates that an invalid operation occurred while attempting to decode
    /// a control. This normally indicates invalid or corrupted data.
    ///
    /// See [DecodeError] for further details on types of errors that can occur.
    ControlError { error: DecodeError, position: usize },
    /// Generic IO Error wrapper for when a generic IO error of some sort occurs
    /// in relation to the readers and writers.
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EmptyInput => {
                write!(f, "No input provided to compression")
            }
            Error::BadFlags(flags) => {
                write!(
                    f,
                    "Unknown flag was set in compression header `{flags:08b}`"
                )
            }
            Error::BadMagic(magic) => {
                write!(
                    f,
                    "Invalid magic number at compression header `{magic:#04X}`"
                )
            }
            Error::ControlError { position, error } => {
                write!(
                    f,
                    "Error occured while decoding control block at position `{position}`:\n{error}"
                )
            }
            Error::Io(err) => {
                write!(f, "IO Error: {err}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Wrapper for Result specified to [RefPackError]
pub type Result<T> = std::result::Result<T, Error>;
