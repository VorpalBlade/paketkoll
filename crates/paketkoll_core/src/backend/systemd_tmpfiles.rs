//! (File only) backend for systemd-tmpfiles

use std::{
    borrow::Cow,
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::Context;
use compact_str::CompactString;
use systemd_tmpfiles::specifier::Resolve;

use crate::{
    types::{
        Checksum, DeviceNode, DeviceType, Directory, Fifo, FileFlags, Gid, Mode, Permissions,
        Properties, RegularFileBasic, RegularFileSystemd, Symlink, Uid,
    },
    utils::{sha256_buffer, sha256_readable, MODE_MASK},
};

use super::{Files, Name};

const NAME: &str = "systemd_tmpfiles";

/// Systemd-tmpfiles backend
#[derive(Debug)]
pub(crate) struct SystemdTmpfiles {}

#[derive(Debug, Default)]
pub(crate) struct SystemdTmpfilesBuilder {}

impl SystemdTmpfilesBuilder {
    pub fn build(self) -> SystemdTmpfiles {
        SystemdTmpfiles {}
    }
}

impl Name for SystemdTmpfiles {
    fn name(&self) -> &'static str {
        NAME
    }
}

impl Files for SystemdTmpfiles {
    fn files(
        &self,
        _interner: &crate::types::Interner,
    ) -> anyhow::Result<Vec<crate::types::FileEntry>> {
        let cmd = std::process::Command::new("systemd-tmpfiles")
            .arg("--cat-config")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn \"systemd-tmpfiles --cat-config\" (is systemd-tmpfiles installed and in PATH?)")?;
        let output = cmd
            .wait_with_output()
            .context("Failed to wait for systemd-tmpfiles --cat-config")?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to run systemd-tmpfiles --cat-config: {}",
                String::from_utf8(output.stderr).context("Failed to parse stderr")?
            );
        }
        let output = String::from_utf8(output.stdout)
            .context("Failed to parse systemd-tmpfiles --cat-config as UTF-8")?;

        parse_systemd_tmpfiles_output(&output)
    }
}

fn parse_systemd_tmpfiles_output(
    output: &str,
) -> Result<Vec<crate::types::FileEntry>, anyhow::Error> {
    let parsed = systemd_tmpfiles::parser::parse_str(output)
        .context("Failed to parse systemd-tmpfiles output")?;

    let mut files = HashMap::new();

    let resolver = systemd_tmpfiles::specifier::SystemResolver::new_from_running_system()
        .context("Failed to create systemd-tmpfiles specifier resolver")?;

    for entry in parsed.into_iter() {
        process_entry(&entry, &mut files, &resolver)
            .with_context(|| format!("Failed to process entry for {}", entry.path()))?;
    }

    Ok(files.into_values().collect())
}

