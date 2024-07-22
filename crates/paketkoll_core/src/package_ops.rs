//! Operations to check on installed packages

use ahash::AHashSet;
use anyhow::Context;

use paketkoll_types::{
    intern::{Interner, PackageRef},
    package::{Dependency, InstallReason, PackageInterned},
};

/// Get a list of all installed packages
pub fn installed_packages(
    backend: &crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
) -> anyhow::Result<(Interner, Vec<PackageInterned>)> {
    let interner = Interner::new();
    let backend_impl = backend
        .create_packages(backend_config, &interner)
        .with_context(|| format!("Failed to create backend for {backend}"))?;
    let packages = backend_impl
        .packages(&interner)
        .with_context(|| format!("Failed to collect information from backend {backend}"))?;
    Ok((interner, packages))
}

/// Get a list of all packages that are not used by any installed package
pub fn unused_packages(
    backend: &crate::backend::ConcreteBackend,
    backend_config: &crate::backend::BackendConfiguration,
) -> anyhow::Result<(Interner, Vec<PackageInterned>)> {
    let interner = Interner::new();
    let backend_impl = backend
        .create_packages(backend_config, &interner)
        .with_context(|| format!("Failed to create backend for {backend}"))?;
    let packages = backend_impl
        .packages(&interner)
        .with_context(|| format!("Failed to collect information from backend {backend}"))?;
    let (package_map, package_aliases) = paketkoll_types::backend::packages_to_split_maps(packages);

    let resolve_alias = |mut id: PackageRef| -> PackageRef {
        // Loop until we find the canonical ID
        while let Some(canonical_id) = package_aliases.get(&id) {
            id = *canonical_id;
        }
        id
    };

    let mut used_packages = ahash::AHashSet::new();
    let mut queue = Vec::new();
    for (id, package) in &package_map {
        if let Some(InstallReason::Explicit) = package.reason {
            queue.push(*id);
        }
    }
    while let Some(id) = queue.pop() {
        let id = resolve_alias(id);
        if !used_packages.insert(id) {
            continue;
        }
        let package = package_map
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Pkg {} not found", id.to_str(&interner)))?;
        for depends in &package.depends {
            match depends {
                Dependency::Single(dep) => queue.push(*dep),
                Dependency::Disjunction(alternatives) => {
                    // Find the first one that is actually installed
                    if let Some(dep) = alternatives
                        .iter()
                        .find(|dep| package_map.contains_key(&resolve_alias(**dep)))
                    {
                        queue.push(*dep);
                    }
                }
            }
        }
    }

    // Now we have the set of used packages. Subtract it from the set of all packages to find the unused ones
    let all_ids: AHashSet<PackageRef> = AHashSet::from_iter(package_map.keys().cloned());

    let mut package_map = package_map;

    let mut unused_packages = Vec::new();
    let unused_ids = all_ids.difference(&used_packages);
    for id in unused_ids {
        let package = package_map
            .remove(id)
            .expect("Internal data structure error");
        unused_packages.push(package);
    }

    Ok((interner, unused_packages))
}
