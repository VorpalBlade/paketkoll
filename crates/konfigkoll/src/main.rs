use crate::fs_scan::ScanResult;
use apply::create_applicator;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;
use compact_str::CompactString;
use eyre::OptionExt;
use eyre::WrapErr;
use itertools::Itertools;
use konfigkoll::cli::Cli;
use konfigkoll::cli::Commands;
use konfigkoll::cli::Paranoia;
use konfigkoll_core::apply::apply_files;
use konfigkoll_core::apply::apply_packages;
use konfigkoll_core::diff::show_fs_instr_diff;
use konfigkoll_core::state::DiffGoal;
use konfigkoll_core::state::FsEntries;
use konfigkoll_script::Phase;
use konfigkoll_script::ScriptEngine;
use konfigkoll_types::FsInstruction;
use konfigkoll_types::PkgIdent;
use konfigkoll_types::PkgInstruction;
use paketkoll_cache::FromArchiveCache;
use paketkoll_cache::OriginalFilesCache;
use paketkoll_core::backend::ConcreteBackend;
use paketkoll_core::paketkoll_types::intern::Interner;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::backend::Packages;
use std::io::BufWriter;
use std::io::Write;
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod apply;
mod fs_scan;
mod init;
mod pkgs;
mod save;

#[cfg(target_env = "musl")]
mod _musl {
    use mimalloc::MiMalloc;
    #[global_allocator]
    static GLOBAL: MiMalloc = MiMalloc;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    // Set up logging with tracing
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .with(tracing_error::ErrorLayer::default())
        .init();

    let cli = Cli::parse();

    let config_path = match cli.config_path {
        Some(v) => v,
        None => std::env::current_dir()?.try_into()?,
    };

    // This must be done before we instantiate the script engine
    if let Commands::Init {} = cli.command {
        init::init_directory(&config_path)?;
        return Ok(());
    }

    let mut script_engine = konfigkoll_script::ScriptEngine::new_with_files(&config_path)?;

    match cli.command {
        Commands::Init {} | Commands::Save { .. } | Commands::Apply {} | Commands::Diff { .. } => {}
        Commands::Check {} => {
            println!("Scripts loaded successfully");
            return Ok(());
        }
    }

    // Script: Do system discovery and configuration
    script_engine.run_phase(Phase::SystemDiscovery).await?;

    let proj_dirs = directories::ProjectDirs::from("", "", "konfigkoll")
        .ok_or_eyre("Failed to get directories for disk cache")?;

    // Create backends
    tracing::info!("Creating backends");
    let interner = Arc::new(Interner::new());
    let file_backend_id = script_engine
        .state()
        .settings()
        .file_backend()
        .ok_or_else(|| eyre::eyre!("A file backend must be set"))?;
    let pkg_backend_ids = script_engine
        .state()
        .settings()
        .enabled_pkg_backends()
        .collect_vec();
    let backend_cfg = paketkoll_core::backend::BackendConfiguration::builder()
        .build()
        .wrap_err("Failed to build backend config")?;
    let backends_pkg: Arc<PackageBackendMap> = Arc::new(
        pkg_backend_ids
            .iter()
            .map(|b| {
                let b: ConcreteBackend = (*b)
                    .try_into()
                    .wrap_err("Backend is not supported by current build")?;
                let backend = b
                    .create_packages(&backend_cfg, &interner)
                    .wrap_err_with(|| format!("Failed to create backend {b}"))?;
                let b: Arc<dyn Packages> = Arc::from(backend);
                Ok(b)
            })
            .map(|b| b.map(|b| (b.as_backend_enum(), b)))
            .collect::<eyre::Result<_>>()?,
    );

    let backend_files: Arc<dyn Files> = {
        let b: ConcreteBackend = file_backend_id
            .try_into()
            .wrap_err("Backend is not supported by current build")?;
        let backend = b
            .create_files(&backend_cfg, &interner)
            .wrap_err_with(|| format!("Failed to create backend {b}"))?;
        let backend = if backend.prefer_files_from_archive() {
            tracing::info!("Using archive cache for backend {}", backend.name());
            // This is slow so we need to cache it
            Box::new(
                FromArchiveCache::from_path(backend, proj_dirs.cache_dir())
                    .wrap_err("Failed to create archive disk cache")?,
            )
        } else {
            // This is fast so we don't need to cache it
            backend
        };
        let backend = OriginalFilesCache::from_path(backend, proj_dirs.cache_dir())
            .wrap_err("Failed to create original files disk cache")?;
        Arc::new(backend)
    };

