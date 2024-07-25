//! Package scanning functions

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Context;
use itertools::Itertools;
use rayon::prelude::*;

use konfigkoll_types::PkgInstructions;
use paketkoll_types::{
    backend::{Backend, PackageBackendMap, PackageMap, PackageMapMap},
    intern::Interner,
};

#[tracing::instrument(skip_all)]
pub(crate) fn load_packages(
    interner: &Arc<Interner>,
    backends_pkg: &PackageBackendMap,
) -> anyhow::Result<(PkgInstructions, PackageMapMap)> {
    let mut pkgs_sys = BTreeMap::new();
    let mut package_maps: BTreeMap<Backend, Arc<PackageMap>> = BTreeMap::new();
    let backend_maps: Vec<_> = backends_pkg
        .values()
        .par_bridge()
        .map(|backend| {
            let backend_pkgs = backend
                .packages(interner)
                .with_context(|| {
                    format!(
                        "Failed to collect information from backend {}",
                        backend.name()
                    )
                })
                .map(|backend_pkgs| {
                    let pkg_map = Arc::new(paketkoll_types::backend::packages_to_package_map(
                        backend_pkgs.clone(),
                    ));
                    let pkg_instructions =
                        konfigkoll_core::conversion::convert_packages_to_pkg_instructions(
                            backend_pkgs.into_iter(),
                            backend.as_backend_enum(),
                            interner,
                        );
                    (pkg_map, pkg_instructions)
                });
            (backend, backend_pkgs)
        })
        .collect();
    for (backend, backend_pkgs) in backend_maps.into_iter() {
        let (backend_pkgs_map, pkg_instructions) = backend_pkgs?;
        package_maps.insert(backend.as_backend_enum(), backend_pkgs_map);
        pkgs_sys.extend(pkg_instructions.into_iter());
    }

    Ok((pkgs_sys, package_maps))
}

type PackagePair<'a> = (
    &'a konfigkoll_types::PkgIdent,
    &'a konfigkoll_types::PkgInstruction,
);

/// Get a diff of packages
pub(crate) fn package_diff<'input>(
    sorted_pkgs_sys: &'input PkgInstructions,
    script_engine: &'input konfigkoll_script::ScriptEngine,
) -> impl Iterator<Item = itertools::EitherOrBoth<PackagePair<'input>, PackagePair<'input>>> {
    let pkg_actions = &script_engine.state().commands().package_actions;
    let left = sorted_pkgs_sys.iter();
    let right = pkg_actions.iter().sorted();

    konfigkoll_core::diff::comm(left, right)
}
