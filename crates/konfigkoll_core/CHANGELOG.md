# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages (and may or may not be lightly
edited).

For a possibly more edited message focused on the binary please see the github
releases.

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
