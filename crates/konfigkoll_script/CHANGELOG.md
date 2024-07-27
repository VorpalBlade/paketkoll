# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

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
