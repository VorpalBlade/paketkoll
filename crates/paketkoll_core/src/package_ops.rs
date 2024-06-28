//! Operations to check on installed packages

use anyhow::Context;

use paketkoll_types::intern::Interner;

/// Get a list of all installed packages
pub fn installed_packages(
    config: &crate::config::PackageListConfiguration,
) -> anyhow::Result<(Interner, Vec<crate::types::PackageInterned>)> {
    let backend = config
        .common
        .backend
        .create_packages(&config.common)
        .with_context(|| format!("Failed to create backend for {}", config.common.backend))?;
    let interner = Interner::new();
    let packages = backend.packages(&interner).with_context(|| {
        format!(
            "Failed to collect information from backend {}",
            config.common.backend
        )
    })?;
    Ok((interner, packages))
}
