# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.5.8] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

### 🐛 Bug fixes

- Fix new clippy warnings on Rust 1.82

### 🩺 Diagnostics & output formatting

- Better errors when package archive reading fails

### ⚙️ Other stuff

- Fix clippy on newer rust

## [0.5.7] - 2024-09-20

### 🚀 Features

- Support uncompressed tar files in deb packages

## [0.5.6] - 2024-09-19

### ⚙️ Other stuff

- Change to some functions to const
- Enable clippy::manual_let_else
- Fix and enable various clippy lints
- Fix some cases of clippy::trivially-copy-pass-by-ref
- Enable clippy::use_self

## [0.5.5] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility (for Options)
- Switch to native eyre traits instead of anyhow compatibility
- Use anyhow::Result type alias consistently

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.5.4] - 2024-08-17

### 🚀 Features

- Switch from log to tracing

### 🐛 Bug fixes

- Redo archive support to handle cases where an archive is not downloadable
- Fix incorrect application of diversions on Debian
- For consistency on APT, do not consider suggests when removing unused packages.

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🚜 Refactoring

- Simplify features by making some code always included
- Make serde non-optional to simplify number of possible configurations

### ⚙️ Other stuff

- Move features to workspace manifest where possible
- Apply nightly clippy fixes

## [0.5.3] - 2024-08-03

### 🐛 Bug fixes

- Correct Pre-Depends handling on Debian

### 🚜 Refactoring

- Use type aliases properly

### ⚙️ Other stuff

- Bump MSRV

## [0.5.2] - 2024-07-29

### 🚀 Features

- Try systemd lookup with /lib if /usr/lib fails, to support Debian

### 🐛 Bug fixes

- Fix race condition on package manager
- Fix test
- Fix parsing of extended status for Debian

### ⚙️ Other stuff

- Fix build and better errors
- Make disabled package manager quieter and adjust other log levels

## [0.5.1] - 2024-07-27

### 🚀 Features

- Use --no-install-recommends for Debian backend
- Disk cache & archive-based files for Debian
- Get files from downloaded archives

### 🚜 Refactoring

- Refactor `original_files`

### 📚 Documentation

- Spell check code comments
- Spell check

### ⚙️ Other stuff

- Format strings using nightly rustfmt
- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up trailing ws
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.5.0] - 2024-07-25

### 🚀 Features

- Add Makefile to help install things, vendor deps by default
- Improve logging
- Handle removing unused dependencies
- Backend support for marking as dep/man install and asking to uninstall unused packages
- Rework, only single files backend allowed
- Disk cache for original file queries
- Create konfigkoll
- Add conversions to nix types
- Add `paketkoll owns` command to quickly find what package owns a file
- Include device type in issue
- Get original files
- Package manager transactions
- Split out types from paketkoll_core

### 🐛 Bug fixes

- Handle some cases of restoring more correctly
- Fix dependency parser for Arch Linux
- Fix visibility on types that are now exposed in issues

### 🚜 Refactoring

- Move backend traits to paketkoll_types
- API fixes for konfigkoll
- Remove unneeded Result
- Paketkoll changes for konfigkoll
- Unify and format Cargo.toml files
- Move some utility functions
- Revamp public API
- Use method for resolving string interning newtypes

### ⚡ Performance improvements

- Some small performance fixes

### ⚙️ Other stuff

- Clippy fixes
- Fix typos and lints from RustRover

## [0.4.1] - 2024-06-28

### 🚀 Features

- Extract vendored mtree code into a new mtree2 library
- Include more info in issues
- Improve systemd-tmpfiles backend parsing on duplicated entry

### 📚 Documentation

- Document some of the non-public code

## [0.4.0] - 2024-06-26

### 🚀 Features

- Add file backend for systemd-tmpfiles.d to paketkoll
- Add JSON output (implements [#3](https://github.com/VorpalBlade/paketkoll/pull/3))
- Add flatpak package listing backend
- Add package backend for Debian
- Add listing of installed packages

### 🐛 Bug fixes

- Fix broken Debian status parsing
- Fix Debian status parser (not all packages has description)
- Set correct feature flags for flate2

### 🚜 Now more maintainable (refactor)

- Refactor API of core crate

### 📚 Documentation

- Add MSRV policy
- Add missing API docs

### ⚡ Performance

- Improve Debian status parsing speed
- Optimise mtree library for actual observed data patterns
- Optimise decode_escapes to avoid branchy code in the common case

### ⚙️ Other stuff

- Tweak serde field and variant names
- Clean up Cargo.toml files
- Remove no longer needed allowing dead code
- *(fix)* Fix build without Arch Linux backend
- Fix formatting
- Enable additional lints
- *(lints)* Enable additional lints

## [0.3.1] - 2024-03-14

### ⚡ Performance

- Save 20 ms on Arch Linux by switching to faster hex parsing

### ⚙️ Other stuff

- Fix new warning on nightly

## [0.3.0] - 2024-03-10

### 🚀 Features

- [**breaking**] Add scanning for unmanaged files

### ⚙️ Other stuff

- Fix nightly clippy lint
- Code cleanup

## [0.2.0] - 2024-02-29

### 🚀 Features

- Ability to limit which crates to scan

### 📚 Documentation

- Add categories & keywords

### ⚙️ Other stuff

- Update dependencies

## [0.1.1] - 2024-02-26

### 🐛 Bug fixes

- Fix nightly warnings
- Temporary allow dead code
- Disable doctest on vendored mtree

### 📚 Documentation

- Add links to README
