# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.2.8] - 2026-01-24

### 🩺 Diagnostics & output formatting

- Improve debug level diagnostics


## [0.2.7] - 2025-07-14

### ⚙️ Other stuff

- Update to Rust 1.88.0

## [0.2.6] - 2025-05-17

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.2.5] - 2025-05-05

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.2.4] - 2025-04-27

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.2.3] - 2025-03-28

### ⚙️ Other stuff

- Migrate to edition 2024
- Bump mimumum required Rust version to 1.85.0

## [0.2.2] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

## [0.2.1] - 2024-09-19

### ⚙️ Other stuff

- Change to some functions to const
- Add some must_use as suggested by clippy
- Fix and enable various clippy lints
- Fix some cases of clippy::trivially-copy-pass-by-ref
- Enable clippy::use_self

## [0.2.0] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.1.4] - 2024-08-17

### 🐛 Bug fixes

- Redo archive support to handle cases where an archive is not downloadable
- Fix incorrect application of diversions on Debian

### 🚜 Refactoring

- Make serde non-optional to simplify number of possible configurations

### ⚙️ Other stuff

- Move features to workspace manifest where possible

## [0.1.3] - 2024-08-03

### 🚜 Refactoring

- Use type aliases properly

### ⚙️ Other stuff

- Bump MSRV

## [0.1.2] - 2024-07-29

### 🚀 Features

- Try systemd lookup with /lib if /usr/lib fails, to support Debian

## [0.1.1] - 2024-07-27

### 🚀 Features

- Disk cache & archive-based files for Debian
- Get files from downloaded archives

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Reformat Cargo.toml files & imports
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is code that has been extracted from the `paketkoll_core` crate and is
the initial release of the `paketkoll_types` crate.

### 🚀 Features

- Handle removing unused dependencies
- Backend support for marking as dependency/manual install and asking to
  uninstall unused packages
- Create konfigkoll
- Add conversions to nix types
- Include device type in `Issue`
- Add new backend operation: Get original files
- Split out types from `paketkoll_core`

### 🐛 Bug fixes

- Handle some cases of restoring more correctly

### 🚜 Refactoring

- Move backend traits to paketkoll_types
- API fixes for konfigkoll
- Paketkoll changes for konfigkoll
- Revamp public API

### 📚 Documentation

- Reorganize README files

### ⚙️ Other stuff

- Add function to get "canonical" package ID
- Fix typos and lints from RustRover
