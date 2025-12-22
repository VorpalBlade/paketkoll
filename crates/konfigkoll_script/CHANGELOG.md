# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.1.14] - 2025-12-22

### ⚙️ Other stuff

- Update Cargo.toml dependencies


## [0.1.13] - 2025-07-14

### ⚙️ Other stuff

- Update to Rust 1.88.0

## [0.1.11] - 2025-05-05

### 🚜 Refactoring

- Clean up inter-module cyclic dependencies

## [0.1.10] - 2025-04-27

### ⚙️ Other stuff

- Clippy fixes with 1.86

## [0.1.9] - 2025-03-28

### ⚙️ Other stuff

- Format toml files
- Migrate to edition 2024
- Bump mimumum required Rust version to 1.85.0

## [0.1.8] - 2024-12-16

### 🚀 Features

- Add support for locked users in sysusers parser
- Prepare workspace hack with cargo-hakari

### 🐛 Bug fixes

- Fix new clippy warnings on Rust 1.82

### 🩺 Diagnostics & output formatting

- Improve parse errors from sysusers

## [0.1.7] - 2024-09-20

### ⚙️ Other stuff

- Add crates.io package keywords & categories

## [0.1.6] - 2024-09-19

### 🩺 Diagnostics & output formatting

- Make ID update less chatty (info isn't usually relevant)

### ⚙️ Other stuff

- Change to some functions to const
- Enable clippy::ignored_unit_patterns
- Add some must_use as suggested by clippy
- Enable clippy::manual_let_else
- Fix and enable various clippy lints
- Fix some cases of clippy::trivially-copy-pass-by-ref
- Enable clippy::use_self

## [0.1.5] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Improve formatting of Rune runtime errors
- Add spans to async functions exposed to Rune (should help get better errors with color-eyre)
- Switch from anyhow to color-eyre for better (and prettier) error messages
- Switch to custom error wrapper to better traverse the Rune callstacks

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.1.4] - 2024-08-17

### 🚀 Features

- Include package name for the modified file (where possible) in a comment when saving

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🚜 Refactoring

- Make serde non-optional to simplify number of possible configurations

### ⚙️ Other stuff

- Remove empty feature table
- Move features to workspace manifest where possible

## [0.1.3] - 2024-08-03

### 🚀 Features

- Error check path in commands for common mistakes
- Early/sensitive configurations can now be globs
- Systemd paths differ on Debian
- Align parameter order between groups and users
- Add ability to set path to nologin

### 🐛 Bug fixes

- Systemd paths are now acquired by running `systemd-paths` on first access
- Gshadow- and shadow- should also be sensitive

### 🚜 Refactoring

- Use type aliases properly

### ⚙️ Other stuff

- Bump MSRV

## [0.1.2] - 2024-07-29

### 🚀 Features

- Save prefix (for when you wrap cmds in a context object)
- Try systemd lookup with /lib if /usr/lib fails, to support Debian

### ⚙️ Other stuff

- Better error message
- Fix build and better errors
- Make disabled package manager quieter and adjust other log levels

## [0.1.1] - 2024-07-27

### 🚜 Refactoring

- Make `konfigkoll_script` independent of `konfigkoll_core`

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Format strings using nightly rustfmt
- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up trailing ws
- Reformat Cargo.toml files & imports
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the initial release of the `konfigkoll_script` crate.

### 🚀 Features

- Stop and print LineEditor action
- Passwd sanity checking
- Improve defaults
- Extend API in rune process module significantly
- Vendor rune process module
- Broaden host_fs to filesystem
- User & group API
- Rework, only single files backend allowed
- Disk cache for original file queries
- Simple line editor (rust + rune API)
- Create konfigkoll

### 🚜 Refactoring

- Clean up Rune API

### 📚 Documentation

- Document some Rune modules better
- Document process module better

### ⚙️ Other stuff

- Minor documentation fixes
- Fix build warnings
- Copyright comment in process.rs
- Format process module
