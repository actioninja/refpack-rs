////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use onlyerror::Error;

/// Possible errors returned by compression and decompression functions
#[derive(Error, Debug)]
pub enum Error {
    /// Error for when no input is provided to a compressor function
    #[error("No input provided to compression")]
    EmptyInput,
    /// Error that occurs when a flag was set in the header flags that is not supported
    #[error("Unknown flag was set in compression header `{0:08b}`")]
    BadFlags(u8),
    /// Error indicating that the header failed to read the magic where it expected it. Location
    /// depends on the exact implementation.
    /// u16: What was read instead of the magic value
    #[error("Invalid magic number at compression header `{0:#04X}`")]
    BadMagic(u8),
    /// Error indicating that offset was 0 in refpack control byte
    /// this is generally only possible in the Simcity4 data control mode
    #[error("Offset is 0 in compressed data control command")]
    BadOffset,
    /// Error indicating that the requested copy offset was larger than the length of the buffer to copy from
    /// this means that the copy function would try to copy
    /// from before the start of the buffer which is an illegal operation
    #[error("Offset went past start of buffer: buffer length `{0}`, offset `{1}`")]
    NegativePosition(usize, usize),
    /// Indicates that the decompressed file would be larger than the indicated size in the header
    /// this is important to prevent accidental massive memory usage
    #[error("Decompressed data is larger than decompressed size in header by `{0}` bytes")]
    BadLength(usize),
    /// Generic IO Error wrapper for when a generic IO error of some sort occurs in relation to
    /// the readers and writers.
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

/// Wrapper for Result specified to [RefPackError](crate::RefPackError)
pub type Result<T> = std::result::Result<T, Error>;
