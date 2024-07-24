# Installation

The preferred method of installing konfigkoll is via your package manager.
For Arch Linux it is available in [AUR](https://aur.archlinux.org/packages/konfigkoll/).

For other systems you will currently have to download the binary from GitHub releases
or build it yourself. The way to build it yourself is from the [git repository],
`cargo install` from crates.io is not recommended (it will work, but you won't get
shell completion nor man pages).

There are three binaries of interest:

* `konfigkoll` - The main binary that will apply and save your configuration.
* `konfigkoll-rune` - This provides LSP language server for the scripting language
  ([Rune]) used in konfigkoll. as well as some generic Rune utilities (such as
  auto-formatting code, though that has limitations currently).
* `paketkoll` - A query tool similar to `debsums`. Parts of it's code is also
  used in konfigkoll, and as such they are maintained in the same git repository.

To build from source:

```bash
git clone https://github.com/VorpalBlade/paketkoll \
    --branch konfigkoll-v0.1.0 # Replace with whatever the current version is
cd paketkoll

# Use one of these:
make install-konfigkoll
make install-paketkoll

# Or use this if you want both
make install
```

You can also select which features to build with, for example to skip the Arch Linux or Debian backends:

```bash
make install CARGO_FLAGS='--no-default-features --features debian,arch_linux,json,vendored'
# CARGO_FLAGS also work with the other install targets of course
```

Remove features from the comma separated list that you don't want. The features are:

* `arch_linux` - Pacman support
* `debian` - Dpkg/Apt support
* `json` - JSON output support (only relevant for paketkoll)
* `vendored` - Use static libraries instead of linking to dynamic libraries on the host.
  This affects compression libraries currently, and not all compression libraries are in use
  for all distros. Currently, this only affects liblzma and libbz2 (both only needed on Debian).

[Rune]: https://rune-rs.github.io/
