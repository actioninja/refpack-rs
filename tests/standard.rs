use std::io::Read;

use CompressionOptions::{Fast, Fastest, Optimal};
use paste::paste;
use refpack::format::{Format, Maxis, Reference, SimEA};
use refpack::{CompressionOptions, easy_compress, easy_decompress};

use crate::corpus::{get_compressed_file, get_uncompressed_file, prepare_corpus};

#[path = "../testing_corpus/corpus.rs"]
mod corpus;

fn test_corpus_symmetrical<F: Format>(name: &str, mode: CompressionOptions) {
    prepare_corpus().expect("Failed to generate corpus");
    let path = get_uncompressed_file(name).expect("Failed to get uncompressed file");
    let mut file = std::fs::File::open(path).expect("Failed to open corpus file");
    let mut uncompressed_buf = vec![];
    file.read_to_end(&mut uncompressed_buf)
        .expect("Failed to read corpus file");
    let compressed =
        easy_compress::<F>(&uncompressed_buf, mode).expect("Failed to compress corpus");
    let decompressed = easy_decompress::<F>(&compressed).expect("Failed to decompress corpus");
    assert!(
        decompressed == uncompressed_buf,
        "Decompressed output didn't match pre-compression input"
    );
}

fn test_corpus_decompress(name: &str) {
    prepare_corpus().expect("Failed to generate corpus");
    let uncompressed = get_uncompressed_file(name).expect("Failed to get uncompressed file");
    let mut uncompressed_file =
        std::fs::File::open(uncompressed).expect("Failed to open corpus file");
    let mut expected_decompressed = vec![];
    uncompressed_file
        .read_to_end(&mut expected_decompressed)
        .expect("Failed to read corpus file");
    let compressed = get_compressed_file(name).expect("Failed to get compressed file");
    let mut compressed_file =
        std::fs::File::open(compressed).expect("Failed to open compressed file");
    let mut compressed_buffer = vec![];
    compressed_file
        .read_to_end(&mut compressed_buffer)
        .expect("Failed to read compressed file");
    let decompressed = easy_decompress::<Reference>(&compressed_buffer)
        .expect("Failed to decompress compressed file");
    assert!(
        decompressed == expected_decompressed,
        "Decompressed didn't match uncompressed file"
    );
}

// Unfortunately due to basically any algorithmic change resulting in potentially different compressed
// output, a check of the actual compressed data against known "good" compressions is useless.
// Therefore, rather than this being a true integration test it's really more of a unit "Can compression
// work without exploding on real data" test.
fn test_corpus_compress<F: Format>(name: &str, mode: CompressionOptions) {
    prepare_corpus().expect("Failed to generate corpus");
    let path = get_uncompressed_file(name).expect("Failed to get uncompressed file");
    let mut file = std::fs::File::open(path).expect("Failed to open corpus file");
    let mut uncompressed_buf = vec![];
    file.read_to_end(&mut uncompressed_buf)
        .expect("Failed to read corpus file");
    easy_compress::<F>(&uncompressed_buf, mode).expect("Failed to compress corpus");
}

macro_rules! corpus_test_final {
    ($name:expr, $format:ident, $mode:expr) => {
        paste! {
            #[test]
            #[allow(nonstandard_style)]
            fn [<integration_ $name _ $format _ $mode _symmetrically_compresses>]() {
                test_corpus_symmetrical::<$format>( $name, $mode );
            }

            #[test]
            #[allow(nonstandard_style)]
            fn [<integration_ $name _ $format _ $mode _decompresses>]() {
                test_corpus_decompress( $name );
            }

            #[test]
            #[allow(nonstandard_style)]
            fn [<integration_ $name _ $format _ $mode _compresses>]() {
                test_corpus_compress::<$format>( $name, $mode );
            }
        }
    };
}

macro_rules! corpus_test_no_mode {
    ($name:expr, $format:ident) => {
        corpus_test_final!($name, $format, Fastest);
        corpus_test_final!($name, $format, Fast);
        corpus_test_final!($name, $format, Optimal);
    };
}

macro_rules! corpus_test {
    ($name:expr) => {
        corpus_test_no_mode!($name, Maxis);
        corpus_test_no_mode!($name, SimEA);
        corpus_test_no_mode!($name, Reference);
    };
}

corpus_test!("dickens");
corpus_test!("mozilla");
corpus_test!("mr");
corpus_test!("nci");
corpus_test!("ooffice");
corpus_test!("osdb");
corpus_test!("reymont");
corpus_test!("samba");
corpus_test!("sao");
corpus_test!("webster");
corpus_test!("x-ray");
corpus_test!("xml");
