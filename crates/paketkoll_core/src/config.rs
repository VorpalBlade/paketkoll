//! Configuration of backend checks

use crate::types::PackageRef;
use paketkoll_types::intern::Interner;
use std::fmt::Debug;

/// Which backend to use for the system package manager
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, strum::Display)]
pub enum Backend {
    /// Backend for Arch Linux and derived distros (pacman)
    #[cfg(feature = "arch_linux")]
    ArchLinux,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[cfg(feature = "debian")]
    Debian,
    /// Backend for flatpak (package list only)
    Flatpak,
    /// Backend for systemd-tmpfiles (file list only)
    #[cfg(feature = "systemd_tmpfiles")]
    SystemdTmpfiles,
}

// Clippy is wrong, this cannot be derived due to the cfg_if
#[allow(clippy::derivable_impls)]
impl Default for Backend {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "arch_linux")] {
                Backend::ArchLinux
            } else if #[cfg(feature = "debian")] {
                Backend::Debian
            }
        }
    }
}

impl Backend {
    /// Create a backend instance
    pub(crate) fn create_files(
        self,
        configuration: &CommonConfiguration,
    ) -> anyhow::Result<Box<dyn crate::backend::Files>> {
        match self {
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            Backend::Flatpak => Err(anyhow::anyhow!(
                "Flatpak backend does not support file checks"
            )),
            #[cfg(feature = "systemd_tmpfiles")]
            Backend::SystemdTmpfiles => Ok(Box::new({
                let builder = crate::backend::systemd_tmpfiles::SystemdTmpfilesBuilder::default();
                builder.build()
            })),
        }
    }

    /// Create a backend instance
    pub(crate) fn create_packages(
        self,
        configuration: &CommonConfiguration,
    ) -> anyhow::Result<Box<dyn crate::backend::Packages>> {
        match self {
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            Backend::Flatpak => Ok(Box::new({
                let builder = crate::backend::flatpak::FlatpakBuilder::default();
                builder.build()
            })),
            #[cfg(feature = "systemd_tmpfiles")]
            Backend::SystemdTmpfiles => Err(anyhow::anyhow!(
                "SystemdTmpfiles backend does not support package checks"
            )),
        }
    }

    pub(crate) fn create_full(
        self,
        configuration: &CommonConfiguration,
    ) -> anyhow::Result<Box<dyn crate::backend::FullBackend>> {
        match self {
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Box::new({
                let mut builder = crate::backend::arch::ArchLinuxBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()?
            })),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Box::new({
                let mut builder = crate::backend::deb::DebianBuilder::default();
                builder.package_filter(configuration.package_filter);
                builder.build()
            })),
            Backend::Flatpak => Err(anyhow::anyhow!(
                "Flatpak backend does not support file checks"
            )),
            #[cfg(feature = "systemd_tmpfiles")]
            Backend::SystemdTmpfiles => Err(anyhow::anyhow!(
                "SystemdTmpfiles backend does not support package checks"
            )),
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

/// Describes shared configuration for all operations
#[derive(Debug, Clone, derive_builder::Builder)]
#[non_exhaustive]
pub struct CommonConfiguration {
    /// Distro backend to use
    pub backend: Backend,
    /// Which packages to include
    #[builder(default = "&PackageFilter::Everything")]
    pub package_filter: &'static PackageFilter,
}

impl CommonConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> CommonConfigurationBuilder {
        Default::default()
    }
}

impl Default for CommonConfiguration {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            package_filter: &PackageFilter::Everything,
        }
    }
}

/// Configuration for [`crate::file_ops::check_all_files`]
#[derive(Debug, derive_builder::Builder)]
#[non_exhaustive]
pub struct CheckAllFilesConfiguration {
    /// Ignored paths (globs). Only appliccable to some operations.
    #[builder(default = "vec![]")]
    pub ignored_paths: Vec<String>,
    /// Should paths be canonicalized before checking? (This is needed on Debian
    /// for example)
    #[builder(default = "false")]
    pub canonicalize_paths: bool,
}

impl CheckAllFilesConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> CheckAllFilesConfigurationBuilder {
        Default::default()
    }
}

/// Describes what we want to check. Not all backends may support all features,
/// in which case an error should be returned.
#[derive(Debug, derive_builder::Builder)]
#[non_exhaustive]
pub struct CommonFileCheckConfiguration {
    /// Common configuration
    pub common: CommonConfiguration,
    /// Should we trust modification time and skip timestamp if mtime matches?
    #[builder(default = "false")]
    pub trust_mtime: bool,
    /// Should configuration files be included
    #[builder(default = "ConfigFiles::Include")]
    pub config_files: ConfigFiles,
}

impl CommonFileCheckConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> CommonFileCheckConfigurationBuilder {
        Default::default()
    }
}

/// Describe how to check config files
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ConfigFiles {
    /// Include config files in check
    Include,
    /// Exclude config files in check
    Exclude,
    /// Only check config files
    Only,
}

/// Describes how to list packages
#[derive(Debug, derive_builder::Builder)]
#[non_exhaustive]
pub struct PackageListConfiguration {
    /// Common configuration
    pub common: CommonConfiguration,
}

impl PackageListConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> PackageListConfigurationBuilder {
        Default::default()
    }
}

/// Describes how to list original files
#[derive(Debug, derive_builder::Builder)]
#[non_exhaustive]
pub struct OriginalFilesConfiguration {
    /// Common configuration
    pub common: CommonConfiguration,
}

impl OriginalFilesConfiguration {
    /// Get a builder for this struct
    pub fn builder() -> OriginalFilesConfigurationBuilder {
        Default::default()
    }
}
