//! The various backends implementing distro specific support

use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use anyhow::Context;
use dashmap::DashMap;
use ignore::{overrides::OverrideBuilder, Match, WalkBuilder, WalkState};
use rayon::prelude::*;

use crate::types::{FileEntry, Issue, IssueKind, PackageInterner, PackageIssue};

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

pub(crate) mod filesystem;

/// Check file system for differences using the given configuration
pub fn check(
    config: &crate::config::CommonConfiguration,
) -> anyhow::Result<(crate::types::PackageInterner, Vec<PackageIssue>)> {
    let backend = config
        .backend
        .create(config)
        .with_context(|| format!("Failed to create backend for {}", config.backend))?;
    let interner = PackageInterner::new();
    // Get distro specific file list
    let results = backend.files(&interner).with_context(|| {
        format!(
            "Failed to collect information from backend {}",
            config.backend
        )
    })?;

    log::debug!(target: "paketkoll_core::backend", "Checking file system");
    // For all file entries entries, check on file system
    // Par-bridge is used here to avoid batching. We do too much work for
    // batching to be useful, and this way we avoid pathological cases with
    // slow batches of large files at the end.
    let mismatches: Vec<_> = results
        .into_iter()
        .par_bridge()
        .filter_map(
            |file_entry| match filesystem::check_file(&file_entry, config) {
                Ok(Some(inner)) => Some((file_entry.package, inner)),
                Ok(None) => None,
                Err(err) => {
                    let issues = smallvec::smallvec![IssueKind::FsCheckError(Box::new(err))];
                    Some((file_entry.package, Issue::new(file_entry.path, issues)))
                }
            },
        )
        .collect();

    Ok((interner, mismatches))
}

/// Check file system for differences (including unexpected files) using the given configuration
pub fn check_unexpected(
    common_cfg: &crate::config::CommonConfiguration,
    unexpected_cfg: &crate::config::CheckUnexpectedConfiguration,
) -> anyhow::Result<(crate::types::PackageInterner, Vec<PackageIssue>)> {
    // Collect distro files
    let backend = common_cfg
        .backend
        .create(common_cfg)
        .with_context(|| format!("Failed to create backend for {}", common_cfg.backend))?;
    let interner = PackageInterner::new();
    // Get distro specific file list
    let mut results = backend.files(&interner).with_context(|| {
        format!(
            "Failed to collect information from backend {}",
            common_cfg.backend
        )
    })?;

    // Possibly canonicalize paths
    if unexpected_cfg.canonicalize_paths {
        log::debug!(target: "paketkoll_core::backend", "Canonicalizing paths");
        canonicalize_file_entries(&mut results);
    }

    log::debug!(target: "paketkoll_core::backend", "Preparing data structures");
    // We want a hashmap from path to data here.
    let path_map: DashMap<&Path, &FileEntry, ahash::RandomState> =
        DashMap::with_capacity_and_hasher(results.len(), ahash::RandomState::new());
    results.par_iter().for_each(|file_entry| {
        path_map.insert(&file_entry.path, file_entry);
    });

    // Build glob set of ignores
    let overrides = {
        let mut builder = OverrideBuilder::new("/");
        // Add standard ignores
        for pattern in BUILTIN_IGNORES {
            builder.add(pattern).expect("Builtin ignore failed");
        }
        // Add user ignores
        for pattern in &unexpected_cfg.ignored_paths {
            builder.add(&("!".to_string() + pattern.as_str()))?;
        }
        builder.build()?
    };

    log::debug!(target: "paketkoll_core::backend", "Walking file system");
    let walker = WalkBuilder::new("/")
        .hidden(false)
        .parents(false)
        .ignore(false)
        .overrides(overrides.clone())
        .git_global(false)
        .git_ignore(false)
        .git_exclude(false)
        .follow_links(false)
        .same_file_system(false)
        .threads(num_cpus::get())
        .build_parallel();

    let (collector, collected_issues) = flume::unbounded();

    walker.run(|| {
        Box::new(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if let Some(file_entry) = path_map.get(path) {
                        file_entry
                            .seen
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        match filesystem::check_file(&file_entry, common_cfg) {
                            Ok(Some(inner)) => {
                                collector
                                    .send((file_entry.package, inner))
                                    .expect("Unbounded queue");
                            }
                            Ok(None) => (),
                            Err(err) => {
                                let issues =
                                    smallvec::smallvec![IssueKind::FsCheckError(Box::new(err))];
                                collector
                                    .send((
                                        file_entry.package,
                                        Issue::new(file_entry.path.clone(), issues),
                                    ))
                                    .expect("Unbounded queue");
                            }
                        }
                    } else {
                        // Unexpected file found
                        collector
                            .send((
                                None,
                                Issue::new(
                                    path.to_path_buf(),
                                    smallvec::smallvec![IssueKind::Unexpected],
                                ),
                            ))
                            .expect("Unbounded queue");
                    }
                }
                Err(ignore_err) => {
                    collector
                        .send(interpret_ignore_error(ignore_err, None))
                        .expect("Unbounded queue");
                }
            }
            WalkState::Continue
        })
    });

    // Identify missing files (we should have seen them walking through the file system)
    results.par_iter().for_each(|file_entry| {
        if file_entry.seen.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Match::Ignore(_) = overrides.matched(
            &file_entry.path,
            file_entry.properties.is_dir().unwrap_or(false),
        ) {
            return;
        }
        collector
            .send((
                file_entry.package,
                Issue::new(
                    file_entry.path.clone(),
                    smallvec::smallvec![IssueKind::Missing],
                ),
            ))
            .expect("Unbounded queue");
    });

    // Collect all items from queue into vec
    let mut mismatches = Vec::new();
    for item in collected_issues.drain() {
        mismatches.push(item);
    }

    // Drop on a background thread, this help a bit.
    drop(path_map);
    rayon::spawn(move || {
        drop(results);
    });

    Ok((interner, mismatches))
}