fn process_entry(
    entry: &systemd_tmpfiles::Entry,
    files: &mut HashMap<PathBuf, crate::types::FileEntry>,
    resolver: &systemd_tmpfiles::specifier::SystemResolver,
) -> anyhow::Result<()> {
    // Figure out path
    if entry.path_is_glob() {
        log::warn!(
            "Path {} is a glob, skipping as we can't handle that",
            entry.path()
        );
        return Ok(());
    }
    let path: CompactString = resolver
        .resolve(entry.path())
        .context("Failed to resolve path")?
        .into_owned()
        .into();

    // Process flags
    let flags = if entry.flags().intersects(
        systemd_tmpfiles::EntryFlags::ARG_CREDENTIAL
            | systemd_tmpfiles::EntryFlags::ERRORS_OK_ON_CREATE
            | systemd_tmpfiles::EntryFlags::BOOT_ONLY,
    ) {
        FileFlags::OK_IF_MISSING
    } else {
        FileFlags::empty()
    };

    // Process entry types
    let props = match entry.directive() {
        systemd_tmpfiles::Directive::CreateFile {
            truncate_if_exists: _,
            mode,
            user,
            group,
            contents,
        } => {
            let contents = match contents {
                Some(c) => resolver
                    .resolve(std::str::from_utf8(c).context("Non-UTF8 data")?)
                    .context("Failed to apply specifiers")?,
                None => Cow::Borrowed(""),
            };
            Properties::RegularFileSystemd(RegularFileSystemd {
                mode: mode
                    .as_ref()
                    .map(|m| Mode::new(m.mode()))
                    .unwrap_or(Mode::new(0o644)),
                owner: resolve_uid(user)?,
                group: resolve_gid(group)?,
                size: contents.len() as u64,
                checksum: sha256_buffer(contents.as_bytes())
                    .with_context(|| format!("Failed to generate checksum for {path:?}"))?,
            })
        }
        systemd_tmpfiles::Directive::WriteToFile {
            append: false,
            contents,
        } => {
            let contents = resolver
                .resolve(std::str::from_utf8(contents).context("Non-UTF8 data")?)
                .context("Failed to apply specifiers")?;
            Properties::RegularFileBasic(RegularFileBasic {
                size: Some(contents.len() as u64),
                checksum: sha256_buffer(contents.as_bytes())
                    .with_context(|| format!("Failed to generate checksum for {path:?}"))?,
            })
        }
        systemd_tmpfiles::Directive::WriteToFile {
            append: true,
            contents: _,
        } => {
            log::error!("w+ (append to file) is not currently supported, skipping entry");
            return Ok(());
        }
        systemd_tmpfiles::Directive::CreateDirectory {
            remove_if_exists: _,
            mode,
            user,
            group,
            cleanup_age: _,
        }
        | systemd_tmpfiles::Directive::CreateSubvolume {
            quota: _,
            mode,
            user,
            group,
            cleanup_age: _,
        } => Properties::Directory(Directory {
            mode: mode
                .as_ref()
                .map(|m| Mode::new(m.mode()))
                .unwrap_or(Mode::new(0o644)),
            owner: resolve_uid(user)?,
            group: resolve_gid(group)?,
        }),
        systemd_tmpfiles::Directive::CreateFifo {
            replace_if_exists: _,
            mode,
            user,
            group,
        } => Properties::Fifo(Fifo {
            mode: mode
                .as_ref()
                .map(|m| Mode::new(m.mode()))
                .unwrap_or(Mode::new(0o644)),
            owner: resolve_uid(user)?,
            group: resolve_gid(group)?,
        }),
        systemd_tmpfiles::Directive::CreateSymlink {
            replace_if_exists: _,
            target,
        } => {
            let target = match target {
                Some(c) => resolver
                    .resolve(std::str::from_utf8(c).context("Non-UTF8 data")?)
                    .context("Failed to apply specifiers")?,
                None => Cow::Owned(format!("/usr/share/factory/{}", path)),
            };
            Properties::Symlink(Symlink {
                owner: Uid::new(0),
                group: Gid::new(0),
                target: target.into_owned().into(),
            })
        }
        systemd_tmpfiles::Directive::CreateCharDeviceNode {
            replace_if_exists: _,
            mode,
            user,
            group,
            device_specifier,
        } => Properties::DeviceNode(DeviceNode {
            mode: mode
                .as_ref()
                .map(|m| Mode::new(m.mode()))
                .unwrap_or(Mode::new(0o644)),
            owner: resolve_uid(user)?,
            group: resolve_gid(group)?,
            device_type: DeviceType::Char,
            major: device_specifier.major,
            minor: device_specifier.minor,
        }),
        systemd_tmpfiles::Directive::CreateBlockDeviceNode {
            replace_if_exists: _,
            mode,
            user,
            group,
            device_specifier,
        } => Properties::DeviceNode(DeviceNode {
            mode: mode
                .as_ref()
                .map(|m| Mode::new(m.mode()))
                .unwrap_or(Mode::new(0o644)),
            owner: resolve_uid(user)?,
            group: resolve_gid(group)?,
            device_type: DeviceType::Block,
            major: device_specifier.major,
            minor: device_specifier.minor,
        }),
        systemd_tmpfiles::Directive::RecursiveCopy {
            recursive_if_exists: _,
            cleanup_age: _,
            source,
        } => {
            let source = match source {
                Some(c) => resolver.resolve(c).context("Failed to apply specifiers")?,
                None => Cow::Owned(format!("/usr/share/factory/{}", path)),
            };
            // Now we need to figure out if the source is a directory or a file
            recursive_copy(files, Path::new(&source.as_ref()), path.as_str(), flags)?;
            return Ok(());
        }
        systemd_tmpfiles::Directive::IgnorePathDuringCleaning { .. } => return Ok(()),
        systemd_tmpfiles::Directive::IgnoreDirectoryDuringCleaning { .. } => return Ok(()),
        systemd_tmpfiles::Directive::RemoveFile { recursive: _ } => Properties::Removed,
        systemd_tmpfiles::Directive::AdjustPermissionsAndTmpFiles {
            mode,
            user,
            group,
            cleanup_age: _,
        } => Properties::Permissions(Permissions {
            mode: mode
                .as_ref()
                .map(|m| Mode::new(m.mode()))
                .unwrap_or(Mode::new(0o644)),
            owner: resolve_uid(user)?,
            group: resolve_gid(group)?,
        }),
        systemd_tmpfiles::Directive::AdjustAccess {
            recursive,
            mode,
            user,
            group,
        } => {
            if *recursive {
                log::warn!("Recursive Z not properly supported for {path}");
            }
            Properties::Permissions(Permissions {
                mode: mode
                    .as_ref()
                    .map(|m| Mode::new(m.mode()))
                    .unwrap_or(Mode::new(0o644)),
                owner: resolve_uid(user)?,
                group: resolve_gid(group)?,
            })
        }
        systemd_tmpfiles::Directive::SetExtendedAttributes { .. } => return Ok(()),
        systemd_tmpfiles::Directive::SetAttributes { .. } => return Ok(()),
        systemd_tmpfiles::Directive::SetAcl { .. } => return Ok(()),
        _ => todo!(),
    };

    files.insert(
        PathBuf::from(path.as_str()),
        crate::types::FileEntry {
            package: None,
            path: PathBuf::from(path.as_str()),
            properties: props,
            flags,
            source: NAME,
            seen: Default::default(),
        },
    );
    Ok(())
}

