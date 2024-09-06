# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.1.7] - 2024-09-06

### 🩺 Diagnostics & output formatting

- Change span log levels
- Switch from anyhow to color-eyre for better (and prettier) error messages
- Improve formatting of Rune runtime errors
- Add spans to async functions exposed to Rune (should help get better errors with color-eyre)
- Switch to custom error wrapper to better traverse the Rune callstacks
- Limit file data that we store inline leading to less verbose debug logs

### 🚜 Refactoring

- Switch to native eyre traits instead of anyhow compatibility (for Options)
- Switch to native eyre traits instead of anyhow compatibility
- Use anyhow::Result type alias consistently

### ⚙️ Other stuff

- Apply auto fixable clippy lints
- Use nightly import grouping in rustfmt

## [0.1.6] - 2024-08-17

### 🚀 Features

- Include package name for the modified file (where possible) in a comment when saving

### 🐛 Bug fixes

- Redo archive support to handle cases where an archive is not downloadable
- Replacing existing symlinks now works (fixes [#67](https://github.com/VorpalBlade/paketkoll/pull/67))

### ⚡ Performance improvements

- Remove unused dependencies (speeds up build time slightly)

### 🩺 Diagnostics & output formatting

- Improve diagnostics for failed file application (now includes failed file name)
- Interactive apply now shows the summary diff unconditionally. Packages can now be skipped.

### 🚜 Refactoring

- Clean up musl code
- Rewrite the way tracing_subscriber is being used
- Make serde non-optional to simplify number of possible configurations
- Make multi-confirmer strongly typed

### ⚙️ Other stuff

- Move features to workspace manifest where possible
- Remove empty feature table
- Apply nightly clippy fixes

## [0.1.5] - 2024-08-03

### 🚀 Features

- Early/sensitive configurations can now be globs (useful for Debian, where you want `/etc/apt/sources.list.d/*` to be early)
- Filter for save to only save a few of the files (useful when incrementally building up the configuration)
- Error check path in commands for common mistakes
- Align parameter order between groups and users
- Add ability to set path to `nologin` (useful for distros that haven't yet merged `bin` and `sbin`)
- Debug tracing for state input and output

### 🐛 Bug fixes

- Fix broken sorting in apply
- Fix duplicated file entries due to canonicalization happening too late
- Systemd paths are now acquired by running `systemd-paths` on first access
- `gshadow-` and `shadow-` should also be sensitive by default
- When `apply` copies a file it no longer copies permissions
- Provide more sensible directions in save when the correct action is to remove an entry from your configuration

### ⚡ Performance improvements

- Don't drop data just before exiting, let the OS do that.

### 🩺 Diagnostics & output formatting

- Warn when attempting to hash big files
- Improved message on no-op change during apply/diff
- Improve save message to describe what is happening

### ⚙️ Other stuff

- Bump mimumum required Rust version to 1.80.0
- Improve template

## [0.1.4] - 2024-07-29

### 🚀 Features

- Save prefix (for when you wrap cmds in a context object)
- Try systemd lookup with /lib if /usr/lib fails, to support Debian

### 🐛 Bug fixes

- Fix race condition on package manager
- Fix parsing of extended status for Debian
- Fix typo in save output

### ⚡ Performance improvements

- Improve mtime time parsing (relevant to Arch Linux)
- Improve mtree parsing performance (relevant to Arch Linux)

### ⚙️ Other stuff

- Better error messages
- Make disabled package manager quieter and adjust other log levels

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
