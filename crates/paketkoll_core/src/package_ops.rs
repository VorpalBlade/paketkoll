//! Operations to check on installed packages

use anyhow::Context;

use paketkoll_types::{intern::Interner, package::PackageInterned};

/// Get a list of all installed packages
pub fn installed_packages(
    backend: &crate::backend::Backend,
    backend_config: &crate::backend::BackendConfiguration,
) -> anyhow::Result<(Interner, Vec<PackageInterned>)> {
    let backend_impl = backend
        .create_packages(backend_config)
        .with_context(|| format!("Failed to create backend for {backend}"))?;
    let interner = Interner::new();
    let packages = backend_impl
        .packages(&interner)
        .with_context(|| format!("Failed to collect information from backend {backend}"))?;
    Ok((interner, packages))
}
