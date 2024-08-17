//! Package scanning functions

use eyre::WrapErr;
use itertools::Itertools;
use konfigkoll_types::PkgInstructions;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::intern::Interner;
use paketkoll_types::package::PackageInstallStatus;
use rayon::prelude::*;
use std::sync::Arc;

#[tracing::instrument(skip_all)]
pub(crate) fn load_packages(
    interner: &Arc<Interner>,
    backends_pkg: &PackageBackendMap,
) -> eyre::Result<(PkgInstructions, PackageMapMap)> {
    let mut pkgs_sys = PkgInstructions::new();
    let mut package_maps: PackageMapMap = PackageMapMap::new();
    let backend_maps: Vec<_> = backends_pkg
        .values()
        .par_bridge()
        .map(|backend| {
            let backend_pkgs = backend
                .packages(interner)
                .wrap_err_with(|| {
                    format!(
                        "Failed to collect information from backend {}",
                        backend.name()
                    )
                })
                .map(|mut backend_pkgs| {
                    // Because we can have partially installed packages on Debian...
                    backend_pkgs.retain(|pkg| pkg.status == PackageInstallStatus::Installed);
                    let pkg_map = Arc::new(paketkoll_types::backend::packages_to_package_map(
                        backend_pkgs.iter(),
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
