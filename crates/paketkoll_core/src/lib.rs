//! # `paketkoll_core` - Core functionality for paketkoll
//!
//! This API is very unstable at the moment. Don't depend on this package (yet).
//! For now all it provides is a way to check distro installed files for differences.
//! The plan is detailed in the README.md in the crate directory in the repository.

#[cfg(not(any(feature = "arch_linux", feature = "debian")))]
compile_error!("At least one backend must be enabled");

pub mod backend;
pub mod config;
pub mod file_ops;
pub mod package_ops;
pub(crate) mod utils;

/// Re-export for downstream to get the correct version
pub use paketkoll_types;
