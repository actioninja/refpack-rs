# refpack-rs

[![Rust Build and Test](https://github.com/actioninja/refpack-rs/actions/workflows/check-and-test.yml/badge.svg)](https://github.com/actioninja/refpack-rs/actions/workflows/check-and-test.yml)
[![docs.rs](https://img.shields.io/docsrs/refpack)](https://docs.rs/refpack/latest/refpack/)
[![Crates.io](https://img.shields.io/crates/v/refpack)](https://crates.io/crates/refpack)
[![GitHub license](https://img.shields.io/github/license/actioninja/refpack-rs)](https://github.com/actioninja/refpack-rs/blob/master/LICENSE.md)
![fuck it](https://img.shields.io/badge/fuck%20it-ship%20it-success)
[![Coverage Status](https://coveralls.io/repos/github/actioninja/refpack-rs/badge.svg?branch=master)](https://coveralls.io/github/actioninja/refpack-rs?branch=master)


<!-- cargo-rdme start -->

A very overengineered rust crate for compressing and decompressing data in the RefPack format
utilized by many EA games of the early 2000s

RefPack is a nonstandardized format that varied greatly in exact encoding and implementation.
`refpack` uses a `Format` system to specify different encoding formats. This is implemented via
generic trait parameters that get monomorphized down to static dispatch.

Put simply, this means that you get the benefit of being able to use any format however you like
without any performance overhead from dynamic dispatch, as well as being able to implement your
own arbitrary formats that are still compatible with the same compression algorithms.

More details on the refpack format can be found at [the niotso wiki](http://wiki.niotso.org/RefPack). The short explanation is that RefPack is a compression scheme loosely based on LZ77 compression.

The [Original Refpack Implementation](http://download.wcnews.com/files/documents/sourcecode/shadowforce/transfer/asommers/mfcapp_src/engine/compress/RefPack.cpp)
was referenced to ensure proper compatibility

## Usage

`refpack-rs` exposes two functions: `compress` and `decompress`, along with `easy` variants
with easier but less flexible of usage.

`compress` and `decompress` take mutable references to a buffer to read and write from,
that implements `std::io::Read` and `std::io::Write`, respectively.

`decompress` will read from the buffer until it encounters a stopcode (byte within (0xFC..=0xFF)),
while `compress` will read in the provided length.

all compression and decompression functions accept one generic argument constrained to the
[Format](https://docs.rs/refpack/latest/refpack/format/trait.Format.html) trait. Implementations should be a unit or "unconstructable"
(one inaccessible `()` member to prevent construction), and define a pair of how to interpret


### Implementations

| Format | Games | Header |
|--------|-------|--------|
| [Reference](https://docs.rs/refpack/latest/refpack/format/struct.Reference.html) | Various 90s Origin Software and EA games | Reference |
| [TheSims12](https://docs.rs/refpack/latest/refpack/format/struct.TheSims12.html) | The Sims, The Sims Online, The Sims 2, SimCity 4 | Maxis |
| [TheSims34](https://docs.rs/refpack/latest/refpack/format/struct.TheSims34.html) | The Sims 3, The Sims 4 | SimEA |


#### Example

```rust
use std::io::Cursor;
use std::io::Seek;
use refpack::format::Reference;

let mut source_reader = Cursor::new(b"Hello World!".to_vec());
let mut out_buf = Cursor::new(vec![]);
refpack::compress::<Reference>(source_reader.get_ref().len(), &mut source_reader, &mut out_buf).unwrap();
```

The easy variants are `compress_easy` and `decompress_easy`, which take a `&[u8]` and return
a `Result<Vec<u8>, RefPackError>`.

Internally they simply call `compress` and `decompress` with a `Cursor` to the input and
output buffers, however they are more convenient to use in many cases.

<!-- cargo-rdme end -->
