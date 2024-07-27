# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.1.3] - 2024-07-27

### 🚀 Features

- Disk cache & archive-based file backend for Debian

### 🚜 Refactoring

- Make `konfigkoll_script` independent of `konfigkoll_core`
- Restructure `paketkoll_cache`

### 📚 Documentation

- Development docs & misc updates
- Spell check code comments

### ⚙️ Other stuff

- Format strings using nightly rustfmt
- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up trailing ws
- Reformat Cargo.toml files & imports
- Use RustRover Optimise imports

## [0.1.2] - 2024-07-25

### 🐛 Bug fixes

- Fix CI release build (second try)

## [0.1.1] - 2024-07-25

### 🐛 Bug fixes

- Fix CI release build

## [0.1.0] - 2024-07-25

This is the initial release of the `konfigkoll` crate.

### 🚀 Features

- Improve defaults
- Add Makefile to help install things, vendor deps by default
- Improve logging
- Handle removing unused dependencies
- Rework, only single files backend allowed
- Disk cache for original file queries
- Create konfigkoll

### 🐛 Bug fixes

- Handle some cases of restoring more correctly

### 🚜 Refactoring

- Combine binary crates
- Split up konfigkoll main module

### 📚 Documentation

- Konfigkoll README
- Mdbook documentation

### ⚡ Performance improvements

- Some small performance fixes

### ⚙️ Other stuff

- Clippy fixes
