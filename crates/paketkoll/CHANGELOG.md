# Changelog

All notable changes to this project will be documented in this file.
Keep in mind that this is only updated when releases are made and the file
is generated automatically from commit messages.

For a possibly more edited message focused on the binary please see the github
releases.

## [0.2.2] - 2024-06-26

### 🚀 Shiny new things (features)

- Add file backend for systemd-tmpfiles.d to paketkoll
- Add JSON output (implements [#3](https://github.com/VorpalBlade/paketkoll/pull/3))
- Add flatpak package listing backend
- Add package backend for Debian
- Add listing of installed packages

### 🚜 Now more maintainable (refactor)

- Refactor API of core crate

### 📚 Things to read (documentation)

- Add MSRV policy

### ⚙️ Other stuff

- *(lints)* Enable additional lints

## [0.2.1] - 2024-03-14

### 🚀 Shiny new things (features)

- Speed up MUSL builds by 4x by switching allocators

### 📚 Things to read (documentation)

- Document new `check-unexpected` sub-command

## [0.2.0] - 2024-03-10

### 🚀 Shiny new things (features)

- Generate man pages for all sub-commands
- [**breaking**] Add scanning for unmanaged files
- Generate man page from command line parser

### 📚 Things to read (documentation)

- Add note about pacman -Qkk

## [0.1.3] - 2024-02-29

### 🚀 Shiny new things (features)

- Ability to limit which crates to scan

### 📚 Things to read (documentation)

- Add categories & keywords

### ⚙️ Other stuff

- Update dependencies

## [0.1.2] - 2024-02-26

### 🚀 Shiny new things (features)

- Report existance of issues with exit code

## [0.1.1] - 2024-02-26

### ⚙️ Other stuff

- Updated the following local packages: paketkoll_core