/// Canonicalize paths in file entries.
///
/// This is needed for Debian as packages don't make sense wrt /usr-merge
fn canonicalize_file_entries(results: &mut Vec<FileEntry>) {
    results.par_iter_mut().for_each(|file_entry| {
        if file_entry.path.as_os_str().as_bytes() == b"/" {
            return;
        }
        // We only want to canonicalize the parenting path, not the file itself,
        // otherwise we can't check properties of symlinks. Since Debian doesn't
        // tell us what is a symlink or not, just use the parent path.
        let parent = file_entry.path.parent();
        let filename = file_entry.path.file_name();
        match (parent, filename) {
            (Some(parent), Some(filename)) => {
                match parent.canonicalize() {
                    Ok(canonical_parent) => {
                        // We only need to do work here if the parent path actually changed (saves ~10 ms).
                        if canonical_parent != parent {
                            file_entry.path = canonical_parent.join(filename);
                        }
                    }
                    Err(err) => {
                        log::error!(
                            "Failed to canonicalize path: {:?} ({:?})",
                            file_entry.path,
                            err
                        );
                    }
                }
            }
            (None, _) => log::error!("Failed to resolve parent of path: {:?}", file_entry.path),
            (_, None) => {
                log::error!("Failed to resolve filenameI of path: {:?}", file_entry.path)
            }
        }
    });
}

/// Attempt to make sense of the errors from the "ignore" crate.
///
/// This involves recursively mapping into some of the variants to find the actual error.
fn interpret_ignore_error(ignore_err: ignore::Error, context: Option<PathBuf>) -> PackageIssue {
    match ignore_err {
        ignore::Error::Partial(_) | ignore::Error::WithLineNumber { .. } => {
            unreachable!("We don't parse ignore files")
        }
        ignore::Error::InvalidDefinition | ignore::Error::UnrecognizedFileType(_) => {
            unreachable!("File types not used")
        }
        ignore::Error::Glob { .. } => unreachable!("We don't use globs from ignores"),
        ignore::Error::WithPath { path, err } => interpret_ignore_error(*err, Some(path)),
        ignore::Error::WithDepth { depth: _, err } => interpret_ignore_error(*err, None),
        ignore::Error::Loop { .. } => unreachable!("We don't follow symlinks"),
        ignore::Error::Io(io_error) => match io_error.kind() {
            std::io::ErrorKind::PermissionDenied => (
                None,
                Issue::new(
                    context.unwrap_or_else(|| PathBuf::from("UNKNOWN_PATH")),
                    smallvec::smallvec![IssueKind::PermissionDenied],
                ),
            ),
            _ => {
                let issues =
                    smallvec::smallvec![IssueKind::FsCheckError(Box::new(io_error.into()))];
                (
                    None,
                    Issue::new(
                        context.unwrap_or_else(|| PathBuf::from("UNKNOWN_PATH")),
                        issues,
                    ),
                )
            }
        },
    }
}

pub(crate) trait Name {
    /// The name of the backend (for logging and debugging purposes)
    // Temporary, this will get exposed
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
}

/// A package manager backend
pub(crate) trait Files: Name {
    /// Collect a list of files managed by the package manager including
    /// any available metadata such as checksums or timestamps about those files
    fn files(
        &self,
        interner: &crate::types::PackageInterner,
    ) -> anyhow::Result<Vec<crate::types::FileEntry>>;
}

/// A package manager backend
// Temporary, this will get exposed
#[allow(dead_code)]
pub(crate) trait Packages: Name {
    /// Collect a list of all installed packages
    fn packages(
        &self,
        interner: &crate::types::PackageInterner,
    ) -> anyhow::Result<Vec<crate::types::Package>>;
}

// TODO: Operations to add
// - Get source file from package (possibly downloading the package to cache if needed)
// - Does a paccache equivalent exist for Debian or do we need to implement smart cache
//   cleaning as a separate tool?

const BUILTIN_IGNORES: &[&str] = &[
    "!**/lost+found",
    "!/dev/",
    "!/home/",
    "!/media/",
    "!/mnt/",
    "!/proc/",
    "!/root/",
    "!/run/",
    "!/sys/",
    "!/tmp/",
    "!/var/tmp/",
];
