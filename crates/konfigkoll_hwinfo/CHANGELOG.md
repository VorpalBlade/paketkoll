# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.1.5] - 2024-09-19

### ⚙️ Other stuff

- Enable clippy::ignored_unit_patterns
- Fix and enable various clippy lints

## [0.1.4] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.1.3] - 2024-08-17

### ⚙️ Other stuff

- Move features to workspace manifest where possible

## [0.1.2] - 2024-08-03

### ⚙️ Other stuff

- Bump MSRV

## [0.1.1] - 2024-07-27

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the initial release of the `konfigkoll_hwinfo` crate.
