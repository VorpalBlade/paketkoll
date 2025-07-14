# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.6.14] - 2025-07-14

### 🚀 Features

- Add missing MSRV to mtree2

## [0.6.13] - 2025-05-17

### 🚀 Features

- Properly handle NetBSD style mtrees and non-octal escapes
- Turn parse errors of invalid fields into Err

## [0.6.12] - 2025-05-05

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.6.11] - 2025-04-27

### 🚀 Features

- Add from_reader_with_cwd to set starting CWD

### 🐛 Bug fixes

- Fix handling of relative paths when cwd is /
- Make relative dir change cwd in mtree format

### 🧪 Testing

- Add test for relative directory changes
- Add snapshot testing using the `insta` crate for mtree2

## [0.6.10] - 2025-03-28

### ⚙️ Other stuff

- Format toml files
- Migrate to edition 2024

## [0.6.9] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

## [0.6.8] - 2024-09-20

### ⚙️ Other stuff

- Add crates.io package keywords & categories

## [0.6.7] - 2024-09-19

### ⚙️ Other stuff

- Change to some functions to const
- Enable clippy::manual_let_else
- Fix and enable various clippy lints
- Fix some cases of clippy::trivially-copy-pass-by-ref
- Enable clippy::use_self

## [0.6.6] - 2024-09-06

### ⚡ Performance improvements

- Arch doesn't use device nodes at all in the mtree files, outline the data

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.6.5] - 2024-08-17

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.6.4] - 2024-07-29

### ⚡ Performance improvements

- Improve time parsing
- Improve mtree parsing performance

### ⚙️ Other stuff

- Fix formatting

## [0.6.3] - 2024-07-27

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up unneeded paths for imported items
- Follow naming conventions
- Use RustRover Optimise imports

## [0.6.2] - 2024-07-25

### 🚜 Refactoring

- Unify and format Cargo.toml files

### ⚡ Performance improvements

- Use `smallvec` for temporary allocation in `MTreeLine`

## [0.6.1] - 2024-06-28

### 📚 Documentation

- Fix example in README
- Fix incorrect SPDX licence expression for mtree2

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
- Speed up parsing by not using our own hex parser

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

- Removed test that didn't test anything useful.

### New contributors

- @lucab
