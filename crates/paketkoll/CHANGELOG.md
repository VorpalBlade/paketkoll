# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.3.4] - 2024-08-17

### 🚀 Features

- Switch from log to tracing

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🚜 Refactoring

- Make serde non-optional to simplify number of possible configurations

### ⚙️ Other stuff

- Move features to workspace manifest where possible

## [0.3.3] - 2024-08-03

### 🐛 Bug fixes

- Correct Pre-Depends handling on Debian

### ⚙️ Other stuff

- Bump MSRV

## [0.3.2] - 2024-07-29

### 🐛 Bug fixes

- Fix parsing of extended status for Debian

### ⚡ Performance improvements

- Improve mtime time parsing (relevant to Arch Linux)
- Improve mtree parsing performance (relevant to Arch Linux)

## [0.3.1] - 2024-07-27

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Format strings using nightly rustfmt
- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up trailing ws
- Debug UI for inspecting files from downloaded archives
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.3.0] - 2024-07-25

This is a massive release, as `konfigkoll` was introduced as a new command.
While this is not directly part of `paketkoll`, it had knock-on effects on the
way the source code is organized.

### 🚀 Features

- Add Makefile to help install things. This is needed to get man pages and
  shell completion files installed. They are no longer generated as part
  of the build script.
- Vendor deps by default (instead of linking them dynamically)
- Rework, only single files backend allowed
- Add `paketkoll owns` command to quickly find what package owns a file

### 🚜 Refactoring

- Split out `print_packages`
- Move backend traits to `paketkoll_types`
- Unify and format Cargo.toml files
- Revamp public API
- Use method for resolving string interning newtypes

### 📚 Documentation

- Reorganize README files

### ⚙️ Other stuff

- Fix typos and lints from RustRover

## [0.2.3] - 2024-06-28

### ⚙️ Other stuff

- Update Cargo.toml dependencies

## [0.2.2] - 2024-06-26

### 🚀 Features

- Add file backend for systemd-tmpfiles.d to paketkoll
- Add JSON output (implements [#3](https://github.com/VorpalBlade/paketkoll/pull/3))
- Add flatpak package listing backend
- Add package backend for Debian
- Add listing of installed packages

### 🚜 Refactoring

- Refactor API of core crate

### 📚 Documentation

- Add MSRV policy

### ⚙️ Other stuff

- *(lints)* Enable additional lints

## [0.2.1] - 2024-03-14

### 🚀 Features

- Speed up MUSL builds by 4x by switching allocators

### 📚 Documentation

- Document new `check-unexpected` sub-command

## [0.2.0] - 2024-03-10

### 🚀 Features

- Generate man pages for all sub-commands
- [**breaking**] Add scanning for unmanaged files
- Generate man page from command line parser

### 📚 Documentation

- Add note about pacman -Qkk

## [0.1.3] - 2024-02-29

### 🚀 Features

- Ability to limit which crates to scan

### 📚 Documentation

- Add categories & keywords

### ⚙️ Other stuff

- Update dependencies

## [0.1.2] - 2024-02-26

### 🚀 Features

- Report existence of issues with exit code

## [0.1.1] - 2024-02-26

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_core
