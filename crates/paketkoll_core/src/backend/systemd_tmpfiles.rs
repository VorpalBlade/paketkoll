//! (File only) backend for systemd-tmpfiles

use ahash::AHashMap;
use compact_str::CompactString;
use eyre::WrapErr;
use paketkoll_types::backend::ArchiveResult;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::Name;
use paketkoll_types::backend::OriginalFileError;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::OriginalFilesResult;
use paketkoll_types::backend::PackageManagerError;
use paketkoll_types::backend::PackageMap;
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
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use paketkoll_utils::MODE_MASK;
use paketkoll_utils::checksum::sha256_buffer;
use paketkoll_utils::checksum::sha256_readable;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use systemd_tmpfiles::specifier::Resolve;

const NAME: &str = "systemd_tmpfiles";

/// Systemd-tmpfiles backend
#[derive(Debug)]
pub(crate) struct SystemdTmpfiles {}

#[derive(Debug, Default)]
pub(crate) struct SystemdTmpfilesBuilder {}

impl SystemdTmpfilesBuilder {
    pub const fn build(self) -> SystemdTmpfiles {
        SystemdTmpfiles {}
    }
}

impl Name for SystemdTmpfiles {
    fn name(&self) -> &'static str {
        NAME
    }

    fn as_backend_enum(&self) -> paketkoll_types::backend::Backend {
        paketkoll_types::backend::Backend::SystemdTmpfiles
    }
}

impl Files for SystemdTmpfiles {
    fn files(&self, _interner: &Interner) -> eyre::Result<Vec<FileEntry>> {
        // Get the entire config from sytemd-tmpfiles
        let cmd = std::process::Command::new("systemd-tmpfiles")
            .arg("--cat-config")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(
                "Failed to spawn \"systemd-tmpfiles --cat-config\" (is systemd-tmpfiles installed \
                 and in PATH?)",
            )?;
        let output = cmd
            .wait_with_output()
            .wrap_err("Failed to wait for systemd-tmpfiles --cat-config")?;
        if !output.status.success() {
            eyre::bail!(
                "Failed to run systemd-tmpfiles --cat-config: {}",
                String::from_utf8(output.stderr).wrap_err("Failed to parse stderr")?
            );
        }
        let output = String::from_utf8(output.stdout)
            .wrap_err("Failed to parse systemd-tmpfiles --cat-config as UTF-8")?;

        // Now parse it
        parse_systemd_tmpfiles_output(&output)
    }

    fn owning_packages(
        &self,
        _paths: &ahash::AHashSet<&Path>,
        _interner: &Interner,
    ) -> eyre::Result<dashmap::DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>> {
        // This doesn't make sense for this provider
        eyre::bail!("Owning packages are not supported for systemd-tmpfiles")
    }

    fn original_files(
        &self,
        _queries: &[OriginalFileQuery],
        _packages: &PackageMap,
        _interner: &Interner,
    ) -> Result<OriginalFilesResult, OriginalFileError> {
        Err(eyre::eyre!(
            "Original file queries are not supported for systemd-tmpfiles"
        ))?
    }

    fn files_from_archives(
        &self,
        _filter: &[PackageRef],
        _package_map: &PackageMap,
        _interner: &Interner,
    ) -> Result<Vec<ArchiveResult>, PackageManagerError> {
        Err(PackageManagerError::UnsupportedOperation(
            "Operation not supported for systemd-tmpfiles",
        ))
    }
}

/// Parse the systemd-tmpfiles output into [`FileEntry`]s that are usable by the
/// shared later stages.
fn parse_systemd_tmpfiles_output(output: &str) -> Result<Vec<FileEntry>, eyre::Error> {
    let parsed = systemd_tmpfiles::parser::parse_str(output)
        .wrap_err("Failed to parse systemd-tmpfiles output")?;

    let mut files = AHashMap::new();

    let mut id_cache = IdCache::default();

    let resolver = systemd_tmpfiles::specifier::SystemResolver::new_from_running_system()
        .wrap_err("Failed to create systemd-tmpfiles specifier resolver")?;

    // Note! It may be tempting to parallelise this, but unfortunately it is "last
    // item wins" (at least per file), including possibly modifying previous
    // entries.
    for entry in &parsed {
        process_entry(entry, &mut files, &mut id_cache, &resolver)
            .wrap_err_with(|| format!("Failed to process entry for {}", entry.path()))?;
    }

    Ok(files.into_values().collect())
}

