//! Conversion from paketkoll issues into konfigkoll instruction stream

use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    os::unix::fs::{FileTypeExt, MetadataExt},
    sync::atomic::AtomicU32,
};

use anyhow::Context;
use camino::Utf8Path;
use compact_str::format_compact;
use konfigkoll_types::{
    FileContents, FsInstruction, FsOp, PkgIdent, PkgInstruction, PkgInstructions, PkgOp,
};
use paketkoll_types::{
    backend::Backend,
    files::{Checksum, Gid, Mode, Uid},
    intern::{Interner, PackageRef},
    issue::Issue,
    package::{InstallReason, PackageInterned},
};
use paketkoll_utils::{checksum::sha256_readable, MODE_MASK};
use parking_lot::Mutex;
use rayon::prelude::*;

use crate::utils::{IdKey, NumericToNameResolveCache};

pub fn convert_issues_to_fs_instructions(
    issues: Vec<(Option<PackageRef>, Issue)>,
) -> anyhow::Result<Vec<FsInstruction>> {
    let error_count = AtomicU32::new(0);
    let id_resolver = Mutex::new(NumericToNameResolveCache::new());

    let converted: Vec<FsInstruction> = issues.into_par_iter().map(|issue| {
        let mut results = vec![];
        let (_pkg, issue) = issue;
        match convert_issue(&issue, &mut results, &id_resolver) {
            Ok(()) => (),
            Err(err) => {
                tracing::error!(target: "konfigkoll_core::conversion", "Error converting issue: {err:?} for {}", issue.path().display());
                error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        results
    }).flatten().collect();

    tracing::debug!("Conversion done, length: {}", converted.len());
    let error_count = error_count.load(std::sync::atomic::Ordering::Relaxed);
    if error_count > 0 {
        anyhow::bail!("{error_count} errors were encountered while converting, see log");
    }

    Ok(converted)
}

fn convert_issue(
    issue: &Issue,
    results: &mut Vec<FsInstruction>,
    id_resolver: &Mutex<NumericToNameResolveCache>,
) -> Result<(), anyhow::Error> {
    let path: &Utf8Path = issue.path().try_into()?;
    for kind in issue.kinds() {
        match kind {
            paketkoll_types::issue::IssueKind::Missing => results.push(FsInstruction {
                path: path.into(),
                op: FsOp::Remove,
                comment: None,
            }),
            paketkoll_types::issue::IssueKind::Exists
            | paketkoll_types::issue::IssueKind::Unexpected => {
                results.extend(from_fs(path, id_resolver)?);
            }
            paketkoll_types::issue::IssueKind::PermissionDenied => {
                anyhow::bail!("Permission denied on {:?}", issue.path());
            }
            paketkoll_types::issue::IssueKind::TypeIncorrect {
                actual: _,
                expected: _,
            } => {
                results.push(FsInstruction {
                    path: path.into(),
                    op: FsOp::Remove,
                    comment: Some(format_compact!("Removed due to type confict")),
                });
                results.extend(from_fs(path, id_resolver)?);
            }
            paketkoll_types::issue::IssueKind::SizeIncorrect { .. } => {
                results.push(FsInstruction {
                    path: path.into(),
                    op: FsOp::CreateFile(
                        fs_load_contents(path, None)
                            .with_context(|| format!("Failed to read {path:?}"))?,
                    ),
                    comment: None,
                });
            }
            paketkoll_types::issue::IssueKind::ChecksumIncorrect {
                actual,
                expected: _,
            } => {
                results.push(FsInstruction {
                    path: path.into(),
                    op: FsOp::CreateFile(
                        fs_load_contents(path, Some(actual))
                            .with_context(|| format!("Failed to read {path:?}"))?,
                    ),
                    comment: None,
                });
            }
            paketkoll_types::issue::IssueKind::SymlinkTarget {
                actual,
                expected: _,
            } => {
                let actual: &Utf8Path = actual.as_path().try_into()?;
                results.push(FsInstruction {
                    path: path.into(),
                    op: FsOp::CreateSymlink {
                        target: actual.into(),
                    },
                    comment: None,
                });
            }
            paketkoll_types::issue::IssueKind::WrongOwner {
                actual,
                expected: _,
            } => results.push(FsInstruction {
                path: path.into(),
                op: FsOp::SetOwner {
                    owner: id_resolver.lock().lookup(&IdKey::User(*actual))?,
                },
                comment: None,
            }),
            paketkoll_types::issue::IssueKind::WrongGroup {
                actual,
                expected: _,
            } => results.push(FsInstruction {
                path: path.into(),
                op: FsOp::SetGroup {
                    group: id_resolver.lock().lookup(&IdKey::Group(*actual))?,
                },
                comment: None,
            }),
            paketkoll_types::issue::IssueKind::WrongMode {
                actual,
                expected: _,
            } => results.push(FsInstruction {
                path: path.into(),
                op: FsOp::SetMode { mode: *actual },
                comment: None,
            }),
            paketkoll_types::issue::IssueKind::WrongDeviceNodeId {
                actual: (dev_type, major, minor),
                expected: _,
            } => results.push(FsInstruction {
                path: path.into(),
                op: match dev_type {
                    paketkoll_types::files::DeviceType::Block => FsOp::CreateBlockDevice {
                        major: *major,
                        minor: *minor,
                    },
                    paketkoll_types::files::DeviceType::Char => FsOp::CreateCharDevice {
                        major: *major,
                        minor: *minor,
                    },
                },
                comment: None,
            }),
            paketkoll_types::issue::IssueKind::MetadataError(_) => todo!(),
            paketkoll_types::issue::IssueKind::FsCheckError(_) => todo!(),
            _ => todo!(),
        };
    }
    Ok(())
}

/// Create all required instructions for a file on the file system
fn from_fs(
    path: &Utf8Path,
    id_resolver: &Mutex<NumericToNameResolveCache>,
) -> anyhow::Result<impl Iterator<Item = FsInstruction>> {
    let metadata = path
        .symlink_metadata()
        .with_context(|| anyhow::anyhow!("Failed to get metadata"))?;

    let mut results = vec![];

    if metadata.is_file() {
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateFile(
                fs_load_contents(path, None).with_context(|| format!("Failed to load {path}"))?,
            ),
            comment: None,
        });
    } else if metadata.is_dir() {
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateDirectory,
            comment: None,
        });
    } else if metadata.file_type().is_symlink() {
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateSymlink {
                target: std::fs::read_link(path)
                    .with_context(|| anyhow::anyhow!("Failed to read symlink target"))?
                    .try_into()?,
            },
            comment: None,
        });
    } else if metadata.file_type().is_fifo() {
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateFifo,
            comment: None,
        });
    } else if metadata.file_type().is_block_device() {
        let rdev = metadata.rdev();
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateBlockDevice {
                // SAFETY: rdev is a valid device number
                major: unsafe { libc::major(rdev) } as u64,
                // SAFETY: rdev is a valid device number
                minor: unsafe { libc::minor(rdev) } as u64,
            },
            comment: None,
        });
    } else if metadata.file_type().is_char_device() {
        let rdev = metadata.rdev();
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::CreateCharDevice {
                // SAFETY: rdev is a valid device number
                major: unsafe { libc::major(rdev) } as u64,
                // SAFETY: rdev is a valid device number
                minor: unsafe { libc::minor(rdev) } as u64,
            },
            comment: None,
        });
    } else if metadata.file_type().is_socket() {
        // Socket files can only be created by a running program and gets
        // removed on program end. We can't do anything with them.
        tracing::warn!(target: "konfigkoll_core::conversion", "Ignoring socket file: {:?}", path);
        return Ok(results.into_iter());
    } else {
        anyhow::bail!("Unsupported file type: {:?}", path);
    }

    // Set metadata
    if !metadata.is_symlink() {
        results.push(FsInstruction {
            path: path.into(),
            op: FsOp::SetMode {
                mode: Mode::new(metadata.mode() & MODE_MASK),
            },
            comment: None,
        });
    }
    results.push(FsInstruction {
        path: path.into(),
        op: FsOp::SetOwner {
            owner: id_resolver
                .lock()
                .lookup(&IdKey::User(Uid::new(metadata.uid())))?,
        },
        comment: None,
    });
    results.push(FsInstruction {
        path: path.into(),
        op: FsOp::SetGroup {
            group: id_resolver
                .lock()
                .lookup(&IdKey::Group(Gid::new(metadata.gid())))?,
        },
        comment: None,
    });

    Ok(results.into_iter())
}

