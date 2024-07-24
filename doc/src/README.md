# Konfigkoll and paketkoll

This repository contains two tools, described below.

## Paketkoll

Paketkoll does a bunch of things:

* On Debian:
  * Faster alternative to `debsums`: Checking integrity of installed files with respect to packages.
  * Faster alternative to `dpkg-query -S`: Listing which package owns a given file
* On Arch Linux:
  * Faster alternative to `pacman -Qkk` / `paccheck`: Checking integrity of installed files with respect to packages.
  * Faster alternative to `pacman -Qo`: Listing which package owns files
* Listing installed packages in a Linux distro neutral way (Debian, Arch Linux, and derivatives).\
  Also supports listing flatpak.
* Getting the original file contents for a given path.

## Konfigkoll

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

### Comparisons

Unlike tools such as ansible, puppet, etc:

* Konfigkoll only manages the computer it is running on, not remote systems over the network.
* Konfigkoll can save the system state to a file, giving you a full template config to work from.
  (You definitely want to customise this saved config though.)

There is perhaps more similarity with NixOS and Guix, but, unlike those:

* You can still use normal management tools and save changes to the config afterwards.\
  With NixOS/Guix every change starts at the config.
* NixOS provides specific config keys for every package, konfigkoll is more general:
  You can patch any config file with sed-like instructions (or custom code), there is
  no special support for specific packages. (There is special support for enabling systemd
  services and working with systemd-sysusers though, since those are such common operations.)

[Rune]: https://rune-rs.github.io/
