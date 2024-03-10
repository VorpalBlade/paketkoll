//! Parsers for Debian package files.

use std::io::BufRead;

use anyhow::{bail, Context};
use bstr::{io::BufReadExt, ByteSlice, ByteVec};

use crate::types::{
    Checksum, FileEntry, FileFlags, PackageInterner, PackageRef, Properties, RegularFileBasic,
};

/// Load lines from a readable as PathBufs
pub(super) fn parse_paths(
    package: PackageRef,
    input: &mut impl BufRead,
) -> anyhow::Result<Vec<FileEntry>> {
    let lines: anyhow::Result<Vec<_>> = input
        .byte_lines()
        .map(|e| match e {
            Ok(inner) if inner.as_slice() == b"/." => {
                // Adjust path of root directory
                Ok(FileEntry {
                    package: Some(package),
                    path: "/".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                })
            }
            Ok(inner) => Ok(FileEntry {
                package: Some(package),
                path: inner.into_path_buf().context("Failed to convert")?,
                properties: Properties::Unknown,
                flags: FileFlags::empty(),
                seen: Default::default(),
            }),
            Err(err) => Err(err).context("Failed to parse"),
        })
        .collect();
    lines
}

/// Parse a .md5sums readable
pub(super) fn parse_md5sums(
    package: PackageRef,
    input: &mut impl BufRead,
) -> anyhow::Result<Vec<FileEntry>> {
    let lines: anyhow::Result<Vec<_>> = input
        .byte_lines()
        .map(|e| match e {
            Ok(mut inner) => {
                // MD5s are 16 bytes. Represented as hex this is 32 bytes
                // This is followed by exactly two spaces, followed by the path
                // without leading /.
                let (checksum, path) = inner.split_at_mut(32);
                path[1] = b'/';
                let path = path[1..].to_path()?.to_owned();
                let mut decoded: [u8; 16] = [0; 16];
                faster_hex::hex_decode(checksum.as_bytes(), &mut decoded)?;

                Ok(FileEntry {
                    package: Some(package),
                    path,
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: Checksum::Md5(decoded),
                    }),
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                })
            }
            Err(err) => Err(err).context("Failed to parse"),
        })
        .collect();
    lines
}

/// Parse /var/lib/dpkg/status for config files
pub(super) fn parse_status(
    interner: &PackageInterner,
    input: &mut impl BufRead,
) -> anyhow::Result<Vec<FileEntry>> {
    let mut state = StatusParsingState::Start;

    let mut results = vec![];

    // This file is UTF-8 at least
    for line in input.lines() {
        let line = line?;
        if let Some(stripped) = line.strip_prefix("Package: ") {
            state = StatusParsingState::InPackage(PackageRef(interner.get_or_intern(stripped)));
        } else if line == "Conffiles:" {
            state = match state {
                StatusParsingState::Start => bail!("Conffiles not in a package"),
                StatusParsingState::InPackage(pkg) => StatusParsingState::InConfFiles(pkg),
                StatusParsingState::InConfFiles(_) => {
                    bail!("Multiple Conffiles sections per package")
                }
            }
        } else if let StatusParsingState::InConfFiles(pkg) = state {
            let ctx = || {
                format!(
                    "Error when processing package: {} (line: {line})",
                    interner
                        .try_resolve(&pkg.as_interner_ref())
                        .expect("Package must be interned at this point")
                )
            };
            if line.starts_with(' ') {
                let line_fragments: smallvec::SmallVec<[&str; 4]> = line.split(' ').collect();
                if line_fragments.len() < 2 {
                    return Err(anyhow::anyhow!("Too short line")).with_context(ctx);
                }
                if Some(&"remove-on-upgrade") == line_fragments.last() {
                    // Skip this entry for now
                    continue;
                }
                if Some(&"newconffile") == line_fragments.last() {
                    // Skip this entry for now
                    continue;
                }
                let file = &line_fragments[1];
                let checksum = line_fragments[2];
                let mut decoded: [u8; 16] = [0; 16];
                faster_hex::hex_decode(checksum.as_bytes(), &mut decoded).with_context(ctx)?;
                results.push(FileEntry {
                    package: Some(pkg),
                    path: file.into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: Checksum::Md5(decoded),
                    }),
                    flags: FileFlags::CONFIG,
                    seen: Default::default(),
                })
            } else {
                state = StatusParsingState::InPackage(pkg)
            }
        }
    }

    Ok(results)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusParsingState {
    Start,
    InPackage(PackageRef),
    InConfFiles(PackageRef),
}

