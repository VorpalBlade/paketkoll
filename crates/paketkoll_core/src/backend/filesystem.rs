//! Generic code for checking files wrt file system

use crate::config::CommonFileCheckConfiguration;
use crate::config::ConfigFiles;
use eyre::Result;
use eyre::WrapErr;
use paketkoll_types::files::Checksum;
use paketkoll_types::files::DeviceNode;
use paketkoll_types::files::DeviceType;
use paketkoll_types::files::Directory;
use paketkoll_types::files::Fifo;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::FileFlags;
use paketkoll_types::files::Gid;
use paketkoll_types::files::Mode;
use paketkoll_types::files::Permissions;
use paketkoll_types::files::Properties;
use paketkoll_types::files::RegularFile;
use paketkoll_types::files::RegularFileBasic;
use paketkoll_types::files::RegularFileSystemd;
use paketkoll_types::files::Symlink;
use paketkoll_types::files::Uid;
use paketkoll_types::issue::EntryType;
use paketkoll_types::issue::Issue;
use paketkoll_types::issue::IssueKind;
use paketkoll_types::issue::IssueVec;
use paketkoll_utils::MODE_MASK;
use std::fs::File;
use std::io::ErrorKind;
use std::io::Read;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Determine if a given file should be processed
const fn should_process(file: &FileEntry, config: &CommonFileCheckConfiguration) -> bool {
    match (config.config_files, file.flags.contains(FileFlags::CONFIG)) {
        (ConfigFiles::Include, _) | (ConfigFiles::Only, true) | (ConfigFiles::Exclude, false) => {
            true
        }
        (ConfigFiles::Exclude, true) | (ConfigFiles::Only, false) => false,
    }
}

/// Check a single file entry from a package database against the file system
pub(crate) fn check_file(
    file: &FileEntry,
    config: &CommonFileCheckConfiguration,
) -> Result<Option<Issue>> {
    let mut issues = IssueVec::new();
    match std::fs::symlink_metadata(&file.path) {
        Ok(metadata) => match &file.properties {
            Properties::RegularFileBasic(RegularFileBasic { size, checksum }) => {
                if !metadata.is_file() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::RegularFile,
                    });
                }
                if should_process(file, config) {
                    check_contents(
                        &mut issues,
                        config,
                        &file.path,
                        &metadata,
                        None,
                        *size,
                        checksum,
                    )?;
                }
            }
            Properties::RegularFileSystemd(RegularFileSystemd {
                mode,
                owner,
                group,
                size,
                checksum,
                contents: _,
            }) => {
                if !metadata.is_file() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::RegularFile,
                    });
                }
                if should_process(file, config) {
                    check_permissions(&mut issues, &metadata, *owner, *group, *mode);
                    check_contents(
                        &mut issues,
                        config,
                        &file.path,
                        &metadata,
                        None,
                        *size,
                        checksum,
                    )?;
                }
            }
            Properties::RegularFile(RegularFile {
                mode,
                owner,
                group,
                mtime,
                size,
                checksum,
            }) => {
                if !metadata.is_file() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::RegularFile,
                    });
                }
                if should_process(file, config) {
                    check_permissions(&mut issues, &metadata, *owner, *group, *mode);
                    check_contents(
                        &mut issues,
                        config,
                        &file.path,
                        &metadata,
                        Some(mtime),
                        Some(*size),
                        checksum,
                    )?;
                }
            }
            Properties::Symlink(Symlink {
                owner,
                group,
                target,
            }) => {
                check_ownership(&mut issues, &metadata, *owner, *group);
                if !metadata.is_symlink() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::Symlink,
                    });
                } else {
                    match std::fs::read_link(&file.path) {
                        Ok(actual_target) => {
                            if *target != actual_target {
                                issues.push(IssueKind::SymlinkTarget {
                                    actual: actual_target,
                                    expected: target.clone(),
                                });
                            }
                        }
                        Err(err) => Err(err).wrap_err_with(|| {
                            format!("Failed to read link target for {:?}", file.path)
                        })?,
                    }
                }
            }
            Properties::Directory(Directory { mode, owner, group }) => {
                if !metadata.is_dir() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::Directory,
                    });
                }
                check_permissions(&mut issues, &metadata, *owner, *group, *mode);
                // We don't do anything with mtime here currently
            }
            Properties::Fifo(Fifo { mode, owner, group }) => {
                if !metadata.file_type().is_fifo() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::Fifo,
                    });
                }
                check_permissions(&mut issues, &metadata, *owner, *group, *mode);
            }
            Properties::DeviceNode(DeviceNode {
                mode,
                owner,
                group,
                device_type,
                major,
                minor,
            }) => {
                let is_expected_type: bool = match device_type {
                    DeviceType::Block => metadata.file_type().is_block_device(),
                    DeviceType::Char => metadata.file_type().is_char_device(),
                };
                if !is_expected_type {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: match device_type {
                            DeviceType::Block => EntryType::BlockDevice,
                            DeviceType::Char => EntryType::CharDevice,
                        },
                    });
                } else {
                    // Only check major/minor if we have a device node
                    let rdev = metadata.rdev();
                    // SAFETY: As far as I can find out, these do not actually
                    // have any safety invariants, as they just perform some simple bitwise
                    // arithmetics.
                    let major_actual = u64::from(libc::major(rdev));
                    // SAFETY: Same as for major
                    let minor_actual = u64::from(libc::minor(rdev));
                    if (major_actual, minor_actual) != (*major, *minor) {
                        issues.push(IssueKind::WrongDeviceNodeId {
                            actual: (*device_type, major_actual, minor_actual),
                            expected: (*device_type, *major, *minor),
                        });
                    }
                }
                check_permissions(&mut issues, &metadata, *owner, *group, *mode);
            }
            Properties::Special => {
                // Should be something other than dir, symlink or file:
                if metadata.is_dir() || metadata.is_file() || metadata.is_symlink() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::Special,
                    });
                }
            }
            Properties::Removed => {
                // Should not exist
                issues.push(IssueKind::Exists);
            }
            Properties::Unknown => {
                // Should be something other than a file (but Debian doesn't tell us what)
                if metadata.is_file() {
                    issues.push(IssueKind::TypeIncorrect {
                        actual: metadata.file_type().into(),
                        expected: EntryType::Unknown,
                    });
                }
            }
            Properties::Permissions(Permissions { mode, owner, group }) => {
                check_permissions(&mut issues, &metadata, *owner, *group, *mode);
            }
        },
        Err(err) => match err.kind() {
            ErrorKind::NotFound if file.properties == Properties::Removed => (),
            ErrorKind::NotFound if file.flags.contains(FileFlags::OK_IF_MISSING) => (),
            ErrorKind::NotFound => {
                issues.push(IssueKind::Missing);
            }
            ErrorKind::PermissionDenied => {
                issues.push(IssueKind::PermissionDenied);
            }
            _ => Err(err).wrap_err_with(|| format!("IO error while processing {:?}", file.path))?,
        },
    }
    // Finally check if we have anything to report
    if issues.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Issue::new(
            file.path.clone(),
            issues,
            Some(file.source),
        )))
    }
}

