//! The various backends implementing distro specific support

use std::fmt::Debug;

use paketkoll_types::backend::{Files, Packages};
use paketkoll_types::intern::{Interner, PackageRef};

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

#[cfg(feature = "systemd_tmpfiles")]
pub(crate) mod systemd_tmpfiles;

pub(crate) mod filesystem;
pub(crate) mod flatpak;

/// A backend that implements all operations
#[allow(dead_code)]
pub trait FullBackend: Files + Packages {}

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

impl TryFrom<paketkoll_types::backend::Backend> for ConcreteBackend {
    type Error = anyhow::Error;

    fn try_from(value: paketkoll_types::backend::Backend) -> Result<Self, Self::Error> {
        match value {
            #[cfg(feature = "arch_linux")]
            paketkoll_types::backend::Backend::Pacman => Ok(Self::Pacman),
            #[cfg(feature = "debian")]
            paketkoll_types::backend::Backend::Apt => Ok(Self::Apt),
            paketkoll_types::backend::Backend::Flatpak => Ok(Self::Flatpak),
            #[cfg(feature = "systemd_tmpfiles")]
            paketkoll_types::backend::Backend::SystemdTmpfiles => Ok(Self::SystemdTmpfiles),
            #[allow(unreachable_patterns)]
            _ => anyhow::bail!("Unsupported backend in current build: {:?}", value),
        }
    }
}

impl From<ConcreteBackend> for paketkoll_types::backend::Backend {
    fn from(value: ConcreteBackend) -> Self {
        match value {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => paketkoll_types::backend::Backend::Pacman,
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => paketkoll_types::backend::Backend::Apt,
            ConcreteBackend::Flatpak => paketkoll_types::backend::Backend::Flatpak,
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => paketkoll_types::backend::Backend::SystemdTmpfiles,
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
        interner: &Interner,
    ) -> anyhow::Result<Box<dyn Files>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build(interner)
            })),
            ConcreteBackend::Flatpak => Err(anyhow::anyhow!(
                "Flatpak backend does not support file checks"
            )),
            #[cfg(feature = "systemd_tmpfiles")]
            ConcreteBackend::SystemdTmpfiles => Ok(Box::new({
                let builder = systemd_tmpfiles::SystemdTmpfilesBuilder::default();
                builder.build()
            })),
        }
    }

    /// Create a backend instance
    pub fn create_packages(
        self,
        configuration: &BackendConfiguration,
        interner: &Interner,
    ) -> anyhow::Result<Box<dyn Packages>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build(interner)
            })),
            ConcreteBackend::Flatpak => Ok(Box::new({
                let builder = flatpak::FlatpakBuilder::default();
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
        interner: &Interner,
    ) -> anyhow::Result<Box<dyn FullBackend>> {
        match self {
            #[cfg(feature = "arch_linux")]
            ConcreteBackend::Pacman => Ok(Box::new({
                let mut builder = arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            ConcreteBackend::Apt => Ok(Box::new({
                let mut builder = deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build(interner)
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
