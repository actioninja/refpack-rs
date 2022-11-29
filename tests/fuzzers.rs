////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use proptest::prelude::*;
use refpack::format::Reference;
use refpack::{easy_compress, easy_decompress};
use refpack_sys::{refpack_compress, refpack_decompress};
use test_strategy::proptest;

#[proptest]
fn rust_compression_symmetrical(
    #[strategy(proptest::collection::vec(any::<u8>(), (100..=1_000)))] input: Vec<u8>,
) {
    let mut cloned = input.clone();
    let compressed = refpack_compress(&mut cloned);

    let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

    prop_assert_eq!(input, decompressed);
}

#[proptest]
fn rust_decompression_symmetrical(
    #[strategy(proptest::collection::vec(any::<u8>(), (100..=1_000)))] input: Vec<u8>,
) {
    let mut compressed = easy_compress::<Reference>(&input).unwrap();
    println!("compressed: {compressed:?}");

    let decompressed = refpack_decompress(&mut compressed);

    prop_assert_eq!(input, decompressed);
}