    // Load installed packages
    tracing::info!("Starting package loading background job");
    let package_loader = {
        let interner = interner.clone();
        let backends_pkg = backends_pkg.clone();
        tokio::task::spawn_blocking(move || pkgs::load_packages(&interner, &backends_pkg))
    };
    // Script: Get FS ignores
    script_engine.run_phase(Phase::Ignores).await?;

    tracing::info!("Waiting for package loading results...");
    let (pkgs_sys, package_maps) = package_loader.await??;
    tracing::info!("Got package loading results");

    // Do FS scan
    tracing::info!("Starting filesystem scan background job");
    let fs_instructions_sys = {
        let ignores: Vec<CompactString> = script_engine
            .state()
            .commands()
            .fs_ignores
            .iter()
            .cloned()
            .collect();
        let trust_mtime = cli.trust_mtime;
        let interner = interner.clone();
        let backends_files = backend_files.clone();
        let package_map = package_maps
            .get(&backend_files.as_backend_enum())
            .expect("No matching package backend?")
            .clone();
        tokio::task::spawn_blocking(move || {
            fs_scan::scan_fs(
                &interner,
                &backends_files,
                &package_map,
                &ignores,
                trust_mtime,
            )
        })
    };

    // Script: Do early package phase
    script_engine.run_phase(Phase::ScriptDependencies).await?;

    // Create the set of package managers for use by the script
    script_engine.state_mut().setup_package_managers(
        &backends_pkg,
        file_backend_id,
        &backend_files,
        &package_maps,
        &interner,
    );

    // Apply early packages (if any)
    if let Commands::Apply {} = cli.command {
        tracing::info!("Applying early packages (if any are missing)");
        let mut applicator = create_applicator(
            cli.confirmation,
            cli.debug_force_dry_run,
            &backends_pkg,
            &interner,
            &package_maps,
            &backend_files,
            script_engine.state().settings().diff(),
            script_engine.state().settings().pager(),
        );
        let pkg_diff = pkgs::package_diff(&pkgs_sys, &script_engine);
        let pkgs_changes = pkg_diff.filter_map(|v| match v {
            itertools::EitherOrBoth::Both(_, _) => None,
            itertools::EitherOrBoth::Left(_) => None,
            itertools::EitherOrBoth::Right((id, instr)) => Some((id, instr.clone())),
        });
        apply_packages(applicator.as_mut(), pkgs_changes, &package_maps, &interner)?;
    }

    // Script: Do main phase
    script_engine.run_phase(Phase::Main).await?;

    // Make sure FS actions are sorted
    script_engine.state_mut().commands_mut().fs_actions.sort();

    tracing::info!("Waiting for file system scan results...");
    let (fs_scan_result, fs_instructions_sys) = fs_instructions_sys.await??;
    tracing::info!("Got file system scan results");

    // Compare expected to system
    let mut script_fs = konfigkoll_core::state::FsEntries::default();
    let mut sys_fs = konfigkoll_core::state::FsEntries::default();
    let fs_actions = std::mem::take(&mut script_engine.state_mut().commands_mut().fs_actions);
    script_fs.apply_instructions(fs_actions.into_iter(), true);
    sys_fs.apply_instructions(fs_instructions_sys.into_iter(), false);

    // Packages are so much easier
    let pkg_diff = pkgs::package_diff(&pkgs_sys, &script_engine);

