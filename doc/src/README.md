# Konfigkoll and paketkoll

## Paketkoll

Paketkoll does a bunch of things:

* On Debian:
  * Faster alternative to `debsums`: Checking integrity of installed files with respect to packages.
  * Faster alternative to `dpkg-query -S`: Listing which package owns a given file
* On Arch Linux:
  * Faster alternative to `pacman -Qkk` / `paccheck`: Checking integrity of installed files with respect to packages.
  * Faster alternative to `pacman -Qo`: Listing which package owns files
* Listing installed packages in a platform neutral way (Debian, Arch Linux, and derivatives).\
  Also supports listing flatpak.
* Getting the original file contents for a given path.

## Konfigkoll

Konfigkoll is a work in progress cross platform configuration manager.
