//! The various backends implementing distro specific support

use compact_str::CompactString;
use paketkoll_types::intern::{Interner, PackageRef};

use crate::types::PackageInterned;

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

pub(crate) mod filesystem;
pub(crate) mod flatpak;

#[cfg(feature = "systemd_tmpfiles")]
pub(crate) mod systemd_tmpfiles;

/// Get the name of a backend (useful in dynamic dispatch for generating reports)
pub(crate) trait Name: Send + Sync {
    /// The name of the backend (for logging and debugging purposes)
    // Temporary, this will get exposed
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
}

/// A package manager backend
pub(crate) trait Files: Name {
    /// Collect a list of files managed by the package manager including
    /// any available metadata such as checksums or timestamps about those files
    fn files(&self, interner: &Interner) -> anyhow::Result<Vec<crate::types::FileEntry>>;

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
pub(crate) trait Packages: Name {
    /// Collect a list of all installed packages
    fn packages(&self, interner: &Interner) -> anyhow::Result<Vec<crate::types::PackageInterned>>;
}

/// A package manager backend (installing/uninstalling packages)
pub(crate) trait PackageManager: Name {
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
pub(crate) trait FullBackend: Files + Packages {
    /// Collect all data from the backend in one go.
    ///
    /// This can be possibly be more efficient for some backends as some work can be shared.
    fn data(
        &self,
        interner: &paketkoll_types::intern::Interner,
    ) -> anyhow::Result<crate::types::BackendData> {
        let results = rayon::join(|| self.packages(interner), || self.files(interner));
        let results = (results.0?, results.1?);
        Ok(crate::types::BackendData {
            packages: results.0,
            files: results.1,
        })
    }
}
