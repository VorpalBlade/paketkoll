//! Scan the file system

use std::sync::Arc;

use anyhow::Context;
use compact_str::CompactString;
use konfigkoll_types::FsInstruction;
use ouroboros::self_referencing;
use paketkoll_core::config::{
    CheckAllFilesConfiguration, CommonFileCheckConfiguration, ConfigFiles,
};
use paketkoll_core::file_ops::{
    canonicalize_file_entries, create_path_map, mismatching_and_unexpected_files,
};
use paketkoll_types::backend::Files;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::PathMap;
use paketkoll_types::intern::Interner;

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
    ignores: &[CompactString],
    trust_mtime: bool,
) -> anyhow::Result<(ScanResult, Vec<FsInstruction>)> {
    tracing::debug!("Scanning filesystem");
    let mut fs_instructions_sys = vec![];
    let mut files = backend.files(interner).with_context(|| {
        format!(
            "Failed to collect information from backend {}",
            backend.name()
        )
    })?;
    if backend.may_need_canonicalization() {
        tracing::debug!("Canonicalizing file entries");
        canonicalize_file_entries(&mut files);
    }
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