#[cfg(test)]
mod tests {
    use super::{parse_md5sums, parse_paths, parse_status};
    use crate::types::{
        Checksum, FileEntry, FileFlags, PackageInterner, PackageRef, Properties, RegularFileBasic,
    };
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_paths() {
        let input = indoc::indoc! {"
            /usr/share/doc/libc6/README
            /usr/share/doc/libc6/changelog.Debian.gz
            /usr/share/doc/libc6/copyright
            /usr/share/doc/libc6/NEWS.gz"};
        let mut input = input.as_bytes();
        let interner = PackageInterner::default();
        let package_ref = PackageRef(interner.get_or_intern("libc6"));
        let result = parse_paths(package_ref, &mut input).unwrap();
        assert_eq!(
            result,
            vec![
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/README".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/changelog.Debian.gz".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/copyright".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/NEWS.gz".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
            ]
        );
    }

    fn hex_to_md5(hex: &[u8]) -> Checksum {
        let mut decoded: [u8; 16] = [0; 16];
        faster_hex::hex_decode(hex, &mut decoded).unwrap();
        Checksum::Md5(decoded)
    }

    #[test]
    fn test_parse_md5sums() {
        let input = indoc::indoc! {"
            1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a  /usr/share/doc/libc6/README
            1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b  /usr/share/doc/libc6/changelog.Debian.gz
            1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c  /usr/share/doc/libc6/copyright
            1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d  /usr/share/doc/libc6/NEWS.gz"};
        let mut input = input.as_bytes();
        let interner = PackageInterner::default();
        let package_ref = PackageRef(interner.get_or_intern("libc6"));
        let result = parse_md5sums(package_ref, &mut input).unwrap();
        assert_eq!(
            result,
            vec![
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/README".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a")
                    }),
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/changelog.Debian.gz".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b")
                    }),
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/copyright".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c")
                    }),
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/NEWS.gz".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d")
                    }),
                    flags: FileFlags::empty(),
                    seen: Default::default(),
                },
            ]
        );
    }

    #[test]
    fn test_parse_status() {
        let input = indoc::indoc! {"
            Package: libc6
            Status: install ok installed
            Priority: optional
            Section: libs
            Installed-Size: 123456
            Maintainer: Some dude <dude@example.com>
            Architecture: arm64
            Multi-Arch: same
            Source: glibc
            Version: 2.36-9+rpt2+deb12u4
            Depends: libgcc, something-else
            Recommends: something (>= 2.0.5~)
            Suggests: glibc-doc, debconf | debconf-2.0
            Breaks: another-package (<< 1.0), yet-another-package (<< 2.0-2~)
            Conffiles:
             /etc/ld.so.conf 1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a
             /etc/ld.so.conf.d/1.conf 1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b
             /etc/ld.so.conf.d/2.conf 1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c
             /etc/ld.so.conf.d/3.conf 1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d
            Description: Very important library
             Some multi-line description
            "};
        let mut input = input.as_bytes();
        let interner = PackageInterner::default();
        let result = parse_status(&interner, &mut input).unwrap();
        assert_eq!(
            result,
            vec![
                FileEntry {
                    package: Some(PackageRef(interner.get_or_intern("libc6"))),
                    path: "/etc/ld.so.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a")
                    }),
                    flags: FileFlags::CONFIG,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef(interner.get_or_intern("libc6"))),
                    path: "/etc/ld.so.conf.d/1.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b")
                    }),
                    flags: FileFlags::CONFIG,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef(interner.get_or_intern("libc6"))),
                    path: "/etc/ld.so.conf.d/2.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c")
                    }),
                    flags: FileFlags::CONFIG,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef(interner.get_or_intern("libc6"))),
                    path: "/etc/ld.so.conf.d/3.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d")
                    }),
                    flags: FileFlags::CONFIG,
                    seen: Default::default(),
                },
            ]
        );
    }
}
