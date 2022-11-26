////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! A rust crate for compressing and decompressing data in the RefPack format utilized by
//! many EA games of the early 2000s
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
//! ### Example
//!
//! ```rust
//! use std::io::Cursor;
//! use std::io::Seek;
//!
//! let mut source_reader = Cursor::new(b"Hello World!".to_vec());
//! let mut out_buf = Cursor::new(vec![]);
//! refpack::compress(source_reader.get_ref().len(), &mut source_reader, &mut out_buf).unwrap();
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

pub use crate::error::{Error as RefPackError, Result as RefPackResult};

/*
#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::*;

    #[proptest(ProptestConfig { cases: 100_000, ..Default::default() })]
    fn symmetrical_compression(#[filter(#input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress(&input).unwrap();
        let decompressed = easy_decompress(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[proptest]
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::<u8>(), (100_000..=500_000)))] input: Vec<u8>,
    ) {
        let _unused = easy_compress(&input).unwrap();
    }

    #[proptest(ProptestConfig {
        max_shrink_iters: 1_000_000,
        ..Default::default()
    })]
    fn symmetrical_compression_large_input(
        #[strategy(proptest::collection::vec(any::<u8>(), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress(&input).unwrap();
        let decompressed = easy_decompress(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[test]
    fn blah() {
        let test = easy_compress(&[0x04, 0x23, 0x13, 0x98]).unwrap();
        println!("{:X?}", test);
    }
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use super::*;
    use crate::format::Reference;

    #[proptest]
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::<u8>(), (100_000..=500_000)))] input: Vec<u8>,
    ) {
        let _unused = easy_compress::<Reference>(&input).unwrap();
    }
}
 */
