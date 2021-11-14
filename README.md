# refpack-rs
[![Rust Build and Test](https://github.com/actioninja/refpack-rs/actions/workflows/main.yml/badge.svg)](https://github.com/actioninja/refpack-rs/actions/workflows/main.yml)
![docs.rs](https://img.shields.io/docsrs/refpack)
![Crates.io](https://img.shields.io/crates/v/refpack)
[![GitHub license](https://img.shields.io/github/license/actioninja/refpack-rs)](https://github.com/actioninja/refpack-rs/blob/master/LICENSE.md)
![fuck it](https://img.shields.io/badge/fuck%20it-ship%20it-success)


# COMPRESSION IS NOT CURRENTLY IMPLEMENTED!

A rust crate for compressing and decompressing data in the RefPack format utilized by
many EA games of the early 2000s

More details on the refpack format can be found at [the niotso wiki](http://wiki.niotso.org/RefPack). The short explanation is that RefPack is a compression scheme loosely based on LZ77 compression.

The [Original Refpack Implementation](http://download.wcnews.com/files/documents/sourcecode/shadowforce/transfer/asommers/mfcapp_src/engine/compress/RefPack.cpp)
was referenced to ensure proper compatibility

# Usage

`refpack-rs` exposes two functions: `compress` and `decompress`, along with `easy` variants
with easier but less flexible of usage.

`compress` and `decompress` take mutable references to a buffer to read and write from,
that implements `std::io::Read` and `std::io::Write`, respectively.

`decompress` will read from the buffer until it encounters a stopcode (byte within (0xFC..=0xFF)),
while `compress` will read in the provided length.

### Example

```rust
let mut out_buf = Cursor::new(vec![]);
decompress(&mut source_reader, &mut out_buf)?;
```

The easy variants are `compress_easy` and `decompress_easy`, which take a `&[u8]` and return
a `Result<Vec<u8>, RefPackError>`.

Internally they simply call `compress` and `decompress` with a `Cursor` to the input and
output buffers, however they are more convenient to use in many cases.