/// Process a single entry from the systemd-tmpfiles output, converting it to a
/// [`FileEntry`].
fn process_entry<'entry>(
    entry: &'entry systemd_tmpfiles::Entry,
    files: &mut AHashMap<PathBuf, FileEntry>,
    id_cache: &mut IdCache<'entry>,
    resolver: &systemd_tmpfiles::specifier::SystemResolver,
) -> eyre::Result<()> {
    // Figure out path
    if entry.path_is_glob() {
        tracing::warn!(
            "Path {} is a glob, skipping as we can't handle that",
            entry.path()
        );
        return Ok(());
    }
    let path: CompactString = resolver
        .resolve(entry.path())
        .wrap_err("Failed to resolve path")?
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
                    .resolve(std::str::from_utf8(c).wrap_err("Non-UTF8 data")?)
                    .wrap_err("Failed to apply specifiers")?,
                None => Cow::Borrowed(""),
            };
            Properties::RegularFileSystemd(RegularFileSystemd {
                mode: mode
                    .as_ref()
                    .map(|m| Mode::new(m.mode()))
                    .unwrap_or(Mode::new(0o644)),
                owner: resolve_uid(user, id_cache)?,
                group: resolve_gid(group, id_cache)?,
                size: Some(contents.len() as u64),
                checksum: sha256_buffer(contents.as_bytes()),
                contents: Some(contents.into_owned().into_bytes().into_boxed_slice()),
            })
        }
        systemd_tmpfiles::Directive::WriteToFile {
            append: false,
            contents,
        } => {
            let contents = resolver
                .resolve(std::str::from_utf8(contents).wrap_err("Non-UTF8 data")?)
                .wrap_err("Failed to apply specifiers")?;
            Properties::RegularFileBasic(RegularFileBasic {
                size: Some(contents.len() as u64),
                checksum: sha256_buffer(contents.as_bytes()),
            })
        }
        systemd_tmpfiles::Directive::WriteToFile {
            append: true,
            contents: _,
        } => {
            tracing::error!("w+ (append to file) is not currently supported, skipping entry");
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
            owner: resolve_uid(user, id_cache)?,
            group: resolve_gid(group, id_cache)?,
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
            owner: resolve_uid(user, id_cache)?,
            group: resolve_gid(group, id_cache)?,
        }),
        systemd_tmpfiles::Directive::CreateSymlink {
            replace_if_exists: _,
            target,
        } => {
            let target = match target {
                Some(c) => resolver
                    .resolve(std::str::from_utf8(c).wrap_err("Non-UTF8 data")?)
                    .wrap_err("Failed to apply specifiers")?,
                None => Cow::Owned(format!("/usr/share/factory/{path}")),
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
            owner: resolve_uid(user, id_cache)?,
            group: resolve_gid(group, id_cache)?,
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
            owner: resolve_uid(user, id_cache)?,
            group: resolve_gid(group, id_cache)?,
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
                Some(c) => resolver.resolve(c).wrap_err("Failed to apply specifiers")?,
                None => Cow::Owned(format!("/usr/share/factory/{path}")),
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
            owner: resolve_uid(user, id_cache)?,
            group: resolve_gid(group, id_cache)?,
        }),
        systemd_tmpfiles::Directive::AdjustAccess {
            recursive,
            mode,
            user,
            group,
        } => {
            if *recursive {
                tracing::warn!("Recursive Z not properly supported for {path}");
            }
            Properties::Permissions(Permissions {
                mode: mode
                    .as_ref()
                    .map(|m| Mode::new(m.mode()))
                    .unwrap_or(Mode::new(0o644)),
                owner: resolve_uid(user, id_cache)?,
                group: resolve_gid(group, id_cache)?,
            })
        }
        systemd_tmpfiles::Directive::SetExtendedAttributes { .. } => return Ok(()),
        systemd_tmpfiles::Directive::SetAttributes { .. } => return Ok(()),
        systemd_tmpfiles::Directive::SetAcl { .. } => return Ok(()),
        _ => todo!(),
    };

    do_insert(files, &path, props, flags)
}