/// Check the contents of a regular file against the expected values
fn check_contents(
    issues: &mut IssueVec,
    config: &CommonFileCheckConfiguration,
    path: &PathBuf,
    actual_metadata: &std::fs::Metadata,
    expected_mtime: Option<&std::time::SystemTime>,
    expected_size: Option<u64>,
    expected_checksum: &Checksum,
) -> Result<()> {
    // Fast path with size
    if let Some(size) = expected_size
        && size != actual_metadata.len()
    {
        issues.push(IssueKind::SizeIncorrect {
            actual: actual_metadata.len(),
            expected: size,
        });
        return Ok(());
    }

    // Possibly fast path using mtime
    if config.trust_mtime
        && let Some(mtime) = expected_mtime
        && *mtime == actual_metadata.modified()?
    {
        return Ok(());
    }
    // Otherwise, check checksum
    let mut reader = match File::open(path) {
        Ok(file) => file,
        Err(err) => match err.kind() {
            ErrorKind::PermissionDenied => {
                issues.push(IssueKind::PermissionDenied);
                return Ok(());
            }
            _ => Err(err).wrap_err_with(|| format!("IO error while reading {path:?}"))?,
        },
    };

    let mut buffer = [0; 16 * 1024];

    match *expected_checksum {
        #[cfg(feature = "__md5")]
        Checksum::Md5(ref expected) => {
            use md5::Digest;
            use paketkoll_types::files::Checksum;
            let mut hasher = md5::Md5::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        hasher.update(&buffer[..n]);
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => Err(e)?,
                }
            }
            let mut actual = Default::default();
            hasher.finalize_into(&mut actual);

            if actual[..] != expected[..] {
                issues.push(IssueKind::ChecksumIncorrect {
                    actual: Checksum::Md5(actual[..].try_into().expect("Invalid length")),
                    expected: expected_checksum.clone(),
                });
            }
        }
        #[cfg(feature = "__sha256")]
        Checksum::Sha256(ref expected) => {
            let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        hasher.update(&buffer[..n]);
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => Err(e)?,
                }
            }
            let actual = hasher.finish();

            if actual.as_ref() != expected {
                issues.push(IssueKind::ChecksumIncorrect {
                    actual: Checksum::Sha256(actual.as_ref().try_into().expect("Invalid length")),
                    expected: expected_checksum.clone(),
                });
            }
        }
        _ => {
            tracing::error!("Checksum {expected_checksum} is of an unsupported type");
            issues.push(IssueKind::FsCheckError(Box::new(eyre::eyre!(
                "Unsupported checksum type"
            ))));
        }
    }

    Ok(())
}

/// Check if permissions match
fn check_permissions(
    issues: &mut IssueVec,
    actual_metadata: &std::fs::Metadata,
    expected_owner: Uid,
    expected_group: Gid,
    expected_mode: Mode,
) {
    check_ownership(issues, actual_metadata, expected_owner, expected_group);
    // There are some extra bits further up in the mode mask that we need to mask
    // out here. They indicate things like file/directory/fifo/device-node
    let actual_mode = actual_metadata.mode() & MODE_MASK;
    if actual_mode != expected_mode.as_raw() {
        issues.push(IssueKind::WrongMode {
            actual: Mode::new(actual_mode),
            expected: expected_mode,
        });
    }
}

/// Check if owner/group matches
fn check_ownership(
    issues: &mut IssueVec,
    actual_metadata: &std::fs::Metadata,
    expected_owner: Uid,
    expected_group: Gid,
) {
    if actual_metadata.uid() != expected_owner.as_raw() {
        issues.push(IssueKind::WrongOwner {
            actual: Uid::new(actual_metadata.uid()),
            expected: expected_owner,
        });
    }
    if actual_metadata.gid() != expected_group.as_raw() {
        issues.push(IssueKind::WrongGroup {
            actual: Gid::new(actual_metadata.gid()),
            expected: expected_group,
        });
    }
}
