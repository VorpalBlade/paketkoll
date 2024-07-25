//! Declaration of backends

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ahash::AHashMap;
use ahash::AHashSet;
use anyhow::{anyhow, Context};
use compact_str::CompactString;
use dashmap::DashMap;

use crate::files::FileEntry;
use crate::intern::{Interner, PackageRef};
use crate::package::PackageInterned;

/// Which backend to use for the system package manager
#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, strum::Display, strum::EnumString,
)]
pub enum Backend {
    /// Backend for Arch Linux and derived distros (pacman)
    #[strum(to_string = "pacman")]
    Pacman,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[strum(to_string = "apt")]
    Apt,
    /// Backend for flatpak (package list only)
    #[strum(to_string = "flatpak")]
    Flatpak,
    /// Backend for systemd-tmpfiles (file list only)
    #[strum(to_string = "systemd-tmpfiles")]
    SystemdTmpfiles,
}

/// Type for a mapping of package IDs to package data
pub type PackageMap = AHashMap<PackageRef, PackageInterned>;

/// Type for a mapping from backend to package map
pub type PackageMapMap = BTreeMap<Backend, Arc<PackageMap>>;

/// Type of map of package backends
pub type PackageBackendMap = BTreeMap<Backend, Arc<dyn Packages>>;

/// Type of map of file backends
pub type FilesBackendMap = BTreeMap<Backend, Arc<dyn Files>>;

/// Get the name of a backend (useful in dynamic dispatch for generating reports)
pub trait Name: Send + Sync + std::fmt::Debug {
    /// The name of the backend (for logging and debugging purposes)
    fn name(&self) -> &'static str;

    /// The backend enum value corresponding to this backend
    fn as_backend_enum(&self) -> Backend;
}

/// A package manager backend
pub trait Files: Name {
    /// Collect a list of files managed by the package manager including
    /// any available metadata such as checksums or timestamps about those files
    fn files(&self, interner: &Interner) -> anyhow::Result<Vec<FileEntry>>;

    /// True if this backend may benefit from path canonicalization for certain scans
    /// (i.e. paths may be inaccurate)
    fn may_need_canonicalization(&self) -> bool {
        false
    }

    /// Find the owners of the specified files
    fn owning_packages(
        &self,
        paths: &AHashSet<&Path>,
        interner: &Interner,
    ) -> anyhow::Result<DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>>;

    /// Get the original contents of files
    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> anyhow::Result<ahash::AHashMap<OriginalFileQuery, Vec<u8>>>;
}

/// Query type for original file contents
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct OriginalFileQuery {
    pub package: CompactString,
    pub path: CompactString,
}

/// A package manager backend (reading list of packages)
pub trait Packages: Name {
    /// Collect a list of all installed packages
    fn packages(&self, interner: &Interner) -> anyhow::Result<Vec<PackageInterned>>;

    /// Collect a map of packages with all alternative names as keys
    fn package_map_complete(&self, interner: &Interner) -> anyhow::Result<PackageMap> {
        let packages = self
            .packages(interner)
            .with_context(|| anyhow!("Failed to load package list"))?;
        Ok(packages_to_package_map(packages))
    }

    /// Perform installation and uninstallation of a bunch of packages
    ///
    /// The package name format depends on the backend.
    fn transact(
        &self,
        install: &[&str],
        uninstall: &[&str],
        ask_confirmation: bool,
    ) -> Result<(), PackageManagerError>;

    /// Mark packages as dependencies and manually installed
    fn mark(&self, dependencies: &[&str], manual: &[&str]) -> Result<(), PackageManagerError>;

    /// Ask package manager to uninstall unused packages
    ///
    /// If needed, this should internally repeat until no more packages can be removed (or the used aborted)
    fn remove_unused(&self, ask_confirmation: bool) -> Result<(), PackageManagerError>;
}

/// Errors that package manager transactions can produce
#[derive(Debug, thiserror::Error)]
pub enum PackageManagerError {
    /// This operation isn't supported by this backend
    #[error("Operation not supported: {0}")]
    UnsupportedOperation(&'static str),
    /// All other errors
    #[error("{0:?}")]
    Other(#[from] anyhow::Error),
}

/// Convert a package vector to a package map
pub fn packages_to_package_map(packages: Vec<PackageInterned>) -> PackageMap {
    let mut package_map =
        AHashMap::with_capacity_and_hasher(packages.len(), ahash::RandomState::new());
    for package in packages.into_iter() {
        if package.ids.is_empty() {
            package_map.insert(package.name, package);
        } else {
            for id in &package.ids {
                package_map.insert(*id, package.clone());
            }
        }
    }
    package_map
}