/// Insert into the hash map, trying to update existing entries (if existing).
fn do_insert(
    files: &mut AHashMap<PathBuf, FileEntry>,
    path: &str,
    props: Properties,
    flags: FileFlags,
) -> eyre::Result<()> {
    let normalised_path = std::path::absolute(PathBuf::from(path))?;
    match files.entry(normalised_path) {
        Entry::Occupied(mut entry) => {
            if let Properties::Permissions(Permissions {
                mode: new_mode,
                owner: new_owner,
                group: new_group,
            }) = props
            {
                // Update existing permissions
                match &mut entry.get_mut().properties {
                    // Handle cases where we keep type but modify fields
                    Properties::Permissions(Permissions { mode, owner, group })
                    | Properties::RegularFileSystemd(RegularFileSystemd {
                        mode,
                        owner,
                        group,
                        ..
                    })
                    | Properties::RegularFile(RegularFile {
                        mode, owner, group, ..
                    })
                    | Properties::Directory(Directory { mode, owner, group })
                    | Properties::Fifo(Fifo { mode, owner, group })
                    | Properties::DeviceNode(DeviceNode {
                        mode, owner, group, ..
                    }) => {
                        *mode = new_mode;
                        *owner = new_owner;
                        *group = new_group;
                    }
                    // Symlinks don't have permissions, but we can update owner and group
                    Properties::Symlink(Symlink { owner, group, .. }) => {
                        *owner = new_owner;
                        *group = new_group;
                    }
                    // Basic files get an upgrade with more info
                    Properties::RegularFileBasic(RegularFileBasic { size, checksum }) => {
                        entry.get_mut().properties =
                            Properties::RegularFileSystemd(RegularFileSystemd {
                                mode: new_mode,
                                owner: new_owner,
                                group: new_group,
                                size: *size,
                                checksum: checksum.clone(),
                                contents: None,
                            });
                    }
                    // Unknown gets upgraded to a replacement
                    Properties::Unknown => {
                        do_insert_inner(&mut entry, path, props, flags);
                    }
                    Properties::Special | Properties::Removed => {
                        tracing::warn!("Tried to update permissions on non-permissions entry");
                    }
                }
            } else {
                // Just replace the entire entry
                do_insert_inner(&mut entry, path, props, flags);
            }
        }
        Entry::Vacant(entry) => {
            entry.insert(FileEntry {
                package: None,
                path: PathBuf::from(path),
                properties: props,
                flags,
                source: NAME,
                seen: Default::default(),
            });
        }
    };
    Ok(())
}

/// Helper to [`do_insert`] that inserts into an occupied entry.
fn do_insert_inner(
    entry: &mut std::collections::hash_map::OccupiedEntry<'_, PathBuf, FileEntry>,
    path: &str,
    props: Properties,
    flags: FileFlags,
) {
    entry.insert(FileEntry {
        package: None,
        path: PathBuf::from(path),
        properties: props,
        flags,
        source: NAME,
        seen: Default::default(),
    });
}

/// Handles the recursive copy instructions from systemd-tmpfiles
fn recursive_copy(
    files: &mut AHashMap<PathBuf, FileEntry>,
    source_path: &Path,
    target_path: &str,
    flags: FileFlags,
) -> eyre::Result<()> {
    // Get source metadata
    let source_metadata = match source_path.metadata() {
        Ok(metadata) => metadata,
        Err(e) => {
            tracing::warn!("Failed to read metadata for {source_path:?}: {e}",);
            return Ok(());
        }
    };

    let props = match source_metadata.is_dir() {
        true => {
            // Try to read directory (but log and skip if not possible)
            let dir_iter = match std::fs::read_dir(source_path) {
                Ok(iter) => iter,
                Err(e) => {
                    tracing::warn!("Failed to read directory {source_path:?}: {e}",);
                    return Ok(());
                }
            };
            // Recurse though directory contents
            for entry in dir_iter {
                let entry =
                    entry.wrap_err_with(|| format!("Failed to read directory {source_path:?}"))?;
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
                tracing::warn!(
                    "Failed to generate checksum for {source_path:?}: {}",
                    checksum.expect_err("Must be an error in this control flow")
                );
                return Ok(());
            };
            Properties::RegularFileSystemd(RegularFileSystemd {
                mode: Mode::new(source_metadata.mode() & MODE_MASK),
                owner: Uid::new(source_metadata.uid()),
                group: Gid::new(source_metadata.gid()),
                size: Some(source_metadata.len()),
                checksum,
                contents: None,
            })
        }
    };

    do_insert(files, target_path, props, flags)
}

