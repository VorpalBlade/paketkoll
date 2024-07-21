# paketkoll_cache

Internal crate for [paketkoll] / [konfigkoll]. You don't want to be here (probably).
That said, this follows semver.

This crate adds disk caching to expensive original file queries in paketkoll.
A dependency of konfigkoll. Not part of paketkoll_core in order to keep build
times and dependencies in check in the development workspace.

## MSRV (Minimum Supported Rust Version) policy

The MSRV may be bumped as needed. It is guaranteed that this library will at
least build on the current stable Rust release. An MSRV change is not considered
a breaking change and as such may change even in a patch version.
