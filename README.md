# Paketkoll and konfigkoll

This repository is home to two projects:

* Paketkoll:\
  A Rust replacement for `debsums` (on Debian/Ubuntu/...) and `paccheck`
  (on Arch Linux and derivatives). It is much faster than those thanks to using
  all your CPU cores in parallel. (It is also much much faster than `pacman -Qkk`
  which is much slower than `paccheck` even.)\
  \
  Additionally it has some other commands such as finding what package owns a file,
  etc. This program is pretty much done. See
  [the README for paketkoll](crates/paketkoll/README.md) for more information.
* Konfigkoll:\
  A personal system configuration manager. This is for "Oh no, I have too many
  computers and want to sync my configuration files between them using git".
  It differs from ansible and similar (designed for sysadmins). This is [chezmoi]
  for the whole computer. It is heavily inspired by [aconfmgr], but supports more
  than just Arch Linux (specifically Debian and derivatives as well).\
  **This program is very much a work in progress.**\
  See [the README for konfigkoll](crates/konfigkoll/README.md) for more information.

[chezmoi]: https://github.com/twpayne/chezmoi
[aconfmgr]: https://github.com/CyberShadow/aconfmgr
