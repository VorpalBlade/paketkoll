# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages.

For a possibly more edited message focused on the binary please see the github
releases.

## [0.4.0] - 2024-06-26

### 🚀 Shiny new things (features)

- Add file backend for systemd-tmpfiles.d to paketkoll
- Add JSON output (implements [#3](https://github.com/VorpalBlade/paketkoll/pull/3))
- Add flatpak package listing backend
- Add package backend for Debian
- Add listing of installed packages

### 🐛 No longer broken (bug fixes)

- Fix broken Debian status parsing
- Fix Debian status parser (not all packages has description)
- Set correct feature flags for flate2

### 🚜 Now more maintainable (refactor)

- Refactor API of core crate

### 📚 Things to read (documentation)

- Add MSRV policy
- Add missing API docs

### ⚡ Go faster! (performance)

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

### ⚡ Go faster! (performance)

- Save 20 ms on Arch Linux by switching to faster hex parsing

### ⚙️ Other stuff

- Fix new warning on nightly

## [0.3.0] - 2024-03-10

### 🚀 Shiny new things (features)

- [**breaking**] Add scanning for unmanaged files

### ⚙️ Other stuff

- Fix nightly clippy lint
- Code cleanup

## [0.2.0] - 2024-02-29

### 🚀 Shiny new things (features)

- Ability to limit which crates to scan

### 📚 Things to read (documentation)

- Add categories & keywords

### ⚙️ Other stuff

- Update dependencies

## [0.1.1] - 2024-02-26

### 🐛 No longer broken (bug fixes)

- Fix nightly warnings
- Temporary allow dead code
- Disable doctest on vendored mtree

### 📚 Things to read (documentation)

- Add links to README

