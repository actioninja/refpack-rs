////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! A very overengineered rust crate for compressing and decompressing data in
//! the RefPack format utilized by many EA games of the early 2000s
//!
//! # RefPack
//! RefPack, also known as QFS, is a semi-standardized compression format
//! utilized by many games published by Electronic Arts from the 90s to the late
//! 2000s. In many cases, it was deployed with a custom header format.
//!
//! ## Structure
//! RefPack shares many similarities with lz77 compression; it is a lossless
//! compression format which relies on length-distance pairs to existing bytes
//! within the decompression buffer. Where it differs from lz77 is that rather
//! than a single format for "Literal" control codes and "Pointer" control
//! codes, RefPack uses 4 distinct control codes for different sizes of pointers
//! and literal blocks. A fifth control code is also present to indicate end of
//! stream rather than requiring a size to be specified before decompression.
//!
//! ### Codes
//! RefPack utilizes one "Literal" bytes-only control code similar to lz77, but
//! with limited precision to multiples of 4. The remaining three control codes
//! are varying sizes of "Pointer" control codes, for small, medium, and large
//! back-references and lengths. The limited precision of the "Literal" control
//! code is compensated for via "Pointer" control codes also having the ability
//! to write up to 3 literal bytes to the stream
//!
//! See [Command](crate::data::control::Command) for further details.
//!
//! ## Decompression
//! Decompression simply requires reading from a stream of `RefPack` data until
//! a stopcode is reached.
//!
//! See [decompression](crate::data::decompression) for further details
//!
//!
//! ## Compression
//! Compressing via RefPack is largely similar to lz77 compression algorithms,
//! and involves a sliding window over the data to search for repeating blocks,
//! and then writing to the stream as the previously specified codes.
//!
//! See [compression](crate::data::compression) for further details
//!
//! ## Headers
//! While the actual data block of RefPack has only one known implementation,
//! multiple types of headers for the library have been identified.
//!
//! ## Other Implementations
//!
//! RefPack has been implemented in various other languages and for various
//! games:
//!
//! - [RefPack.cpp (download)](http://download.wcnews.com/files/documents/sourcecode/shadowforce/transfer/asommers/mfcapp_src/engine/compress/RefPack.cpp):
//!   Original canonical implementation of RefPack by Frank Barchard for Origin
//!   Software. Utilized by some early Origin Software games.
//! - [JDBPF](https://github.com/actioninja/JDBPF/blob/90644a3286580aa7676779a2d2e5a3c9de9a31ff/src/ssp/dbpf/converter/DBPFPackager.java#L398C9-L398C9):
//!   Early Simcity 4 Java Library for reading DBPF files which utilize RefPack
//! - [JDBPFX](https://github.com/actioninja/JDBPF/blob/90644a3286580aa7676779a2d2e5a3c9de9a31ff/src/ssp/dbpf/converter/DBPFPackager.java#L398C9-L398C9):
//!   Later currently maintained fork of JDBPF
//! - [DBPFSharp](https://github.com/0xC0000054/DBPFSharp/blob/3038b9c15b0ddd3ccfb4b72bc6ac4541eee677fb/src/DBPFSharp/QfsCompression.cs#L100):
//!   Simcity 4 DBPF Library written in C#
//! - [Sims2Tools](https://github.com/whoward69/Sims2Tools/blob/0baaf2dce985474215cf0f64096a8dd9950c2757/DbpfLibrary/Utils/Decompressor.cs#L54C1-L54C1):
//!   Sims 2 DBPF Library written in C#
//!
//!
//! # This Crate
//!
//! This crate is a rust implementation designed to compress and decompress
//! refpack data with any header format. It uses generics to support arbitrary
//! header formats to allow pure usage of this library without having to write
//! "glue" code to parse header info.
//!
//! Put simply, this means that you get the benefit of being able to use any
//! format however you like without any performance overhead from dynamic
//! dispatch, as well as being able to implement your own arbitrary formats that
//! are still compatible with the same compression algorithms.
//!
//! # Usage
//!
//! `refpack-rs` exposes two functions: `compress` and `decompress`, along with
//! `easy` variants with easier but less flexible of usage.
//!
//! `compress` and `decompress` take mutable references to a buffer to read and
//! write from, that implements `std::io::Read` and `std::io::Write`,
//! respectively.
//!
//! `decompress` will read from the buffer until it encounters a stopcode (byte
//! within (0xFC..=0xFF)), while `compress` will read in the provided length.
//!
//! all compression and decompression functions accept one generic argument
//! constrained to the [Format](crate::format::Format) trait. Implementations
//! be "unconstructable" types, with the recommended type being an empty enum.
//!
//! ## Implementations
//!
//! | Format | Games | Header |
//! |--------|-------|--------|
//! | [Reference](crate::format::Reference) | Various 90s Origin Software and EA games | [Reference](crate::header::Reference) |
//! | [Maxis](crate::format::Maxis) | The Sims, The Sims Online, Simcity 4, The Sims 2 | [Maxis](crate::header::Maxis) |
//! | [SimEA](crate::format::SimEA) | The Sims 3, The Sims 4 | [SimEA](crate::header::SimEA) |
//!
//!
//! ### Example
//!
//! ```
//! use std::io::{Cursor, Seek};
//!
//! use refpack::format::Reference;
//!
//! # fn main() {
//! let mut source_reader = Cursor::new(b"Hello World!".to_vec());
//! let mut out_buf = Cursor::new(vec![]);
//! refpack::compress::<Reference>(
//!     source_reader.get_ref().len(),
//!     &mut source_reader,
//!     &mut out_buf,
//!     refpack::CompressionOptions::Optimal,
//! )
//! .unwrap();
//! # }
//! ```
//!
//! The easy variants are `compress_easy` and `decompress_easy`, which take a
//! `&[u8]` and return a `Result<Vec<u8>, RefPackError>`.
//!
//! Internally they simply call `compress` and `decompress` with a `Cursor` to
//! the input and output buffers, however they are more convenient to use in
//! many cases.

