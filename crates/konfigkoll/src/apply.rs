//! Code for applying the configuration to the system.

use std::sync::Arc;

use either::Either;
use konfigkoll::cli::Paranoia;
use konfigkoll_core::apply::Applicator;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::intern::Interner;

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_applicator(
    confirmation: Paranoia,
    force_dry_run: bool,
    backend_map: &PackageBackendMap,
    interner: &Arc<Interner>,
    package_maps: &PackageMapMap,
    files_backend: &Arc<dyn Files>,
    diff_command: Vec<String>,
    pager_command: Vec<String>,
) -> Box<dyn Applicator> {
    // TODO: This is where privilege separation would happen (well, one of the
    // locations)
    let inner_applicator = if force_dry_run {
        Either::Left(konfigkoll_core::apply::NoopApplicator::default())
    } else {
        Either::Right(konfigkoll_core::apply::InProcessApplicator::new(
            backend_map.clone(),
            interner,
            package_maps,
            files_backend,
        ))
    };
    // Create applicator based on paranoia setting
    let applicator: Box<dyn Applicator> = match confirmation {
        Paranoia::Yolo => Box::new(inner_applicator),
        Paranoia::Ask => Box::new(konfigkoll_core::apply::InteractiveApplicator::new(
            inner_applicator,
            diff_command,
            pager_command,
        )),
        Paranoia::DryRun => Box::new(konfigkoll_core::apply::NoopApplicator::default()),
    };
    applicator
}
