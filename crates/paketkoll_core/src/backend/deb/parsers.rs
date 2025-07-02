//! Parsers for Debian package files.

use bstr::ByteSlice;
use bstr::ByteVec;
use bstr::io::BufReadExt;
use compact_str::format_compact;
use eyre::WrapErr;
use eyre::bail;
use paketkoll_types::files::Checksum;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::FileFlags;
use paketkoll_types::files::Properties;
use paketkoll_types::files::RegularFileBasic;
use paketkoll_types::intern::ArchitectureRef;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use paketkoll_types::package::Dependency;
use paketkoll_types::package::InstallReason;
use paketkoll_types::package::Package;
use paketkoll_types::package::PackageBuilder;
use paketkoll_types::package::PackageInstallStatus;
use paketkoll_types::package::PackageInterned;
use smallvec::SmallVec;
use std::io::BufRead;

/// Load lines from a readable as `PathBufs`
pub(super) fn parse_paths(
    package: PackageRef,
    input: &mut impl BufRead,
) -> eyre::Result<Vec<FileEntry>> {
    let lines: eyre::Result<Vec<_>> = input
        .byte_lines()
        .map(|e| match e {
            Ok(inner) if inner.as_slice() == b"/." => {
                // Adjust path of root directory
                Ok(FileEntry {
                    package: Some(package),
                    path: "/".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    source: super::NAME,
                    seen: Default::default(),
                })
            }
            Ok(inner) => Ok(FileEntry {
                package: Some(package),
                path: inner.into_path_buf().wrap_err("Failed to convert")?,
                properties: Properties::Unknown,
                flags: FileFlags::empty(),
                source: super::NAME,
                seen: Default::default(),
            }),
            Err(err) => Err(err).wrap_err("Failed to parse"),
        })
        .collect();
    lines
}

/// Parse a .md5sums readable
pub(super) fn parse_md5sums(
    package: PackageRef,
    input: &mut impl BufRead,
) -> eyre::Result<Vec<FileEntry>> {
    let lines: eyre::Result<Vec<_>> = input
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
                        size: None,
                        checksum: Checksum::Md5(decoded),
                    }),
                    flags: FileFlags::empty(),
                    source: super::NAME,
                    seen: Default::default(),
                })
            }
            Err(err) => Err(err).wrap_err("Failed to parse"),
        })
        .collect();
    lines
}

/// Parse depends lines like:
///
/// Depends: libc6 (>= 2.34), libice6 (>= 1:1.0.0), libx11-6, libxaw7 (>=
/// 2:1.0.14), libxcursor1 (>> 1.1.2), libxext6, libxi6, libxmu6 (>= 2:1.1.3),
/// libxmuu1 (>= 2:1.1.3), libxrandr2 (>= 2:1.5.0), libxt6, libxxf86vm1, cpp
fn parse_depends(interner: &Interner, input: &str) -> Vec<Dependency<PackageRef>> {
    let mut result = vec![];
    for segment in input.split(',') {
        let segment = segment.trim_start();
        let disjunctions: SmallVec<[&str; 4]> = segment.split('|').collect();
        if disjunctions.len() > 1 {
            let alternatives = disjunctions
                .into_iter()
                .map(|e| dependency_name(e.trim_start(), interner))
                .collect();
            result.push(Dependency::Disjunction(alternatives));
        } else {
            let package_ref = dependency_name(segment, interner);
            result.push(Dependency::Single(package_ref));
        }
    }

    result
}

fn parse_provides(interner: &Interner, input: &str) -> Vec<PackageRef> {
    let mut result = vec![];
    for segment in input.split(',') {
        let segment = segment.trim_start();
        let package_ref = dependency_name(segment, interner);
        result.push(package_ref);
    }

    result
}

fn dependency_name(segment: &str, interner: &Interner) -> PackageRef {
    // We throw away version info, that is not relevant to this library
    let name = match segment.split_once(' ') {
        Some((name, _)) => name.trim(),
        None => segment.trim(),
    };
    // If the name contains a : it may be for a specific arch, we ignore that
    let name = match name.split_once(':') {
        Some((name, _)) => name,
        None => name,
    };
    PackageRef::get_or_intern(interner, name)
}