/// Generate a checksum from a path on the system (needed for copy directives)
fn generate_checksum_from_file(path: &Path) -> eyre::Result<Checksum> {
    let mut reader =
        std::fs::File::open(path).wrap_err_with(|| format!("IO error while reading {path:?}"))?;
    sha256_readable(&mut reader)
}

/// Cache for UID and GID lookups (they are slow when using glibc at least)
#[derive(Default)]
struct IdCache<'a>(AHashMap<IdCacheKey<'a>, u32>);

/// Key for the ID cache
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IdCacheKey<'a> {
    User(&'a str),
    Group(&'a str),
}

impl IdCacheKey<'_> {
    const fn as_str(&self) -> &str {
        match self {
            IdCacheKey::User(s) => s,
            IdCacheKey::Group(s) => s,
        }
    }
}

impl<'a> IdCache<'a> {
    /// Look up in ID cache, and if not found use the provided resolver to
    /// resolve and insert the ID
    fn lookup(
        &mut self,
        key: IdCacheKey<'a>,
        resolver: impl FnOnce(&'_ str) -> eyre::Result<u32>,
    ) -> eyre::Result<u32> {
        let cache_entry = self.0.entry(key);
        match cache_entry {
            Entry::Occupied(e) => Ok(*e.get()),
            Entry::Vacant(v) => {
                let id = resolver(key.as_str())?;
                v.insert(id);
                Ok(id)
            }
        }
    }
}

/// Resolve a group identifier to a GID
fn resolve_gid<'entry>(
    group: &'entry systemd_tmpfiles::Id,
    id_cache: &mut IdCache<'entry>,
) -> eyre::Result<Gid> {
    match group {
        systemd_tmpfiles::Id::Caller { new_only: _ } => Ok(Gid::new(0)),
        systemd_tmpfiles::Id::Numeric { id, new_only: _ } => Ok(Gid::new(*id)),
        systemd_tmpfiles::Id::Name { name, new_only: _ } => id_cache
            .lookup(
                IdCacheKey::Group(name.as_str()),
                |name: &str| -> eyre::Result<u32> {
                    let entry = nix::unistd::Group::from_name(name)
                        .wrap_err_with(|| format!("Failed to resolve GID for {name}"))?
                        .ok_or_else(|| eyre::eyre!("Failed to resolve GID for {name}"))?;
                    Ok(entry.gid.as_raw())
                },
            )
            .map(Gid::new),
        _ => todo!(),
    }
}

/// Resolve a user identifier to a UID
fn resolve_uid<'entry>(
    user: &'entry systemd_tmpfiles::Id,
    id_cache: &mut IdCache<'entry>,
) -> eyre::Result<Uid> {
    match user {
        systemd_tmpfiles::Id::Caller { new_only: _ } => Ok(Uid::new(0)),
        systemd_tmpfiles::Id::Numeric { id, new_only: _ } => Ok(Uid::new(*id)),
        systemd_tmpfiles::Id::Name { name, new_only: _ } => id_cache
            .lookup(
                IdCacheKey::User(name.as_str()),
                |name: &str| -> eyre::Result<u32> {
                    let entry = nix::unistd::User::from_name(name)
                        .wrap_err_with(|| format!("Failed to resolve UID for {name}"))?
                        .ok_or_else(|| eyre::eyre!("Failed to resolve UID for {name}"))?;
                    Ok(entry.uid.as_raw())
                },
            )
            .map(Uid::new),
        _ => todo!(),
    }
}
