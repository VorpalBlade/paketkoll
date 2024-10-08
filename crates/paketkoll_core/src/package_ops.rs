//! Operations to check on installed packages

use eyre::WrapErr;
use paketkoll_types::intern::Interner;
use paketkoll_types::package::PackageInterned;

/// Get a list of all installed packages
pub fn installed_packages(
    backend: crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
) -> eyre::Result<(Interner, Vec<PackageInterned>)> {
    let interner = Interner::new();
    let backend_impl = backend
        .create_packages(backend_config, &interner)
        .wrap_err_with(|| format!("Failed to create backend for {backend}"))?;
    let packages = backend_impl
        .packages(&interner)
        .wrap_err_with(|| format!("Failed to collect information from backend {backend}"))?;
    Ok((interner, packages))
}