/// Parse /var/lib/dpkg/status for config files
pub(super) fn parse_status(
    interner: &Interner,
    input: &mut impl BufRead,
    primary_architecture: ArchitectureRef,
) -> eyre::Result<(Vec<FileEntry>, Vec<PackageInterned>)> {
    let mut state = StatusParsingState::Start;

    let all_architecture = ArchitectureRef::get_or_intern(interner, "all");

    let mut config_files = vec![];
    let mut packages = vec![];

    let mut package_builder: Option<PackageBuilder<PackageRef, ArchitectureRef>> = None;
    let mut depends = vec![];

    // This file is UTF-8 at least
    let mut buffer = String::new();
    while input.read_line(&mut buffer)? > 0 {
        // Ensure that the buffer is cleared on every iteration regardless of where we
        // exit the loop.
        let guard = scopeguard::guard(&mut buffer, |buf| {
            buf.clear();
        });
        let line = guard.trim_end();
        if let Some(stripped) = line.strip_prefix("Package: ") {
            if let Some(mut builder) = package_builder {
                builder.depends(std::mem::take(&mut depends));
                let mut package = builder.build()?;
                fixup_pkg_ids(
                    &mut package,
                    primary_architecture,
                    all_architecture,
                    interner,
                );
                packages.push(package);
            }
            package_builder = Some(PackageInterned::builder());
            // This will be updated later with the correct reason when we parse extended
            // status
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .reason(Some(InstallReason::Explicit));
            let package_name = PackageRef::get_or_intern(interner, stripped);
            state = StatusParsingState::InPackage(package_name);
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .name(package_name);
        } else if let StatusParsingState::InConfFiles(pkg) = state {
            let ctx = || {
                format!(
                    "Error when processing package: {} (line: {line})",
                    pkg.try_as_str(interner)
                        .expect("Package must be interned at this point")
                )
            };
            if line.starts_with(' ') {
                let line_fragments: SmallVec<[&str; 4]> = line.split(' ').collect();
                if line_fragments.len() < 2 {
                    return Err(eyre::eyre!("Too short line")).wrap_err_with(ctx);
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
                faster_hex::hex_decode(checksum.as_bytes(), &mut decoded).wrap_err_with(ctx)?;
                config_files.push(FileEntry {
                    package: Some(pkg),
                    path: file.into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: Checksum::Md5(decoded),
                    }),
                    flags: FileFlags::CONFIG,
                    source: super::NAME,
                    seen: Default::default(),
                });
            } else {
                state = StatusParsingState::InPackage(pkg);
            }
        }
        // Separate if statement, so we process the next line when exiting parsing conf
        // files
        if let Some(stripped) = line.strip_prefix("Version: ") {
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .version(stripped.into());
        } else if let Some(stripped) = line.strip_prefix("Architecture: ") {
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .architecture(Some(ArchitectureRef::get_or_intern(interner, stripped)));
        } else if let Some(stripped) = line.strip_prefix("Description: ") {
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .desc(Some(stripped.into()));
        } else if let Some(stripped) = line.strip_prefix("Pre-Depends: ") {
            depends.extend(parse_depends(interner, stripped));
        } else if let Some(stripped) = line.strip_prefix("Depends: ") {
            depends.extend(parse_depends(interner, stripped));
        } else if let Some(stripped) = line.strip_prefix("Provides: ") {
            package_builder
                .as_mut()
                .expect("Invalid internal state")
                .provides(parse_provides(interner, stripped));
        } else if let Some(stripped) = line.strip_prefix("Status: ") {
            match stripped {
                "install ok installed" => {
                    package_builder
                        .as_mut()
                        .expect("Invalid internal state")
                        .status(PackageInstallStatus::Installed);
                }
                _ => {
                    package_builder
                        .as_mut()
                        .expect("Invalid internal state")
                        .status(PackageInstallStatus::Partial);
                }
            }
        } else if line == "Conffiles:" {
            state = match state {
                StatusParsingState::Start => bail!("Conffiles not in a package"),
                StatusParsingState::InPackage(pkg) => StatusParsingState::InConfFiles(pkg),
                StatusParsingState::InConfFiles(_) => {
                    bail!("Multiple Conffiles sections per package")
                }
            }
        }
    }

    if let Some(mut builder) = package_builder {
        builder.depends(std::mem::take(&mut depends));
        let mut package = builder.build()?;
        fixup_pkg_ids(
            &mut package,
            primary_architecture,
            all_architecture,
            interner,
        );
        packages.push(package);
    }

    Ok((config_files, packages))
}

