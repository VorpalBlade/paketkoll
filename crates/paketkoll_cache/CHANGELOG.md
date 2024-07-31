# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

## [0.2.2] - 2024-07-31

### 🚜 Refactoring

- Use type aliases properly

## [0.2.1] - 2024-07-29

### 🚀 Features

- Try systemd lookup with /lib if /usr/lib fails, to support Debian

## [0.2.0] - 2024-07-27

### 🚀 Features

- Disk cache & archive-based files for Debian
- Cache for getting files from downloaded archives
- Get files from downloaded archives

### 🚜 Refactoring

- Restructure `paketkoll_cache`

### ⚡ Performance improvements

- Enable refresh on disk cache

### ⚙️ Other stuff

- Run rustfmt with nightly `imports_granularity = "Item"`
- Reformat Cargo.toml files & imports
- Clean up unneeded paths for imported items
- Use RustRover Optimise imports

## [0.1.0] - 2024-07-25

This is the first release of the `paketkoll_cache` crate.
