# Konfigkoll

[Documentation] [ [lib.rs] ] [ [crates.io] ] [ [AUR] ]

Konfigkoll is a work in progress cross distro configuration manager. It aims to solve the problem
"I have too many computers and want to keep the system configs in sync", rather than
"I am a sysadmin and want to manage a fleet". As such it is a *personal* system configuration manager.

The design of konfigkoll is heavily inspired by the excellent [Aconfmgr](https://github.com/CyberShadow/aconfmgr),
but with a few key differences:

* Aconfmgr is Arch Linux specific, konfigkoll aims to be cross distro
  (currently Arch Linux + work in progress support for Debian & derivatives).
* Aconfmgr is written in Bash, and is rather slow. Konfigkoll is written in Rust, and is much faster.\
  As an example, applying my personal config with aconfmgr on my system takes about 30 seconds, while konfigkoll
  takes about 2 seconds for the equivalent config. (This is assuming `--trust-mtime`, both are
  significantly slowed down if checksums are verified for every file).
* Aconfmgr uses bash as the configuration language, konfigkoll uses [Rune].

Please see [the documentation](https://vorpalblade.github.io/paketkoll/book#konfigkoll) for more information.

## MSRV (Minimum Supported Rust Version) policy

The MSRV may be bumped as needed. It is guaranteed that this program will at
least build on the current stable Rust release. An MSRV change is not considered
a breaking change and as such may change even in a patch version.

## What does the name mean?

konfigkoll is a Swedish for "config check/tracking", though
the translation to English isn't perfect ("ha koll på" means "keep an eye on"
for example). Some nuance is lost in the translation!

[Documentation]: https://vorpalblade.github.io/paketkoll/book
[crates.io]: https://crates.io/crates/konfigkoll
[lib.rs]: https://lib.rs/crates/konfigkoll
[AUR]: https://aur.archlinux.org/packages/konfigkoll