// I like clippy to yell at me about everything!
#![warn(clippy::pedantic, clippy::cargo)]
// Due to the high amount of byte conversions, sometimes intentional lossy conversions are
// necessary.
#![allow(clippy::cast_possible_truncation)]
// same as above
#![allow(clippy::cast_lossless)]
// and above
#![allow(clippy::cast_possible_wrap)]
// above
#![allow(clippy::cast_precision_loss)]
// Annoying and wrong, RefPack is a compression scheme.
#![allow(clippy::doc_markdown)]
// Default::default() is more idiomatic imo
#![allow(clippy::default_trait_access)]
// too many lines is a dumb metric
#![allow(clippy::too_many_lines)]
// causes weirdness with header and reader
#![allow(clippy::similar_names)]
// all uses of #[inline(always)] have been benchmarked thoroughly
#![allow(clippy::inline_always)]

pub mod data;
mod error;
pub mod format;
pub mod header;

pub use crate::data::compression::{compress, easy_compress, CompressionOptions};
pub use crate::data::decompression::{decompress, easy_decompress};
pub use crate::error::{Error as RefPackError, Result as RefPackResult};

#[cfg(test)]
mod test {
    use proptest::collection::vec;
    use proptest::num::u8;
    use proptest::prop_assert_eq;
    use test_strategy::proptest;

    use crate::data::compression::CompressionOptions;
    use crate::format::{Maxis, Reference, SimEA};
    use crate::{easy_compress, easy_decompress};

    #[proptest]
    fn reference_symmetrical_read_write(
        #[strategy(vec(0..=1u8, 1..1000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<Reference>(&data, compression_options).unwrap();

        let got = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    // excluded from normal runs to avoid massive CI runs, *very* long running
    // tests
    #[proptest]
    #[ignore]
    fn reference_large_symmetrical_read_write(
        #[strategy(vec(0..=1u8, 1..16_000_000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<Reference>(&data, compression_options).unwrap();

        let got = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    #[proptest]
    fn maxis_symmetrical_read_write(
        #[strategy(vec(0..=1u8, 1..1000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<Maxis>(&data, compression_options).unwrap();

        let got = easy_decompress::<Maxis>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    // excluded from normal runs to avoid massive CI runs, *very* long running
    // tests
    #[proptest]
    #[ignore]
    fn maxis_large_symmetrical_read_write(
        #[strategy(vec(0..=1u8, 1..16_000_000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<Maxis>(&data, compression_options).unwrap();

        let got = easy_decompress::<Maxis>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    #[proptest]
    fn simea_symmetrical_read_write(
        // this should include inputs of > 16mb, but testing those inputs is extremely slow
        #[strategy(vec(0..=1u8, 1..1000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<SimEA>(&data, compression_options).unwrap();

        let got = easy_decompress::<SimEA>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }

    // excluded from normal runs to avoid massive CI runs, *very* long running
    // tests
    #[proptest]
    #[ignore]
    fn simea_large_symmetrical_read_write(
        #[strategy(vec(0..=1u8, 1..16_000_000))] data: Vec<u8>,
        compression_options: CompressionOptions,
    ) {
        let compressed = easy_compress::<SimEA>(&data, compression_options).unwrap();

        let got = easy_decompress::<SimEA>(&compressed).unwrap();

        prop_assert_eq!(data, got);
    }
}