    // At the end, decide what we want to do with the results
    match cli.command {
        Commands::Save { filter } => {
            tracing::debug!("Computing changes to save");
            // Split out additions and removals
            let fs_changes = fs_state_diff_save(script_fs, sys_fs)?;

            let mut pkg_additions = vec![];
            let mut pkg_removals = vec![];
            pkg_diff.for_each(|v| match v {
                itertools::EitherOrBoth::Both(_, _) => (),
                itertools::EitherOrBoth::Left((id, instr)) => {
                    pkg_additions.push((id, instr.clone()));
                }
                itertools::EitherOrBoth::Right((id, instr)) => {
                    pkg_removals.push((id, instr.inverted()));
                }
            });

            cmd_save_changes(
                cli.confirmation,
                &config_path,
                &script_engine,
                &filter,
                &fs_changes,
                pkg_additions,
                pkg_removals,
                &interner,
            )?;
        }
        Commands::Apply {} => {
            tracing::debug!("Computing changes to apply");
            let fs_changes =
                fs_state_diff_apply(&backend_files, &fs_scan_result, script_fs, sys_fs)?;

            let pkgs_changes = pkg_diff
                .filter_map(|v| match v {
                    itertools::EitherOrBoth::Both(_, _) => None,
                    itertools::EitherOrBoth::Left((id, instr)) => Some((id, instr.inverted())),
                    itertools::EitherOrBoth::Right((id, instr)) => Some((id, instr.clone())),
                })
                .collect_vec();

            cmd_apply_changes(
                cli.confirmation,
                cli.debug_force_dry_run,
                &script_engine,
                &interner,
                &backends_pkg,
                &backend_files,
                &package_maps,
                fs_changes,
                pkgs_changes,
            )?;
        }
        Commands::Diff { path } => {
            tracing::info!("Computing diff");
            let fs_changes =
                fs_state_diff_apply(&backend_files, &fs_scan_result, script_fs, sys_fs)?;

            let diff_cmd = script_engine.state().settings().diff();
            let pager_cmd = script_engine.state().settings().pager();
            for change in fs_changes {
                if change.path.starts_with(&path) {
                    show_fs_instr_diff(&change, &diff_cmd, &pager_cmd)?;
                }
            }
            // Let the OS clean these up, freeing in the program is slower
            std::mem::forget(pkg_diff);
        }
        Commands::Check {} | Commands::Init {} => unreachable!(),
    }

    // Let the OS clean these up, freeing in the program is slower (~35 ms on Intel
    // Skylake)
    std::mem::forget(backend_files);
    std::mem::forget(backends_pkg);
    std::mem::forget(fs_scan_result);
    std::mem::forget(interner);
    std::mem::forget(package_maps);
    std::mem::forget(pkgs_sys);
    std::mem::forget(proj_dirs);
    std::mem::forget(script_engine);

    Ok(())
}

/// Implements the actual saving for the `save` command
#[allow(clippy::too_many_arguments)]
fn cmd_save_changes(
    confirmation: Paranoia,
    config_path: &Utf8Path,
    script_engine: &ScriptEngine,
    filter: &Option<Utf8PathBuf>,
    fs_changes: &[FsInstruction],
    pkg_additions: Vec<(&PkgIdent, PkgInstruction)>,
    pkg_removals: Vec<(&PkgIdent, PkgInstruction)>,
    interner: &Interner,
) -> eyre::Result<()> {
    if !fs_changes.is_empty() || !pkg_additions.is_empty() || !pkg_removals.is_empty() {
        tracing::warn!("There are differences (saving to unsorted.rn)");
    } else {
        tracing::info!("No differences to save, you are up to date!");
    }

    // Open output file (for appending) in config dir
    let output_path = config_path.join("unsorted.rn");
    let mut output = BufWriter::new(
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&output_path)
            .wrap_err_with(|| format!("Failed to open output file {}", output_path))?,
    );
    output.write_all("// This file is generated by konfigkoll\n".as_bytes())?;
    output.write_all(
        "// You will need to merge the changes you want into your own actual config\n".as_bytes(),
    )?;
    output.write_all("pub fn unsorted_additions(props, cmds) {\n".as_bytes())?;
    let prefix = script_engine.state().settings().save_prefix();
    konfigkoll_core::save::save_packages(&prefix, &mut output, pkg_additions.into_iter())?;
    let files_path = config_path.join("files");
    let sensitive_configs = script_engine.state().settings().sensitive_configs()?;
    konfigkoll_core::save::save_fs_changes(
        &prefix,
        &mut output,
        |path, contents| {
            if sensitive_configs.is_match(path.as_str()) {
                tracing::warn!(
                    "{} has changes, but it is marked sensitive, won't auto-save",
                    path
                );
                return Ok(());
            }
            match (confirmation == Paranoia::DryRun, &filter) {
                (true, _) => save::noop_file_data_saver(path),
                (false, Some(filter)) => {
                    if path.starts_with(filter) {
                        save::file_data_saver(&files_path, path, contents)
                    } else {
                        save::filtered_file_data_saver(path)
                    }
                }
                (false, None) => save::file_data_saver(&files_path, path, contents),
            }
        },
        fs_changes.iter(),
        interner,
    )?;
    output.write_all("}\n".as_bytes())?;

    output.write_all(
        "\n// These are entries in your config that are not applied to the current system\n"
            .as_bytes(),
    )?;
    output.write_all(
        "// Note that these may not correspond *exactly* to what is in your config\n".as_bytes(),
    )?;
    output.write_all("// (e.g. write and copy will get mixed up).\n".as_bytes())?;
    output.write_all("pub fn unsorted_removals(props, cmds) {\n".as_bytes())?;
    konfigkoll_core::save::save_packages(&prefix, &mut output, pkg_removals.into_iter())?;
    output.write_all("}\n".as_bytes())?;
    Ok(())
}

