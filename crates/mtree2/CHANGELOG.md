# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages.

## [0.6.0] - 2024-06-28

### Features

- Extract vendored mtree code from paketkoll_core into a new mtree2 library
- Updated dependencies (semver major version)
- [**breaking**] Keep FileMode as numeric, implement support for sticky bit. \
  This is also a performance improvement thanks to reduced size of data structures.
- Resolve escapes in file paths

### Bug fixes

- Fix future-deprecation warning
- Fix compiler warning in mtree2 due to unused code
- Fix clippy warnings

### Performance improvements

- [**breaking**] Unix UID & GID are actually only 32-bit. This is also exposed in Rust standard
  library types, and there is no point in supporting more than that.
- [**breaking**] Optimise type sizes based on which fields are actually present in Arch Linux mtrees.\
  This avoids reserving a lot of space for the uncommon cases.
  Params has gone from 672 bytes to 528 bytes
- Cut temporary allocations by 2/3\
  More than 1 million temporary allocations just went away by using `ok_or_else` instead of `ok_or`!
- Speed up parsing by by not using our own hex parser

### Refactoring

- Update to edition 2021
- Fix formatting with "cargo fmt"
- Drop no longer needed newtype_array dependency

## 0.4.1

NOTE: This refers to a release from the previous mtree-rs project that mtree2 is a fork of.

### Added

- Added more documentation

### Changed

- Change some potential panics to errors.

### Removed

- Removed test that didn't test anyting useful.

### New contributors

- @lucab
