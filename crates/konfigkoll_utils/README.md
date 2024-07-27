# konfigkoll_utils

This crate exists to improve incremental build times for konfigkoll by
making konfigkoll_script not depend on konfigkoll_core. That is the only
reason this is a separate crate.

You are free to use this, and this follows semver, but it isn't primarily
intended for third party consumption.

## MSRV (Minimum Supported Rust Version) policy

The MSRV may be bumped as needed. It is guaranteed that this library will at
least build on the current stable Rust release. An MSRV change is not considered
a breaking change and as such may change even in a patch version.
