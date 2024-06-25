# Systemd_tmpfiles parsing library in pure Rust

[ [lib.rs] ] [ [crates.io] ]

A library to parse the [file format] used by [systemd-tmpfiles].

## MSRV (Minimum Supported Rust Version) policy

The MSRV may be bumped as needed. It is guaranteed that this library will at
least build on the current stable Rust release. An MSRV change is not considered
a breaking change and as such may change even in a patch version.

[file format]: https://www.man7.org/linux/man-pages/man5/tmpfiles.d.5.html
[systemd-tmpfiles]: https://www.man7.org/linux/man-pages/man8/systemd-tmpfiles.8.html
[crates.io]: https://crates.io/crates/systemd_tmpfiles
[lib.rs]: https://lib.rs/crates/systemd_tmpfiles
