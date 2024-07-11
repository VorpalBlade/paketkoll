//! The various backends implementing distro specific support

use ahash::{AHashMap, AHashSet};
use anyhow::{anyhow, Context};
use compact_str::CompactString;
use dashmap::DashMap;
use paketkoll_types::{
    files::FileEntry,
    intern::{Interner, PackageRef},
    package::PackageInterned,
};
use std::{fmt::Debug, path::PathBuf};

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

pub(crate) mod filesystem;
pub(crate) mod flatpak;

#[cfg(feature = "systemd_tmpfiles")]
pub(crate) mod systemd_tmpfiles;

/// Get the name of a backend (useful in dynamic dispatch for generating reports)
pub trait Name: Send + Sync {
    /// The name of the backend (for logging and debugging purposes)
    fn name(&self) -> &'static str;

    /// The backend enum value corresponding to this backend
    fn as_backend_enum(&self) -> paketkoll_types::Backend;
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
}

/// A package manager backend (installing/uninstalling packages)
pub trait PackageManager: Name {
    /// Perform installation and uninstallation of a bunch of packages
    ///
    /// The package name format depends on the backend.
    fn transact(
        &self,
        install: &[CompactString],
        uninstall: &[CompactString],
        ask_confirmation: bool,
    ) -> anyhow::Result<()>;
}

/// A backend that implements all operations
#[allow(dead_code)]
pub trait FullBackend: Files + Packages + PackageManager {}

/// Which backend to use for the system package manager
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, strum::Display)]
pub enum ConcreteBackend {
    /// Backend for Arch Linux and derived distros (pacman)
    #[cfg(feature = "arch_linux")]
    #[strum(to_string = "pacman")]
    Pacman,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[cfg(feature = "debian")]
    #[strum(to_string = "apt")]
    Apt,
    /// Backend for flatpak (package list only)
    #[strum(to_string = "flatpak")]
    Flatpak,
    /// Backend for systemd-tmpfiles (file list only)
    #[cfg(feature = "systemd_tmpfiles")]
    #[strum(to_string = "systemd-tmpfiles")]
    SystemdTmpfiles,
}

impl TryFrom<paketkoll_types::Backend> for ConcreteBackend {
    type Error = anyhow::Error;

    fn try_from(value: paketkoll_types::Backend) -> Result<Self, Self::Error> {
        match value {
            #[cfg(feature = "arch_linux")]
            paketkoll_types::Backend::Pacman => Ok(Self::Pacman),
            #[cfg(feature = "debian")]
            paketkoll_types::Backend::Apt => Ok(Self::Apt),
            paketkoll_types::Backend::Flatpak => Ok(Self::Flatpak),
            #[cfg(feature = "systemd_tmpfiles")]
            paketkoll_types::Backend::SystemdTmpfiles => Ok(Self::SystemdTmpfiles),
            #[allow(unreachable_patterns)]
            _ => anyhow::bail!("Unsupported backend in current build: {:?}", value),
        }
    }
}

impl From<ConcreteBackend> for paketkoll_types::Backend {
    fn from(value: ConcreteBackend) -> Self {
        match value {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => paketkoll_types::Backend::Pacman,
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => paketkoll_types::Backend::Apt,
            ConcreteBackend::Flatpak => paketkoll_types::Backend::Flatpak,
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => paketkoll_types::Backend::SystemdTmpfiles,
        }
    }
}

// Clippy is wrong, this cannot be derived due to the cfg_if
#[allow(clippy::derivable_impls)]
impl Default for ConcreteBackend {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "arch_linux")] {
                ConcreteBackend::Pacman
            } else if #[cfg(feature = "debian")] {
                ConcreteBackend::Apt
            } else {
                ConcreteBackend::Flatpak
            }
        }
    }
}

impl ConcreteBackend {
    /// Create a backend instance
    pub fn create_files(
        self,
        configuration: &BackendConfiguration,
    ) -> anyhow::Result<Box<dyn Files>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            ConcreteBackend::Flatpak => Err(anyhow::anyhow!(
                "Flatpak backend does not support file checks"
            )),
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => Ok(Box::new({
                let builder = crate::backend::systemd_tmpfiles::SystemdTmpfilesBuilder::default();
                builder.build()
            })),
        }
    }

    /// Create a backend instance
    pub fn create_packages(
        self,
        configuration: &BackendConfiguration,
    ) -> anyhow::Result<Box<dyn Packages>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            ConcreteBackend::Flatpak => Ok(Box::new({
                let builder = crate::backend::flatpak::FlatpakBuilder::default();
                builder.build()
            })),
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => Err(anyhow::anyhow!(
                "SystemdTmpfiles backend does not support package checks"
            )),
        }
    }

    /// Create a full backend implementation
    pub fn create_full(
        self,
        configuration: &BackendConfiguration,
    ) -> anyhow::Result<Box<dyn FullBackend>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            ConcreteBackend::Flatpak => Err(anyhow::anyhow!(
                "Flatpak backend does not support file checks"
            )),
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => Err(anyhow::anyhow!(
                "SystemdTmpfiles backend does not support package checks"
            )),
        }
    }
}

/// Describes how to build a backend
#[derive(Debug, Clone, derive_builder::Builder)]
#[non_exhaustive]
pub struct BackendConfiguration {
    /// Which packages to include
    #[builder(default = "&PackageFilter::Everything")]
    pub package_filter: &'static PackageFilter,
}

impl BackendConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> BackendConfigurationBuilder {
        Default::default()
    }
}

impl Default for BackendConfiguration {
    fn default() -> Self {
        Self {
            package_filter: &PackageFilter::Everything,
        }
    }
}

/// Action to perform according to filter
#[derive(Debug)]
pub enum FilterAction {
    Exclude,
    Include,
}

/// A filter for which packages to load data for
pub enum PackageFilter {
    Everything,
    // Given a package name (without version), decide if we should process it
    FilterFunction(Box<dyn Fn(&str) -> FilterAction + Sync + Send>),
}

impl PackageFilter {
    /// Should we include this package?
    ///
    /// We do de-interning here, since the fast path is to just include everything.
    pub(crate) fn should_include_interned(&self, package: PackageRef, interner: &Interner) -> bool {
        match self {
            PackageFilter::Everything => true,
            PackageFilter::FilterFunction(f) => match f(package.to_str(interner)) {
                FilterAction::Include => true,
                FilterAction::Exclude => false,
            },
        }
    }
}

impl Debug for PackageFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageFilter::Everything => write!(f, "Everything"),
            PackageFilter::FilterFunction(_) => write!(f, "FilterFunction(...)"),
        }
    }
}
