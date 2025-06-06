//! Contain file checking functionality

use compact_str::CompactString;
use eyre::WrapErr;
use ignore::Match;
use ignore::WalkBuilder;
use ignore::WalkState;
use ignore::overrides::OverrideBuilder;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::OriginalFilesResult;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::PathMap;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use paketkoll_types::issue::Issue;
use paketkoll_types::issue::IssueKind;
use paketkoll_types::issue::PackageIssue;
use rayon::prelude::*;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// Perform a query of original files
#[doc(hidden)]
pub fn original_files(
    backend: crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
    queries: &[OriginalFileQuery],
) -> eyre::Result<OriginalFilesResult> {
    let interner = Interner::new();
    let backend_impl = backend
        .create_full(backend_config, &interner)
        .wrap_err_with(|| format!("Failed to create backend for {backend}"))?;

    let package_map = backend_impl
        .package_map_complete(&interner)
        .wrap_err_with(|| format!("Failed to collect information from backend {backend}"))?;

    let results = backend_impl
        .original_files(queries, &package_map, &interner)
        .wrap_err_with(|| format!("Failed to collect original files from backend {backend}"))?;

    Ok(results)
}

/// Check file system for differences using the given configuration
pub fn check_installed_files(
    backend: crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
    filecheck_config: &crate::config::CommonFileCheckConfiguration,
) -> eyre::Result<(Interner, Vec<PackageIssue>)> {
    let interner = Interner::new();
    let backend_impl = backend
        .create_files(backend_config, &interner)
        .wrap_err_with(|| format!("Failed to create backend for {backend}"))?;
    // Get distro specific file list
    let results = backend_impl
        .files(&interner)
        .wrap_err_with(|| format!("Failed to collect information from backend {backend}"))?;

    tracing::debug!("Checking file system");
    // For all file entries, check on file system
    // Par-bridge is used here to avoid batching. We do too much work for
    // batching to be useful, and this way we avoid pathological cases with
    // slow batches of large files at the end.
    let mismatches: Vec<_> = results
        .into_iter()
        .par_bridge()
        .filter_map(|file_entry| {
            match crate::backend::filesystem::check_file(&file_entry, filecheck_config) {
                Ok(Some(inner)) => Some((file_entry.package, inner)),
                Ok(None) => None,
                Err(err) => {
                    let issues = smallvec::smallvec![IssueKind::FsCheckError(Box::new(err))];
                    Some((
                        file_entry.package,
                        Issue::new(file_entry.path, issues, Some(file_entry.source)),
                    ))
                }
            }
        })
        .collect();

    Ok((interner, mismatches))
}

/// Check file system for differences (including unexpected files) using the
/// given configuration
pub fn check_all_files(
    backend: crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
    filecheck_config: &crate::config::CommonFileCheckConfiguration,
    unexpected_cfg: &crate::config::CheckAllFilesConfiguration,
) -> eyre::Result<(Interner, Vec<PackageIssue>)> {
    let interner = Interner::new();
    // Collect distro files
    let backend_impl = backend
        .create_files(backend_config, &interner)
        .wrap_err_with(|| format!("Failed to create backend for {backend}"))?;
    // Get distro specific file list
    let mut expected_files = backend_impl
        .files(&interner)
        .wrap_err_with(|| format!("Failed to collect information from backend {backend}",))?;

    // Possibly canonicalize paths
    if unexpected_cfg.canonicalize_paths {
        tracing::debug!("Canonicalizing paths");
        canonicalize_file_entries(&mut expected_files);
    }

    tracing::debug!("Preparing data structures");
    // We want a hashmap from path to data here.
    let path_map = create_path_map(&expected_files);

    let mismatches = mismatching_and_unexpected_files(
        &expected_files,
        &path_map,
        filecheck_config,
        unexpected_cfg,
    )?;

    // Drop on a background thread, this help a bit.
    drop(path_map);
    rayon::spawn(move || {
        drop(expected_files);
    });
    Ok((interner, mismatches))
}

