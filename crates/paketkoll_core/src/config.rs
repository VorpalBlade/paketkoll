//! Configuration of backend checks

use typed_builder::TypedBuilder;

/// Which backend to use for the system package manager
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, strum::Display)]
pub enum Backend {
    /// Backend for Arch Linux and derived distros (pacman)
    #[cfg(feature = "arch_linux")]
    ArchLinux,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[cfg(feature = "debian")]
    Debian,
}

impl Backend {
    /// Create a backend instance
    pub(crate) fn create(self) -> anyhow::Result<Box<dyn crate::backend::Files>> {
        match self {
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Box::new(
                crate::backend::arch::ArchLinuxBuilder::default().build()?,
            )),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Box::new(
                crate::backend::deb::DebianBuilder::default().build(),
            )),
        }
    }
}

/// Describes what we want to check. Not all backends may support all features,
/// in which case an error should be returned.
#[derive(Debug, Clone, TypedBuilder)]
#[non_exhaustive]
pub struct CheckConfiguration {
    /// Distro backend to use
    pub backend: Backend,
    /// Should we trust modification time and skip timestamp if mtime matches?
    #[builder(default = false)]
    pub trust_mtime: bool,
    /// Should configuration files be included
    #[builder(default = ConfigFiles::Include)]
    pub config_files: ConfigFiles,
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
