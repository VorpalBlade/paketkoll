# paketkoll_core - Core functionality for paketkoll

This will be expanded into doing more in the future for another planned project,
but right now this is only the backend library for paketkoll, and not really
usable standalone.

The API is currently unstable.

What this library is currently:

* A way to check if Arch Linux (pacman) or Debian (apt/dpkg) installed files have been changed.

What this library may one day become:

* Get lists of installed packages (pacman, apt, cargo, flatpak, maybe even snap)
  Other backends (RPM, APK, ...) will be welcome, though not something I have need
  of myself.
* Get information about files installed by those package managers (where available)
  The goal is to be able to check for changes. I might also consider some non-package
  manager backends about "managed files". One example is tmpfiles.d. The goal here is
  to find out about all sorts of files on the system that are prescribed to have a
  specific state.
* Get original unchanged files where possible (downloading the package to the package
  cache if missing and extracting the file in question from there).

## Caveats

This library currently vendors a patched version of [mtree-rs](https://github.com/derekdreery/mtree-rs).
Hopefully the changes required will be merged upstream and a new release made, at
which point the plan is to no longer vendor that dependency.