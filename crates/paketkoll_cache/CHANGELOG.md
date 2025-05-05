# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.2.10] - 2025-05-05

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_types

## [0.2.9] - 2025-04-27

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.2.8] - 2025-03-28

### ⚙️ Other stuff

- Migrate to edition 2024
- Bump mimumum required Rust version to 1.85.0

## [0.2.7] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

## [0.2.6] - 2024-09-20

### 🐛 Bug fixes

- Do not wrap the error in the caching layer, it breaks the systemd path fallback logic

## [0.2.5] - 2024-09-19

### ⚙️ Other stuff

- Change to some functions to const
- Fix and enable various clippy lints

## [0.2.4] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility (for Options)
- Switch to native eyre traits instead of anyhow compatibility

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.2.3] - 2024-08-17

### 🐛 Bug fixes

- Redo archive support to handle cases where an archive is not downloadable
- Fix incorrect application of diversions on Debian

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🚜 Refactoring

- Make serde non-optional to simplify number of possible configurations

### ⚙️ Other stuff

- Move features to workspace manifest where possible

## [0.2.2] - 2024-08-03

### 🩺 Diagnostics & output formatting

- Better cache message when package is not installed
- Improve cache key lookup error message

### 🚜 Refactoring

- Use type aliases properly

### ⚙️ Other stuff

- Bump MSRV

## [0.2.1] - 2024-07-29

### 🚀 Features

- Try systemd lookup with /lib if /usr/lib fails, to support Debian

## [0.2.0] - 2024-07-27

### 🚀 Features

- Disk cache & archive-based files for Debian
- Cache for getting files from downloaded archives
- Get files from downloaded archives

### 🚜 Refactoring

- Restructure `paketkoll_cache`

### ⚡ Performance improvements

- Enable refresh on disk cache

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Reformat Cargo.toml files & imports
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the first release of the `paketkoll_cache` crate.