fn fixup_pkg_ids(
    package: &mut Package<PackageRef, ArchitectureRef>,
    primary_architecture: ArchitectureRef,
    all_architecture: ArchitectureRef,
    interner: &Interner,
) {
    match package.architecture {
        Some(arch) if arch == primary_architecture || arch == all_architecture => {
            let pkg = package.name.as_str(interner);
            let arch = arch.as_str(interner);
            package.ids.push(package.name);
            package.ids.push(PackageRef::get_or_intern(
                interner,
                format_compact!("{pkg}:{arch}"),
            ));
        }
        Some(arch) => {
            let pkg = package.name.as_str(interner);
            let arch = arch.as_str(interner);
            package.ids.push(PackageRef::get_or_intern(
                interner,
                format_compact!("{pkg}:{arch}"),
            ));
        }
        None => {
            tracing::error!(
                "Package {} has no architecture",
                package.name.as_str(interner)
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusParsingState {
    Start,
    InPackage(PackageRef),
    InConfFiles(PackageRef),
}

/// Parse `/var/lib/apt/extended_states`
pub(super) fn parse_extended_status(
    interner: &Interner,
    input: &mut impl BufRead,
) -> eyre::Result<ahash::AHashMap<(PackageRef, ArchitectureRef), Option<InstallReason>>> {
    let mut state = ExtendedStatusParsingState::Start;

    let all_arch = ArchitectureRef::get_or_intern(interner, "all");
    let mut result = ahash::AHashMap::new();

    let mut buffer = String::new();
    while input.read_line(&mut buffer)? > 0 {
        let line = buffer.trim();
        if let Some(stripped) = line.strip_prefix("Package: ") {
            let package = PackageRef::get_or_intern(interner, stripped);
            state = ExtendedStatusParsingState::Package { pkg: package };
        } else if let ExtendedStatusParsingState::Package { pkg } = state {
            if let Some(stripped) = line.strip_prefix("Architecture: ") {
                let arch = ArchitectureRef::get_or_intern(interner, stripped);
                state = ExtendedStatusParsingState::Architecture { pkg, arch };
            }
        } else if let ExtendedStatusParsingState::Architecture { pkg, arch } = state
            && let Some(stripped) = line.strip_prefix("Auto-Installed: ")
        {
            let reason = match stripped {
                "1" => Some(InstallReason::Dependency),
                "0" => Some(InstallReason::Explicit),
                _ => {
                    tracing::warn!("Unknown auto-installed value: {}", stripped);
                    None
                }
            };
            result.insert((pkg, arch), reason);
            // Because this file is screwy it can say the primary architecture instead of
            // all. Wtf Debian?
            result.insert((pkg, all_arch), reason);
            state = ExtendedStatusParsingState::Start;
        }
        buffer.clear();
    }

    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExtendedStatusParsingState {
    Start,
    Package {
        pkg: PackageRef,
    },
    Architecture {
        pkg: PackageRef,
        arch: ArchitectureRef,
    },
}

#[cfg(test)]
mod tests {
    use super::parse_md5sums;
    use super::parse_paths;
    use super::parse_status;
    use paketkoll_types::files::Checksum;
    use paketkoll_types::files::FileEntry;
    use paketkoll_types::files::FileFlags;
    use paketkoll_types::files::Properties;
    use paketkoll_types::files::RegularFileBasic;
    use paketkoll_types::intern::ArchitectureRef;
    use paketkoll_types::intern::Interner;
    use paketkoll_types::intern::PackageRef;
    use paketkoll_types::package::Dependency;
    use paketkoll_types::package::InstallReason;
    use paketkoll_types::package::Package;
    use paketkoll_types::package::PackageInstallStatus;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_paths() {
        let input = indoc::indoc! {"
            /usr/share/doc/libc6/README
            /usr/share/doc/libc6/changelog.Debian.gz
            /usr/share/doc/libc6/copyright
            /usr/share/doc/libc6/NEWS.gz"};
        let mut input = input.as_bytes();
        let interner = Interner::default();
        let package_ref = PackageRef::get_or_intern(&interner, "libc6");
        let result = parse_paths(package_ref, &mut input).unwrap();
        assert_eq!(
            result,
            vec![
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/README".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/changelog.Debian.gz".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/copyright".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/NEWS.gz".into(),
                    properties: Properties::Unknown,
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
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
        let interner = Interner::default();
        let package_ref = PackageRef::get_or_intern(&interner, "libc6");
        let result = parse_md5sums(package_ref, &mut input).unwrap();
        assert_eq!(
            result,
            vec![
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/README".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a")
                    }),
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/changelog.Debian.gz".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b")
                    }),
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/copyright".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c")
                    }),
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(package_ref),
                    path: "/usr/share/doc/libc6/NEWS.gz".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d")
                    }),
                    flags: FileFlags::empty(),
                    source: super::super::NAME,
                    seen: Default::default(),
                },
            ]
        );
    }

    #[test]
    fn test_parse_dependencies() {
        let input = "libc6 (>= 2.34), libice6 (>= 1:1.0.0), libx11-6, libxaw7 (>= 2:1.0.14), \
                     libxcursor1 (>> 1.1.2), libxext6, libxi6, libxmu6 (>= 2:1.1.3), libxmuu1 (>= \
                     2:1.1.3), libxrandr2 (>= 2:1.5.0), libxt6, libxxf86vm1, cpp";
        let interner = Interner::default();
        let result = super::parse_depends(&interner, input);

        assert_eq!(
            result,
            vec![
                Dependency::Single(PackageRef::get_or_intern(&interner, "libc6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libice6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libx11-6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxaw7")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxcursor1")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxext6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxi6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxmu6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxmuu1")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxrandr2")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxt6")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "libxxf86vm1")),
                Dependency::Single(PackageRef::get_or_intern(&interner, "cpp")),
            ]
        );

        let input = "python3-attr, python3-importlib-metadata | python3 (>> 3.8), \
                     python3-importlib-resources | python3 (>> 3.9), python3-pyrsistent, \
                     python3-typing-extensions | python3 (>> 3.8), python3:any";
        let result = super::parse_depends(&interner, input);

        assert_eq!(
            result,
            vec![
                Dependency::Single(PackageRef::get_or_intern(&interner, "python3-attr")),
                Dependency::Disjunction(vec![
                    PackageRef::get_or_intern(&interner, "python3-importlib-metadata"),
                    PackageRef::get_or_intern(&interner, "python3")
                ]),
                Dependency::Disjunction(vec![
                    PackageRef::get_or_intern(&interner, "python3-importlib-resources"),
                    PackageRef::get_or_intern(&interner, "python3")
                ]),
                Dependency::Single(PackageRef::get_or_intern(&interner, "python3-pyrsistent")),
                Dependency::Disjunction(vec![
                    PackageRef::get_or_intern(&interner, "python3-typing-extensions"),
                    PackageRef::get_or_intern(&interner, "python3")
                ]),
                Dependency::Single(PackageRef::get_or_intern(&interner, "python3")),
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
            Pre-Depends: dummy
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
        let interner = Interner::default();
        let primary_arch = ArchitectureRef::get_or_intern(&interner, "arm64");
        let (files, packages) = parse_status(&interner, &mut input, primary_arch).unwrap();
        assert_eq!(
            packages,
            vec![Package {
                name: PackageRef::get_or_intern(&interner, "libc6"),
                architecture: Some(ArchitectureRef::get_or_intern(&interner, "arm64")),
                version: "2.36-9+rpt2+deb12u4".into(),
                desc: Some("Very important library".into()),
                depends: vec![
                    Dependency::Single(PackageRef::get_or_intern(&interner, "libgcc")),
                    Dependency::Single(PackageRef::get_or_intern(&interner, "something-else")),
                    Dependency::Single(PackageRef::get_or_intern(&interner, "dummy")),
                ],
                provides: vec![],
                reason: Some(InstallReason::Explicit),
                status: PackageInstallStatus::Installed,
                ids: smallvec::smallvec![
                    PackageRef::get_or_intern(&interner, "libc6"),
                    PackageRef::get_or_intern(&interner, "libc6:arm64"),
                ],
            }]
        );
        assert_eq!(
            files,
            vec![
                FileEntry {
                    package: Some(PackageRef::get_or_intern(&interner, "libc6")),
                    path: "/etc/ld.so.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9a")
                    }),
                    flags: FileFlags::CONFIG,
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef::get_or_intern(&interner, "libc6")),
                    path: "/etc/ld.so.conf.d/1.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9b")
                    }),
                    flags: FileFlags::CONFIG,
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef::get_or_intern(&interner, "libc6")),
                    path: "/etc/ld.so.conf.d/2.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9c")
                    }),
                    flags: FileFlags::CONFIG,
                    source: super::super::NAME,
                    seen: Default::default(),
                },
                FileEntry {
                    package: Some(PackageRef::get_or_intern(&interner, "libc6")),
                    path: "/etc/ld.so.conf.d/3.conf".into(),
                    properties: Properties::RegularFileBasic(RegularFileBasic {
                        size: None,
                        checksum: hex_to_md5(b"1f7b7e9e7e9e7e9e7e9e7e9e7e9e7e9d")
                    }),
                    flags: FileFlags::CONFIG,
                    source: super::super::NAME,
                    seen: Default::default(),
                },
            ]
        );
    }

    #[test]
    fn test_parse_extended_status() {
        let input = indoc::indoc! {"
            Package: ncal
            Architecture: arm64
            Auto-Installed: 1

            Package: libqrencode4
            Architecture: arm64
            Auto-Installed: 1

            Package: linux-image-6.6.28+rpt-rpi-2712
            Architecture: arm64
            Auto-Installed: 1

            Package: linux-image-6.6.28+rpt-rpi-v8
            Architecture: arm64
            Auto-Installed: 1

            Package: linux-headers-6.6.28+rpt-common-rpi
            Architecture: arm64
            Auto-Installed: 1

            Package: linux-headers-6.6.28+rpt-rpi-v8
            Architecture: arm64
            Auto-Installed: 1
        "};

        let interner = Interner::default();
        let mut input = input.as_bytes();
        let result = super::parse_extended_status(&interner, &mut input).unwrap();

        let expected = ahash::AHashMap::from_iter(vec![
            (
                (
                    PackageRef::get_or_intern(&interner, "ncal"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "ncal"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "libqrencode4"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "libqrencode4"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-image-6.6.28+rpt-rpi-2712"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-image-6.6.28+rpt-rpi-2712"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-image-6.6.28+rpt-rpi-v8"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-image-6.6.28+rpt-rpi-v8"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-headers-6.6.28+rpt-common-rpi"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-headers-6.6.28+rpt-common-rpi"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-headers-6.6.28+rpt-rpi-v8"),
                    ArchitectureRef::get_or_intern(&interner, "arm64"),
                ),
                Some(InstallReason::Dependency),
            ),
            (
                (
                    PackageRef::get_or_intern(&interner, "linux-headers-6.6.28+rpt-rpi-v8"),
                    ArchitectureRef::get_or_intern(&interner, "all"),
                ),
                Some(InstallReason::Dependency),
            ),
        ]);

        assert_eq!(result, expected);
    }
}