/// Load real contents from file system
fn fs_load_contents(path: &Utf8Path, checksum: Option<&Checksum>) -> anyhow::Result<FileContents> {
    let mut reader = BufReader::new(File::open(path)?);
    // Always use sha256, recompute if we were given an MD5.
    // This is needed to normalise the checksums for diffing later on.
    let checksum = match checksum {
        Some(c @ Checksum::Sha256(_)) => c.clone(),
        Some(_) | None => sha256_readable(&mut reader)?,
    };
    let size = path.metadata()?.size();
    // I don't like this, but I don't see much of a better option to avoid running out of memory
    if size > 1024 * 1024 {
        Ok(FileContents::FromFile {
            checksum,
            path: path.into(),
        })
    } else {
        reader.rewind()?;
        let mut buf = Vec::with_capacity(size as usize);
        reader.read_to_end(&mut buf)?;
        Ok(FileContents::Literal {
            checksum,
            data: buf.into_boxed_slice(),
        })
    }
}

pub fn convert_packages_to_pkg_instructions(
    packages: impl Iterator<Item = PackageInterned>,
    package_manager: Backend,
    interner: &Interner,
) -> PkgInstructions {
    let mut results = PkgInstructions::default();

    for package in packages {
        // We only consider explicitly installed packages
        if package.reason == Some(InstallReason::Dependency) {
            continue;
        }
        let identifier = if package.ids.is_empty() {
            package.name.to_str(interner).into()
        } else {
            package.ids[0].to_str(interner).into()
        };
        results.insert(
            PkgIdent {
                package_manager,
                identifier,
            },
            PkgInstruction {
                op: PkgOp::Install,
                comment: package.desc.clone(),
            },
        );
    }

    results
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use paketkoll_types::package::PackageInstallStatus;

    use super::*;

    #[test]
    fn test_convert_packages_to_pkg_instructions() {
        let interner = Interner::new();
        let packages = vec![
            PackageInterned {
                name: PackageRef::get_or_intern(&interner, "foo"),
                version: "1.0".into(),
                desc: Some("A package".into()),
                depends: vec![],
                provides: vec![],
                reason: Some(InstallReason::Explicit),
                status: PackageInstallStatus::Installed,
                ids: smallvec::smallvec![],
                architecture: None,
            },
            PackageInterned {
                name: PackageRef::get_or_intern(&interner, "bar"),
                version: "1.0".into(),
                desc: Some("Another package".into()),
                depends: vec![],
                provides: vec![],
                reason: Some(InstallReason::Dependency),
                status: PackageInstallStatus::Installed,
                ids: smallvec::smallvec![],
                architecture: None,
            },
            PackageInterned {
                name: PackageRef::get_or_intern(&interner, "quux"),
                architecture: None,
                version: "2.0".into(),
                desc: Some("Yet another package".into()),
                depends: vec![],
                provides: vec![],
                reason: Some(InstallReason::Explicit),
                status: PackageInstallStatus::Installed,
                ids: smallvec::smallvec![PackageRef::get_or_intern(&interner, "quux/x86-64")],
            },
        ];

        let instructions =
            convert_packages_to_pkg_instructions(packages.into_iter(), Backend::Apt, &interner);

        assert_eq!(instructions.len(), 2);
        assert_eq!(
            instructions.iter().sorted().collect::<Vec<_>>(),
            vec![
                (
                    &PkgIdent {
                        package_manager: Backend::Apt,
                        identifier: "foo".into()
                    },
                    &PkgInstruction {
                        op: PkgOp::Install,
                        comment: Some("A package".into())
                    }
                ),
                (
                    &PkgIdent {
                        package_manager: Backend::Apt,
                        identifier: "quux/x86-64".into()
                    },
                    &PkgInstruction {
                        op: PkgOp::Install,
                        comment: Some("Yet another package".into())
                    }
                )
            ]
        );
    }
}