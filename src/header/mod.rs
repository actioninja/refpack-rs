////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Module for things relating to the header of the data which include
//! decompressed length, sometimes flags or a magic number, and sometimes
//! compressed length.

use std::io::{Read, Seek, Write};

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use test_strategy::Arbitrary;

use crate::header::mode::Mode;
use crate::RefPackResult;

pub mod mode;

/// Magic number in the header. This immediately follows a flag field in most
/// cases
pub const MAGIC: u8 = 0xFB;

#[cfg(test)]
#[derive(Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct HeaderArgs {
    decompressed_limit: u32,
    compressed_limit: Option<u32>,
}

#[cfg(test)]
fn generate_compressed_length(compressed_limit: Option<u32>) -> BoxedStrategy<Option<u32>> {
    if let Some(compressed_limit) = compressed_limit {
        (0..=compressed_limit).prop_map(Some).boxed()
    } else {
        Just(None).boxed()
    }
}

/// represents a decoded header
#[derive(Eq, PartialEq, Debug, Default, Copy, Clone)]
#[cfg_attr(test, derive(Arbitrary))]
#[cfg_attr(test, arbitrary(args = HeaderArgs))]
pub struct Header {
    #[cfg_attr(test, strategy(0..=args.decompressed_limit))]
    pub decompressed_length: u32,
    #[cfg_attr(test, strategy(generate_compressed_length(args.compressed_limit)))]
    pub compressed_length: Option<u32>,
}

impl Header {
    /// # Errors
    /// - [RefPackError::BadMagic]: Invalid magic number read
    /// - [RefPackError::Io]: Generic IO Error
    pub fn read<M: Mode>(reader: &mut (impl Read + Seek)) -> RefPackResult<Header> {
        M::read(reader)
    }

    /// # Errors
    /// - [RefPackError::Io] if the write fails due to a generic IO Error
    pub fn write<M: Mode>(self, writer: &mut (impl Write + Seek)) -> RefPackResult<()> {
        M::write(self, writer)
    }
}
