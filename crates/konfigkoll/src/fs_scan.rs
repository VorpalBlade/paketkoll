//! Scan the file system

use ahash::AHashSet;
use compact_str::CompactString;
use dashmap::DashMap;
use eyre::WrapErr;
use itertools::Itertools;
use konfigkoll_types::FsInstruction;
use ouroboros::self_referencing;
use paketkoll_core::config::CheckAllFilesConfiguration;
use paketkoll_core::config::CommonFileCheckConfiguration;
use paketkoll_core::config::ConfigFiles;
use paketkoll_core::file_ops::canonicalize_file_entries;
use paketkoll_core::file_ops::create_path_map;
use paketkoll_core::file_ops::mismatching_and_unexpected_files;
use paketkoll_types::backend::ArchiveQueryError;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::PathMap;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use rayon::prelude::*;
use std::sync::Arc;

#[self_referencing]
pub(crate) struct ScanResult {
    pub files: Vec<FileEntry>,
    #[borrows(files)]
    #[covariant]
    pub path_map: PathMap<'this>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn scan_fs(
    interner: &Arc<Interner>,
    backend: &Arc<dyn Files>,
    package_map: &PackageMap,
    ignores: &[CompactString],
    trust_mtime: bool,
) -> eyre::Result<(ScanResult, Vec<FsInstruction>)> {
    tracing::debug!("Scanning filesystem");
    let mut fs_instructions_sys = vec![];
    let files = if backend.prefer_files_from_archive() {
        tracing::debug!("Using files from archives");
        let all = package_map.keys().cloned().collect::<Vec<_>>();
        let mut files = backend.files_from_archives(&all, package_map, interner)?;
        // For all the failures, attempt to resolve them with the traditional backend
        let missing: AHashSet<PackageRef> = files
            .iter()
            .filter_map(|e| match e {
                Ok(_) => None,
                Err(ArchiveQueryError::PackageMissing {
                    query: _,
                    alternates,
                }) => Some(alternates),
                Err(err) => {
                    tracing::error!("Unknown error: {err}");
                    None
                }
            })
            .flatten()
            .cloned()
            .collect();
        let mut extra_files = vec![];
        if !missing.is_empty() {
            files.retain(Result::is_ok);
            tracing::warn!(
                "Attempting to resolve missing files with traditional backend (going to take \
                 extra time)"
            );
            let traditional_files = backend.files(interner)?;
            for file in traditional_files.into_iter() {
                if let Some(pkg_ref) = file.package {
                    if missing.contains(&pkg_ref) {
                        extra_files.push(file);
                    }
                }
            }
            if backend.may_need_canonicalization() {
                tracing::debug!("Canonicalizing file entries");
                canonicalize_file_entries(&mut extra_files);
            }
        }
        let mut files: Vec<_> = files
            .into_iter()
            .map(|e| e.expect("All errors should be filtered out by now"))
            .collect();
        if backend.may_need_canonicalization() {
            tracing::debug!("Canonicalizing file entries");
            files.par_iter_mut().for_each(|entry| {
                canonicalize_file_entries(&mut entry.1);
            });
        }
        let file_map = DashMap::new();
        files
            .into_par_iter()
            .flat_map_iter(|(_pkg, files)| files)
            .for_each(|entry| {
                file_map.insert(entry.path.clone(), entry);
            });
        extra_files.into_iter().for_each(|entry| {
            let old = file_map.insert(entry.path.clone(), entry);
            if let Some(old) = old {
                if old.properties.is_dir() == Some(false) {
                    tracing::warn!("Duplicate file entry for {}", old.path.display());
                }
            }
        });
        file_map.into_iter().map(|(_, v)| v).collect_vec()
    } else {
        let mut files = backend.files(interner).wrap_err_with(|| {
            format!(
                "Failed to collect information from backend {}",
                backend.name()
            )
        })?;
        if backend.may_need_canonicalization() {
            tracing::debug!("Canonicalizing file entries");
            canonicalize_file_entries(&mut files);
        }
        files
    };
    // Drop mutability
    let files = files;

    tracing::debug!("Building path map");
    let scan_result = ScanResultBuilder {
        files,
        path_map_builder: |files| create_path_map(files.as_slice()),
    }
    .build();

    tracing::debug!("Checking for unexpected files");
    let common_config = CommonFileCheckConfiguration::builder()
        .trust_mtime(trust_mtime)
        .config_files(ConfigFiles::Include)
        .build()?;
    let unexpected_config = CheckAllFilesConfiguration::builder()
        .canonicalize_paths(backend.may_need_canonicalization())
        .ignored_paths(ignores.to_owned())
        .build()?;

    let issues = mismatching_and_unexpected_files(
        scan_result.borrow_files(),
        scan_result.borrow_path_map(),
        &common_config,
        &unexpected_config,
    )?;

    // Convert issues to an instruction stream
    fs_instructions_sys
        .extend(konfigkoll_core::conversion::convert_issues_to_fs_instructions(issues)?);
    // Ensure instructions are sorted
    fs_instructions_sys.sort();
    Ok((scan_result, fs_instructions_sys))
}
