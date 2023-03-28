# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.0.0] - 2023-03-28

### Added
- Support for the `TheSims34` format, which is used by The Sims 3 and The Sims 4.

### Changed
- **BREAKING:** Header mode's `LENGTH` field is now replaced by a function that returns the length
  of the header.

## [2.0.0]
Major rewrite to support multiple formats while also resolving them at compile time.

## [1.0.0]
First "production" version. This version was specialized for The Sims 2.


[Unreleased]: https://github.com/actioninja/refpack-rs/compare/v3.0.0...HEAD
[3.0.0]: https://github.com/actioninja/refpack-rs/compare/v2.0.0...v3.0.0
[2.0.0]: https://github.com/actioninja/refpack-rs/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/actioninja/refpack-rs/releases/tag/v1.0.0