/// Implements the actual application for the `apply` command
#[allow(clippy::too_many_arguments)]
fn cmd_apply_changes(
    confirmation: Paranoia,
    debug_force_dry_run: bool,
    script_engine: &ScriptEngine,
    interner: &Arc<Interner>,
    backends_pkg: &Arc<PackageBackendMap>,
    backend_files: &Arc<dyn Files>,
    package_maps: &PackageMapMap,
    fs_changes: Vec<FsInstruction>,
    pkgs_changes: Vec<(&PkgIdent, PkgInstruction)>,
) -> eyre::Result<()> {
    if fs_changes.is_empty() && pkgs_changes.is_empty() {
        tracing::info!("No system changes to apply, you are up-to-date");
    } else {
        tracing::warn!("Applying changes");
    }

    let mut applicator = create_applicator(
        confirmation,
        debug_force_dry_run,
        backends_pkg,
        interner,
        package_maps,
        backend_files,
        script_engine.state().settings().diff(),
        script_engine.state().settings().pager(),
    );

    // Split into early / late file changes based on settings
    let early_configs = script_engine.state().settings().early_configs()?;
    let mut early_fs_changes = vec![];
    let mut late_fs_changes = vec![];
    for change in fs_changes {
        if early_configs.is_match(change.path.as_str()) {
            early_fs_changes.push(change);
        } else {
            late_fs_changes.push(change);
        }
    }

    // Apply early file system
    apply_files(applicator.as_mut(), &mut early_fs_changes)?;

    // Apply packages
    apply_packages(
        applicator.as_mut(),
        pkgs_changes.into_iter(),
        package_maps,
        interner,
    )?;

    // Apply rest of file system
    apply_files(applicator.as_mut(), &mut late_fs_changes)?;

    std::mem::forget(early_fs_changes);
    std::mem::forget(late_fs_changes);
    Ok(())
}

/// Compute the FS changes for the save direction
fn fs_state_diff_save(script_fs: FsEntries, sys_fs: FsEntries) -> eyre::Result<Vec<FsInstruction>> {
    let mut fs_additions =
        konfigkoll_core::state::diff(&DiffGoal::Save, script_fs, sys_fs)?.collect_vec();
    fs_additions.sort();
    Ok(fs_additions)
}

/// Compute the FS changes for the apply direction
fn fs_state_diff_apply(
    backend_files: &Arc<dyn Files>,
    fs_scan_result: &ScanResult,
    script_fs: FsEntries,
    sys_fs: FsEntries,
) -> eyre::Result<Vec<FsInstruction>> {
    let mut fs_changes = konfigkoll_core::state::diff(
        &DiffGoal::Apply(backend_files.clone(), fs_scan_result.borrow_path_map()),
        sys_fs,
        script_fs,
    )?
    .collect_vec();
    fs_changes.sort();
    Ok(fs_changes)
}