fn recursive_copy(
    files: &mut HashMap<PathBuf, crate::types::FileEntry>,
    source_path: &Path,
    target_path: &str,
    flags: FileFlags,
) -> anyhow::Result<()> {
    // Get source metadata
    let source_metadata = match source_path.metadata() {
        Ok(metadata) => metadata,
        Err(e) => {
            log::warn!("Failed to read metadata for {source_path:?}: {e}",);
            return Ok(());
        }
    };

    let props = match source_metadata.is_dir() {
        true => {
            // Try to read directory (but log and skip if not possible)
            let dir_iter = match std::fs::read_dir(source_path) {
                Ok(iter) => iter,
                Err(e) => {
                    log::warn!("Failed to read directory {source_path:?}: {e}",);
                    return Ok(());
                }
            };
            // Recurse though directory contents
            for entry in dir_iter {
                let entry =
                    entry.with_context(|| format!("Failed to read directory {source_path:?}"))?;
                let entry_path = entry.path();
                let entry_name = entry.file_name();
                let entry_name = entry_name.to_string_lossy();
                let entry_target = format!("{target_path}/{entry_name}");
                recursive_copy(files, &entry_path, &entry_target, flags)?;
            }

            // Insert directory itself
            Properties::Directory(Directory {
                mode: Mode::new(source_metadata.mode() & MODE_MASK),
                owner: Uid::new(source_metadata.uid()),
                group: Gid::new(source_metadata.gid()),
            })
        }
        false => {
            let checksum = generate_checksum_from_file(source_path);
            let Ok(checksum) = checksum else {
                log::warn!(
                    "Failed to generate checksum for {source_path:?}: {}",
                    checksum.expect_err("Must be an error in this control flow")
                );
                return Ok(());
            };
            Properties::RegularFileSystemd(RegularFileSystemd {
                mode: Mode::new(source_metadata.mode() & MODE_MASK),
                owner: Uid::new(source_metadata.uid()),
                group: Gid::new(source_metadata.gid()),
                size: source_metadata.len(),
                checksum,
            })
        }
    };

    files.insert(
        PathBuf::from(target_path),
        crate::types::FileEntry {
            package: None,
            path: PathBuf::from(target_path),
            properties: props,
            flags,
            source: NAME,
            seen: Default::default(),
        },
    );
    Ok(())
}

fn generate_checksum_from_file(path: &Path) -> anyhow::Result<Checksum> {
    let mut reader =
        std::fs::File::open(path).with_context(|| format!("IO error while reading {:?}", path))?;
    sha256_readable(&mut reader)
}

fn resolve_gid(group: &systemd_tmpfiles::Id) -> anyhow::Result<Gid> {
    Ok(match group {
        systemd_tmpfiles::Id::Caller { new_only: _ } => Gid::new(0),
        systemd_tmpfiles::Id::Id { id, new_only: _ } => Gid::new(*id),
        systemd_tmpfiles::Id::Name { name, new_only: _ } => {
            let entry = nix::unistd::Group::from_name(name)
                .with_context(|| format!("Failed to resolve GID for {name}"))?
                .with_context(|| format!("Failed to resolve GID for {name}"))?;
            Gid::new(entry.gid.as_raw())
        }
        _ => todo!(),
    })
}

fn resolve_uid(user: &systemd_tmpfiles::Id) -> anyhow::Result<Uid> {
    Ok(match user {
        systemd_tmpfiles::Id::Caller { new_only: _ } => Uid::new(0),
        systemd_tmpfiles::Id::Id { id, new_only: _ } => Uid::new(*id),
        systemd_tmpfiles::Id::Name { name, new_only: _ } => {
            let entry = nix::unistd::User::from_name(name)
                .with_context(|| format!("Failed to resolve UID for {name}"))?
                .with_context(|| format!("Failed to resolve UID for {name}"))?;
            Uid::new(entry.uid.as_raw())
        }
        _ => todo!(),
    })
}
