//! Declaration of backends

use crate::files::FileEntry;
use crate::intern::{Interner, PackageRef};
use crate::package::PackageInterned;
use ahash::AHashMap;
use ahash::AHashSet;
use anyhow::{anyhow, Context};
use compact_str::CompactString;
use dashmap::DashMap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Type for mapping of package aliases
pub type PackageAliases = AHashMap<PackageRef, PackageRef>;

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
    /// (i.e. paths may be inaccuarate)
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
    ) -> anyhow::Result<()>;

    /// Mark packages as dependencies and manually installed
    fn mark(&self, dependencies: &[&str], manual: &[&str]) -> anyhow::Result<()>;
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

/// Convert a package vector to a package map
pub fn packages_to_split_maps(packages: Vec<PackageInterned>) -> (PackageMap, PackageAliases) {
    let mut package_map: PackageMap =
        AHashMap::with_capacity_and_hasher(packages.len(), ahash::RandomState::new());
    let mut package_aliases: AHashMap<PackageRef, PackageRef> = AHashMap::new();
    let mut alias_insert = |v, canon| {
        if v != canon {
            package_aliases.insert(v, canon);
        }
    };
    for package in packages.into_iter() {
        let canonical_id = *package.canonical_id();
        for provides in &package.provides {
            alias_insert(*provides, canonical_id);
        }
        if package.ids.is_empty() {
            alias_insert(package.name, canonical_id);
        } else {
            for id in &package.ids {
                alias_insert(*id, canonical_id);
            }
        }
        package_map.insert(canonical_id, package);
    }
    (package_map, package_aliases)
}
