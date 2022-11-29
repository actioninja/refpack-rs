////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use thiserror::Error;

/// Possible errors returned by compression and decompression functions
#[derive(Error, Debug)]
pub enum Error {
    /// Error for when no input is provided to a compressor function
    #[error("No input provided to compression")]
    EmptyInput,
    /// Error indicating that the header failed to read the magic where it expected it. Location
    /// depends on the exact implementation.
    /// u16: What was read instead of the magic value
    #[error("Invalid magic number at compression header `{0:#04X}`")]
    BadMagic(u8),
    /// Generic IO Error wrapper for when a generic IO error of some sort occurs in relation to
    /// the readers and writers.
    #[error("IO Error")]
    Io(#[from] std::io::Error),
}

/// Wrapper for Result specified to [RefPackError](crate::RefPackError)
pub type Result<T> = std::result::Result<T, Error>;
