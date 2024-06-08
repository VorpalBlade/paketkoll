//! The various backends implementing distro specific support

#[cfg(feature = "arch_linux")]
pub(crate) mod arch;

#[cfg(feature = "debian")]
pub(crate) mod deb;

pub(crate) mod flatpak;

pub(crate) mod filesystem;

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
    fn files(
        &self,
        interner: &crate::types::Interner,
    ) -> anyhow::Result<Vec<crate::types::FileEntry>>;
}

/// A package manager backend
pub(crate) trait Packages: Name {
    /// Collect a list of all installed packages
    fn packages(
        &self,
        interner: &crate::types::Interner,
    ) -> anyhow::Result<Vec<crate::types::Package>>;
}

// TODO: Operations to add
// - Get source file from package (possibly downloading the package to cache if needed)
// - Does a paccache equivalent exist for Debian or do we need to implement smart cache
//   cleaning as a separate tool?

#[allow(dead_code)]
pub(crate) trait FullBackend: Files + Packages {
    /// Collect all data from the backend in one go.
    ///
    /// This can be possibly be more efficient for some backends as some work can be shared.
    fn data(&self, interner: &crate::types::Interner) -> anyhow::Result<crate::types::BackendData> {
        let results = rayon::join(|| self.packages(interner), || self.files(interner));
        let results = (results.0?, results.1?);
        Ok(crate::types::BackendData {
            packages: results.0,
            files: results.1,
        })
    }
}
