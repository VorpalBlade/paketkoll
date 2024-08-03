# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

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
