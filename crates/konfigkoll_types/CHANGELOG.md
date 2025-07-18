# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.2.9] - 2025-07-14

### ⚙️ Other stuff

- Update to Rust 1.88.0

## [0.2.8] - 2025-05-17

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_types, paketkoll_utils

## [0.2.7] - 2025-05-05

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_types, paketkoll_utils

## [0.2.6] - 2025-04-27

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_types, paketkoll_utils

## [0.2.5] - 2025-03-28

### ⚙️ Other stuff

- Migrate to edition 2024
- Bump mimumum required Rust version to 1.85.0

## [0.2.4] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

## [0.2.3] - 2024-09-20

### ⚙️ Other stuff

- Add crates.io package keywords & categories

## [0.2.2] - 2024-09-19

### ⚙️ Other stuff

- Change to some functions to const
- Enable clippy::use_self

## [0.2.1] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.2.0] - 2024-08-17

### 🚀 Features

- Include package name for the modified file (where possible) in a comment when saving

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

## [0.1.3] - 2024-08-03

### 🐛 Bug fixes

- Fix broken sorting in apply

### ⚙️ Other stuff

- Bump MSRV

## [0.1.1] - 2024-07-27

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the initial release of the `konfigkoll_types` crate.
