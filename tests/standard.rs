use std::io::Read;

use CompressionOptions::{Fast, Fastest, Optimal};
use paste::paste;
use refpack::format::{Format, Maxis, Reference, SimEA};
use refpack::{CompressionOptions, easy_compress, easy_decompress};

use crate::corpus::{get_uncompressed_file, prepare_corpus};

#[path = "../testing_corpus/corpus.rs"]
mod corpus;

fn test_corpus<F: Format>(name: &str, mode: CompressionOptions) {
    prepare_corpus().expect("Failed to generate corpus");
    let path = get_uncompressed_file(name).expect("Failed to get uncompressed file");
    let mut file = std::fs::File::open(path).expect("Failed to open corpus file");
    let mut uncompressed_buf = vec![];
    file.read_to_end(&mut uncompressed_buf)
        .expect("Failed to read corpus file");
    let compressed =
        easy_compress::<F>(&uncompressed_buf, mode).expect("Failed to compress corpus");
    let decompressed = easy_decompress::<F>(&compressed).expect("Failed to decompress corpus");
    assert_eq!(decompressed, uncompressed_buf);
}

macro_rules! symmetrical_corpus_test_final {
    ($name:expr, $format:ident, $mode:expr) => {
        paste! {
            #[test]
            #[allow(nonstandard_style)]
            fn [<integration_ $name _ $format _ $mode _symmetrically_compresses>]() {
                test_corpus::<$format>( $name, $mode );
            }
        }
    };
}

macro_rules! symmetrical_corpus_test_no_mode {
    ($name:expr, $format:ident) => {
        symmetrical_corpus_test_final!($name, $format, Fastest);
        symmetrical_corpus_test_final!($name, $format, Fast);
        symmetrical_corpus_test_final!($name, $format, Optimal);
    };
}

macro_rules! symmetrical_corpus_test {
    ($name:expr) => {
        symmetrical_corpus_test_no_mode!($name, Maxis);
        symmetrical_corpus_test_no_mode!($name, SimEA);
        symmetrical_corpus_test_no_mode!($name, Reference);
    };
}

symmetrical_corpus_test!("dickens");
symmetrical_corpus_test!("mozilla");
symmetrical_corpus_test!("mr");
symmetrical_corpus_test!("nci");
symmetrical_corpus_test!("ooffice");
symmetrical_corpus_test!("osdb");
symmetrical_corpus_test!("reymont");
symmetrical_corpus_test!("samba");
symmetrical_corpus_test!("sao");
symmetrical_corpus_test!("webster");
symmetrical_corpus_test!("x-ray");
symmetrical_corpus_test!("xml");
