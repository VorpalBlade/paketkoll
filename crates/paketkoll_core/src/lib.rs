//! # `paketkoll_core` - Core functionality for paketkoll
//!
//! This API is very unstable at the moment. Don't depend on this package (yet).
//! For now all it provides is a way to check distro installed files for differences.
//! The plan is detailed in the README.md in the crate directory in the repository.

#[cfg(not(any(feature = "arch_linux", feature = "debian")))]
compile_error!("At least one backend must be enabled");

pub(crate) mod backend;
pub mod config;
pub mod file_ops;
pub mod package_ops;
pub mod types;

/// Vendored dependency due to upstream being slow to accept PRs
///
/// We also need to allow dead code, since we don't use all functions from it.
#[allow(dead_code)]
mod mtree;
