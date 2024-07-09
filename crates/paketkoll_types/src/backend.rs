//! Declaration of backends

use crate::files::FileEntry;
use crate::intern::{Interner, PackageRef};
use crate::package::PackageInterned;
use ahash::AHashMap;
use ahash::AHashSet;
use anyhow::{anyhow, Context};
use compact_str::CompactString;
use dashmap::DashMap;
use std::path::PathBuf;

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

    /// Find the owners of the specified packages
    fn owning_package(
        &self,
        paths: &AHashSet<PathBuf>,
        interner: &Interner,
    ) -> anyhow::Result<DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>>;

    /// Get the original contents of files
    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: ahash::AHashMap<PackageRef, PackageInterned>,
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

    /// Collect a map of packages with the interned name as key
    fn package_map(
        &self,
        interner: &Interner,
    ) -> anyhow::Result<ahash::AHashMap<PackageRef, PackageInterned>> {
        let packages = self
            .packages(interner)
            .with_context(|| anyhow!("Failed to load package list"))?;
        let mut package_map =
            AHashMap::with_capacity_and_hasher(packages.len(), ahash::RandomState::new());
        for package in packages.into_iter() {
            package_map.insert(package.name, package);
        }
        Ok(package_map)
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
}
