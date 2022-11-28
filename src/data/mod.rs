////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

//! Module for things relating the actual compressed data block. Anything past the header info,
//! the actual compression algorithms themselves, control codes, etc.

pub mod compression;
pub mod control;
pub mod decompression;

pub(crate) fn copy_within_slice(v: &mut [impl Copy], from: usize, to: usize, len: usize) {
    if from > to {
        let (dst, src) = v.split_at_mut(from);
        dst[to..to + len].copy_from_slice(&src[..len]);
    } else {
        let (src, dst) = v.split_at_mut(to);
        dst[..len].copy_from_slice(&src[from..from + len]);
    }
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use test_strategy::proptest;

    use crate::format::Reference;
    use crate::{easy_compress, easy_decompress};

    #[proptest(ProptestConfig { cases: 100_000, ..Default::default() })]
    fn symmetrical_compression(#[filter(#input.len() > 0)] input: Vec<u8>) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }

    #[proptest]
    fn large_input_compression(
        #[strategy(proptest::collection::vec(any::<u8>(), (100_000..=500_000)))] input: Vec<u8>,
    ) {
        let _unused = easy_compress::<Reference>(&input).unwrap();
    }

    #[proptest(ProptestConfig {
    max_shrink_iters: 1_000_000,
    ..Default::default()
    })]
    fn symmetrical_compression_large_input(
        #[strategy(proptest::collection::vec(any::<u8>(), (2_000..=2_000)))] input: Vec<u8>,
    ) {
        let compressed = easy_compress::<Reference>(&input).unwrap();
        let decompressed = easy_decompress::<Reference>(&compressed).unwrap();

        prop_assert_eq!(input, decompressed);
    }
}
