# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [Unreleased]

## [5.0.0]
### Added
    - Provably optimal compression for the format utilizing deeper search techniques (#12) - @chieltbest
### Changed
    - all compression functions now take a `CompressionOptions` parameter. For now this just
      specifies compression mode to use. The default is a fast, but not quite real-time capable
      algorithm. A faster algorithm as well as an optimal one is provided. (#12) - @chieltbest


## [4.0.2]
### Documentation
 - Very minor typo correction in readme, bump for cargo.io update

## [4.0.1]

### Documentation
- Fix some missing docs in the readme and crate level docs, oops

## [4.0.0]

### Changed
- Large refactor to remove support for varying implementations of control codes. Turns out that these are actually
  entirely the same in every single implementation, including Simcity 4. The Nioso and SimsWiki are *inaccurate* on
  this.
- Formats no longer include a "Control" format
- `TheSims12` format was renamed to `Maxis`, and now is the intended format to use for Simcity 4
- `TheSims34` format was renamed to `SimEA`
- `byteorder` dependency was bumped to `1.5`
- removed dependency on `onlyerror`, no macros are used for errors

### Fixed
- Some range errors on reference were corrected - Thanks @lingeringwillx

### Documentation
- Major improvements to documentation to allow this repo to serve as a better "source of truth" on the topic of RefPack
  and QFS.

## [3.0.3] - 2023-06-23

### Documentation
- Some minor docs cleanups

## [3.0.2] - 2023-06-19

### Documentation
- Some minor docs cleanups

## [3.0.1] - 2023-03-30

### Changed
- Major performance improvements for `decompress` and `decompress_easy` via rewritten decompression,
  more aggressive inlining, and avoiding unnecessary allocations. Throughput should see minimum 80% 
  improvement and in best cases can be as much as 700% -@chieltbest
- IO Errors now actually output what the error was when printed. Whoops. -@actioninja
- Many new unit tests to harden functionality against regressions and test error cases -@actioninja 

### Fixed
- Potential nonspec compliant behavior on the `SimsEA` flags field was corrected, it now writes the
  magic bits in the middle. -@actioninja

## [3.0.0] - 2023-03-28

### Added
- Support for the `TheSims34` format, which is used by The Sims 3 and The Sims 4. -@chieltbest

### Changed
- **BREAKING:** Header mode's `LENGTH` field is now replaced by a function that returns the length
  of the header.

## [2.0.0]
Major rewrite to support multiple formats while also resolving them at compile time.

## [1.0.0]
First "production" version. This version was specialized for The Sims 2.


[Unreleased]: https://github.com/actioninja/refpack-rs/compare/v4.0.2...HEAD
[4.0.2]: https://github.com/actioninja/refpack-rs/compare/v4.0.1...v4.0.2
[4.0.1]: https://github.com/actioninja/refpack-rs/compare/v4.0.0...v4.0.1
[4.0.0]: https://github.com/actioninja/refpack-rs/compare/v3.0.3...v4.0.0
[3.0.3]: https://github.com/actioninja/refpack-rs/compare/v3.0.2...v3.0.3
[3.0.2]: https://github.com/actioninja/refpack-rs/compare/v3.0.1...v3.0.2
[3.0.1]: https://github.com/actioninja/refpack-rs/compare/v3.0.0...v3.0.1
[3.0.0]: https://github.com/actioninja/refpack-rs/compare/v2.0.0...v3.0.0
[2.0.0]: https://github.com/actioninja/refpack-rs/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/actioninja/refpack-rs/releases/tag/v1.0.0
