//! The various backends implementing distro specific support

use anyhow::Context;
use rayon::prelude::*;

use crate::types::{Issue, IssueKind, PackageInterner, PackageRef};

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

pub(crate) mod filesystem;

/// Check file system for differences using the given configuration
pub fn check(
    config: &crate::config::CheckConfiguration,
) -> anyhow::Result<(
    crate::types::PackageInterner,
    impl Iterator<Item = (Option<PackageRef>, Issue)>,
)> {
    let backend = config
        .backend
        .create()
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
                    let issues = smallvec::smallvec![IssueKind::FileCheckError(Box::new(err))];
                    Some((file_entry.package, Issue::new(file_entry.path, issues)))
                }
            },
        )
        .collect();

    Ok((interner, mismatches.into_iter()))
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
