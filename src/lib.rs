////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! A very overengineered rust crate for compressing and decompressing data in the RefPack format
//! utilized by many EA games of the early 2000s
//!
//! RefPack is a nonstandardized format that varied greatly in exact encoding and implementation.
//! `refpack` uses a `Format` system to specify different encoding formats. This is implemented via
//! generic trait parameters that get monomorphized down to static dispatch.
//!
//! Put simply, this means that you get the benefit of being able to use any format however you like
//! without any performance overhead from dynamic dispatch, as well as being able to implement your
//! own arbitrary formats that are still compatible with the same compression algorithms.
//!
//! More details on the refpack format can be found at [the niotso wiki](http://wiki.niotso.org/RefPack). The short explanation is that RefPack is a compression scheme loosely based on LZ77 compression.
//!
//! The [Original Refpack Implementation](http://download.wcnews.com/files/documents/sourcecode/shadowforce/transfer/asommers/mfcapp_src/engine/compress/RefPack.cpp)
//! was referenced to ensure proper compatibility
//!
//! # Usage
//!
//! `refpack-rs` exposes two functions: `compress` and `decompress`, along with `easy` variants
//! with easier but less flexible of usage.
//!
//! `compress` and `decompress` take mutable references to a buffer to read and write from,
//! that implements `std::io::Read` and `std::io::Write`, respectively.
//!
//! `decompress` will read from the buffer until it encounters a stopcode (byte within (0xFC..=0xFF)),
//! while `compress` will read in the provided length.
//!
//! all compression and decompression functions accept one generic argument constrained to the
//! [Format](crate::format::Format) trait. Implementations should be a unit or "unconstructable"
//! (one inaccessible `()` member to prevent construction), and define a pair of how to interpret
//!
//!
//! ## Implementations
//!
//! | Format | Games | Control | Header |
//! |--------|-------|---------|--------|
//! | [Reference](crate::format::Reference) | Various 90s Origin Software and EA games | [Reference](crate::data::control::mode::Reference) | [Reference](crate::header::mode::Reference) |
//! | [TheSims12](crate::format::TheSims12) | The Sims, The Sims Online, The Sims 2 | [Reference](crate::data::control::mode::Reference) | [Maxis](crate::header::mode::Maxis) |
//! | [Simcity4](crate::format::Simcity4) | Simcity 4 | [Simcity4](crate::data::control::mode::Simcity4) | [Maxis](crate::header::mode::Maxis) |
//! | [TheSims34](crate::format::TheSims34) | The Sims 3, The Sims 4 | [Reference](crate::data::control::mode::Reference) | [Maxis2](crate::header::mode::Maxis2) |
//!
//!
//! ### Example
//!
//! ```
//! use std::io::Cursor;
//! use std::io::Seek;
//! use refpack::format::Reference;
//!
//! # fn main() {
//! let mut source_reader = Cursor::new(b"Hello World!".to_vec());
//! let mut out_buf = Cursor::new(vec![]);
//! refpack::compress::<Reference>(source_reader.get_ref().len(), &mut source_reader, &mut out_buf).unwrap();
//! # }
//! ```
//!
//! The easy variants are `compress_easy` and `decompress_easy`, which take a `&[u8]` and return
//! a `Result<Vec<u8>, RefPackError>`.
//!
//! Internally they simply call `compress` and `decompress` with a `Cursor` to the input and
//! output buffers, however they are more convenient to use in many cases.

#![warn(clippy::pedantic, clippy::cargo)]
// Due to the high amount of byte conversions, sometimes intentional lossy conversions are necessary.
#![allow(clippy::cast_possible_truncation)]
// same as above
#![allow(clippy::cast_lossless)]
// Annoying and wrong, RefPack is a compression scheme.
#![allow(clippy::doc_markdown)]
// Default::default() is more idiomatic imo
#![allow(clippy::default_trait_access)]
// too many lines is a dumb metric
#![allow(clippy::too_many_lines)]
// causes weirdness with header and reader
#![allow(clippy::similar_names)]

pub mod data;
mod error;
pub mod format;
pub mod header;

pub use crate::data::compression::{compress, easy_compress};
pub use crate::data::decompression::{decompress, easy_decompress};
pub use crate::error::{Error as RefPackError, Result as RefPackResult};

#[cfg(test)]
mod test {
    use proptest::collection::vec;
    use proptest::num::u8;
    use proptest::prop_assert_eq;
    use test_strategy::proptest;

    use crate::format::{Reference, Simcity4, TheSims12, TheSims34};
    use crate::{easy_compress, easy_decompress};

    #[proptest]
    fn reference_symmetrical_read_write(#[strategy(vec(u8::ANY, 1..1000))] data: Vec<u8>) {
        let compressed = easy_compress::<Reference>(&data).unwrap();

        let got = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    #[proptest]
    fn sims12_symmetrical_read_write(#[strategy(vec(u8::ANY, 1..1000))] data: Vec<u8>) {
        let compressed = easy_compress::<TheSims12>(&data).unwrap();

        let got = easy_decompress::<TheSims12>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    #[proptest]
    fn simcity4_symmetrical_read_write(#[strategy(vec(u8::ANY, 1..1000))] data: Vec<u8>) {
        let compressed = easy_compress::<Simcity4>(&data).unwrap();

        let got = easy_decompress::<Simcity4>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    #[proptest]
    fn sims34_symmetrical_read_write(
        // this should include inputs of > 16mb, but testing those inputs is extremely slow
        #[strategy(vec(u8::ANY, 1..1000))] data: Vec<u8>,
    ) {
        let compressed = easy_compress::<TheSims34>(&data).unwrap();

        let got = easy_decompress::<TheSims34>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }
}