/// Find mismatching and unexpected files
///
/// This takes a list of expected files to be seen and some config objects.
///
/// Returned will be a list of issues found (along with which package is
/// associated with that file if known).
pub fn mismatching_and_unexpected_files<'a>(
    expected_files: &'a Vec<FileEntry>,
    path_map: &PathMap<'a>,
    filecheck_config: &crate::config::CommonFileCheckConfiguration,
    unexpected_cfg: &crate::config::CheckAllFilesConfiguration,
) -> eyre::Result<Vec<(Option<PackageRef>, Issue)>> {
    tracing::debug!("Building ignores");
    // Build glob set of ignores
    let overrides = build_ignore_overrides(&unexpected_cfg.ignored_paths)?;

    tracing::debug!("Walking file system");
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
                        match crate::backend::filesystem::check_file(file_entry, filecheck_config) {
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
                                        Issue::new(
                                            file_entry.path.clone(),
                                            issues,
                                            Some(file_entry.source),
                                        ),
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
                                    None,
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

    tracing::debug!("Identifying and processing missing files");
    // Identify missing files (we should have seen them walking through the file
    // system)
    expected_files.par_iter().for_each(|file_entry| {
        if file_entry.seen.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Match::Ignore(_) = overrides.matched(
            &file_entry.path,
            file_entry.properties.is_dir().unwrap_or(false),
        ) {
            return;
        }
        // We also need to check the parent directories against ignores
        for parent in file_entry.path.ancestors() {
            match overrides.matched(parent, true) {
                Match::None => (),
                Match::Ignore(_) => return,
                Match::Whitelist(_) => break,
            }
        }
        collector
            .send((
                file_entry.package,
                Issue::new(
                    file_entry.path.clone(),
                    smallvec::smallvec![IssueKind::Missing],
                    Some(file_entry.source),
                ),
            ))
            .expect("Unbounded queue");
    });

    tracing::debug!("Collecting results");
    // Collect all items from queue into vec
    let mut mismatches = Vec::new();
    for item in collected_issues.drain() {
        mismatches.push(item);
    }
    Ok(mismatches)
}

#[doc(hidden)]
/// Build the ignore overrides for the given configuration
pub fn build_ignore_overrides(
    ignored_paths: &Vec<CompactString>,
) -> eyre::Result<ignore::overrides::Override> {
    let mut builder = OverrideBuilder::new("/");
    for pattern in BUILTIN_IGNORES {
        builder.add(pattern).expect("Builtin ignore failed");
    }
    for pattern in ignored_paths {
        builder.add(&("!".to_string() + pattern.as_str()))?;
    }
    Ok(builder.build()?)
}

/// Create a path map for a set of expected files
pub fn create_path_map(expected_files: &[FileEntry]) -> PathMap<'_> {
    let mut path_map: PathMap<'_> =
        PathMap::with_capacity_and_hasher(expected_files.len(), ahash::RandomState::new());
    for file_entry in expected_files {
        path_map.insert(&file_entry.path, file_entry);
    }
    path_map
}

/// Canonicalize paths in file entries.
///
/// This is needed for Debian as packages don't make sense wrt /usr-merge
pub fn canonicalize_file_entries(results: &mut Vec<FileEntry>) {
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
                        // We only need to do work here if the parent path actually changed (saves
                        // ~10 ms).
                        if canonical_parent != parent {
                            file_entry.path = canonical_parent.join(filename);
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to canonicalize path: {:?} ({:?})",
                            file_entry.path,
                            err
                        );
                    }
                }
            }
            (None, _) => tracing::error!(
                "Failed to resolve parent of path: {:?}: {:?}",
                file_entry.path,
                file_entry
            ),
            (_, None) => {
                tracing::error!("Failed to resolve filenameI of path: {:?}", file_entry.path);
            }
        }
    });
}

/// Attempt to make sense of the errors from the "ignore" crate.
///
/// This involves recursively mapping into some of the variants to find the
/// actual error.
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
                    None,
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
                        None,
                    ),
                )
            }
        },
    }
}

/// Built in ignores for [`check_all_files`]
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
