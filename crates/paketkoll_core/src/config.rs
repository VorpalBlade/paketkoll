//! Configuration for [`crate::file_ops`] and [`crate::package_ops`]

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
