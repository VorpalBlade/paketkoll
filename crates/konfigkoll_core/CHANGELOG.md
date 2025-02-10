# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.5.2] - 2025-02-10

### ⚙️ Other stuff

- Bump mimumum required Rust version to 1.81.0

## [0.5.1] - 2024-12-16

### 🚀 Features

- Prepare workspace hack with cargo-hakari

## [0.5.0] - 2024-09-20

### 🚀 Features

- Improve diff view when restoring to package manager state (fixes [#91](https://github.com/VorpalBlade/paketkoll/pull/91))

### ⚙️ Other stuff

- Add crates.io package keywords & categories

## [0.4.2] - 2024-09-19

### 🐛 Bug fixes

- Fix handling of comment instructions in apply

### 🩺 Diagnostics & output formatting

- Improve error message when failing to delete non-empty directory

### 🧪 Testing

- Add integration tests based on containers

### ⚙️ Other stuff

- Enable clippy::ignored_unit_patterns
- Fix and enable various clippy lints
- Enable clippy::use_self

## [0.4.1] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Switch from anyhow to color-eyre for better (and prettier) error messages
- Limit file data that we store inline leading to less verbose debug logs

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility (for Options)
- Switch to native eyre traits instead of anyhow compatibility
- Use anyhow::Result type alias consistently

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.4.0] - 2024-08-17

### 🚀 Features

- Include package name for the modified file (where possible) in a comment when saving

### 🐛 Bug fixes

- Replacing existing symlinks now works (fixes [#67](https://github.com/VorpalBlade/paketkoll/pull/67))
- Redo archive support to handle cases where an archive is not downloadable

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🩺 Diagnostics & output formatting

- Improve diagnostics for failed file application (now includes failed file name)
- Interactive apply now shows the summary diff unconditionally. Packages can now be skipped.

### 🚜 Refactoring

- Make multi-confirmer strongly typed

### ⚙️ Other stuff

- Move features to workspace manifest where possible
- Apply nightly clippy fixes

## [0.3.1] - 2024-08-03

### 🚀 Features

- Debug tracing for state input and output

### 🐛 Bug fixes

- Apply copying a file should not copy permissions
- Fix broken sorting in apply
- More sensible directions in save when the correct action is to remove an entry

### 🩺 Diagnostics & output formatting

- Warn when attempting to hash big files
- Improved message on no-op change during apply/diff

### 🚜 Refactoring

- Use type aliases properly

### ⚙️ Other stuff

- Bump MSRV
- Debug prints for conversion

## [0.3.0] - 2024-07-29

### 🚀 Features

- Save prefix (for when you wrap cmds in a context object)

### 🐛 Bug fixes

- Fix typo in save

## [0.2.0] - 2024-07-27

### 🚜 Refactoring

- Make `konfigkoll_script` independent of `konfigkoll_core`

### 📚 Documentation

- Spell check code comments

### ⚙️ Other stuff

- Format strings using nightly rustfmt
- Run rustfmt with nightly `imports_granularity = "Item"`
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the initial release of the `konfigkoll_core` crate.

### 🚀 Features

- Stop and print LineEditor action
- Better error reporting on removing non-empty directories
- Improve logging
- Handle removing unused dependencies
- Rework, only single files backend allowed
- Simple line editor (rust + rune API)
- Create konfigkoll

### 🐛 Bug fixes

- When removing we need to start at the innermost path instead of the outermost one
- Fix spurious restore instructions
- Handle some cases of restoring more correctly

### ⚙️ Other stuff

- Get rid of outdated TODO comments
- Clippy fixes
