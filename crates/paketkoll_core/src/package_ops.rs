//! Operations to check on installed packages

use anyhow::Context;

use crate::types::Interner;

pub fn installed_packages(
    config: &crate::config::PackageListConfiguration,
) -> anyhow::Result<(crate::types::Interner, Vec<crate::types::Package>)> {
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
