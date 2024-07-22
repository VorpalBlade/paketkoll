use ahash::AHashSet;
use anyhow::Context;
use apply::create_applicator;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;
use compact_str::CompactString;
use itertools::Itertools;
use konfigkoll::cli::Cli;
use konfigkoll::cli::Commands;
use konfigkoll::cli::Paranoia;
use konfigkoll_core::apply::apply_files;
use konfigkoll_core::apply::apply_packages;
use konfigkoll_core::diff::show_fs_instr_diff;
use konfigkoll_core::state::DiffGoal;
use konfigkoll_script::Phase;
use paketkoll_cache::FilesCache;
use paketkoll_core::backend::ConcreteBackend;
use paketkoll_core::paketkoll_types::intern::Interner;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::Packages;
use std::io::BufWriter;
use std::io::Write;
use std::sync::Arc;

mod apply;
mod fs_scan;
mod pkgs;
mod save;

#[cfg(target_env = "musl")]
use mimalloc::MiMalloc;

#[cfg(target_env = "musl")]
#[cfg_attr(target_env = "musl", global_allocator)]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Set up logging with tracing
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()?;
    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(filter)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    // Compatibility for log crate
    tracing_log::LogTracer::init()?;

    let cli = Cli::parse();

    let config_path = match cli.config_path {
        Some(v) => v,
        None => std::env::current_dir()?.try_into()?,
    };

    if let Commands::Init {} = cli.command {
        init_directory(&config_path)?;
        return Ok(());
    }

    let mut script_engine = konfigkoll_script::ScriptEngine::new_with_files(&config_path)?;

    match cli.command {
        Commands::Init {} | Commands::Save {} | Commands::Apply {} | Commands::Diff { .. } => (),
        Commands::Check {} => {
            println!("Scripts loaded successfully");
            return Ok(());
        }
    }

    // Script: Do system discovery and configuration
    script_engine.run_phase(Phase::SystemDiscovery).await?;

    let proj_dirs = directories::ProjectDirs::from("", "", "konfigkoll")
        .context("Failed to get directories for disk cache")?;

    // Create backends
    tracing::info!("Creating backends");
    let interner = Arc::new(Interner::new());
    let file_backend_id = script_engine
        .state()
        .settings()
        .file_backend()
        .ok_or_else(|| anyhow::anyhow!("A file backend must be set"))?;
    let pkg_backend_ids = script_engine
        .state()
        .settings()
        .enabled_pkg_backends()
        .collect_vec();
    let backend_cfg = paketkoll_core::backend::BackendConfiguration::builder()
        .build()
        .context("Failed to build backend config")?;
    let backends_pkg: Arc<PackageBackendMap> = Arc::new(
        pkg_backend_ids
            .iter()
            .map(|b| {
                let b: ConcreteBackend = (*b)
                    .try_into()
                    .context("Backend is not supported by current build")?;
                let backend = b
                    .create_packages(&backend_cfg, &interner)
                    .with_context(|| format!("Failed to create backend {b}"))?;
                let b: Arc<dyn Packages> = Arc::from(backend);
                Ok(b)
            })
            .map(|b| b.map(|b| (b.as_backend_enum(), b)))
            .collect::<anyhow::Result<_>>()?,
    );

    let backend_files: Arc<dyn Files> = {
        let b: ConcreteBackend = file_backend_id
            .try_into()
            .context("Backend is not supported by current build")?;
        let backend = b
            .create_files(&backend_cfg, &interner)
            .with_context(|| format!("Failed to create backend {b}"))?;
        let backend = FilesCache::from_path(backend, proj_dirs.cache_dir())
            .context("Failed to create disk cache")?;
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
        tokio::task::spawn_blocking(move || {
            fs_scan::scan_fs(&interner, &backends_files, &ignores, trust_mtime)
        })
    };

    // Script: Do early package phase
    script_engine.run_phase(Phase::ScriptDependencies).await?;

    tracing::info!("Retriving package loading results");
    let (pkgs_sys, package_maps) = package_loader.await??;
    tracing::info!("Got package loading results");

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
            itertools::EitherOrBoth::Right(v) => Some(v),
        });
        apply_packages(applicator.as_mut(), pkgs_changes)?;
    }

    // Script: Do main phase
    script_engine.run_phase(Phase::Main).await?;

    // Make sure FS actions are sorted
    script_engine.state_mut().commands_mut().fs_actions.sort();

    tracing::info!("Retriving file system scan results...");
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
        Commands::Save {} => {
            tracing::info!("Saving changes");
            // Split out additions and removals
            let mut fs_additions =
                konfigkoll_core::state::diff(&DiffGoal::Save, script_fs, sys_fs)?.collect_vec();
            fs_additions.sort();
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

            // Open output file (for appending) in config dir
            let output_path = config_path.join("unsorted.rn");
            let mut output = BufWriter::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&output_path)
                    .with_context(|| format!("Failed to open output file {}", output_path))?,
            );
            output.write_all("// This file is generated by konfigkoll\n".as_bytes())?;
            output.write_all(
                "// You will need to merge the changes you want into your own actual config\n"
                    .as_bytes(),
            )?;
            output.write_all("pub fn unsorted_additions(props, cmds) {\n".as_bytes())?;
            konfigkoll_core::save::save_packages(&mut output, pkg_additions.into_iter())?;
            let files_path = config_path.join("files");
            konfigkoll_core::save::save_fs_changes(
                &mut output,
                |path, contents| match cli.confirmation == Paranoia::DryRun {
                    true => save::noop_file_data_saver(path),
                    false => save::file_data_saver(&files_path, path, contents),
                },
                fs_additions.iter(),
            )?;
            output.write_all("}\n".as_bytes())?;

            output.write_all("\n// These are entries in your config that are not applied to the current system\n".as_bytes())?;
            output.write_all(
                "// Note that these may not correspond *exactly* to what is in your config\n"
                    .as_bytes(),
            )?;
            output.write_all("// (e.g. write and copy will get mixed up).\n".as_bytes())?;
            output.write_all("pub fn unsorted_removals(props, cmds) {\n".as_bytes())?;
            konfigkoll_core::save::save_packages(&mut output, pkg_removals.into_iter())?;
            output.write_all("}\n".as_bytes())?;
        }
        Commands::Apply {} => {
            tracing::info!("Applying changes");
            let mut fs_changes = konfigkoll_core::state::diff(
                &DiffGoal::Apply(backend_files.clone(), fs_scan_result.borrow_path_map()),
                sys_fs,
                script_fs,
            )?
            .collect_vec();
            fs_changes.sort();
            let pkgs_changes = pkg_diff.filter_map(|v| match v {
                itertools::EitherOrBoth::Both(_, _) => None,
                itertools::EitherOrBoth::Left(_) => None,
                itertools::EitherOrBoth::Right(v) => Some(v),
            });

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

            // Split into early / late file changes based on settings
            let early_configs: AHashSet<Utf8PathBuf> =
                script_engine.state().settings().early_configs().collect();
            let mut early_fs_changes = vec![];
            let mut late_fs_changes = vec![];
            for change in fs_changes {
                if early_configs.contains(&change.path) {
                    early_fs_changes.push(change);
                } else {
                    late_fs_changes.push(change);
                }
            }

            // Apply early file system
            apply_files(applicator.as_mut(), early_fs_changes.iter())?;

            // Apply packages
            apply_packages(applicator.as_mut(), pkgs_changes)?;

            // Apply rest of file system
            apply_files(applicator.as_mut(), late_fs_changes.iter())?;
        }
        Commands::Diff { path } => {
            tracing::info!("Computing diff");
            let mut fs_changes = konfigkoll_core::state::diff(
                &DiffGoal::Apply(backend_files.clone(), fs_scan_result.borrow_path_map()),
                sys_fs,
                script_fs,
            )?
            .collect_vec();
            fs_changes.sort();
            let diff_cmd = script_engine.state().settings().diff();
            let pager_cmd = script_engine.state().settings().pager();
            for change in fs_changes {
                if change.path.starts_with(&path) {
                    show_fs_instr_diff(&change, &diff_cmd, &pager_cmd)?;
                }
            }
        }
        Commands::Check {} | Commands::Init {} => unreachable!(),
    }

    Ok(())
}

fn init_directory(config_path: &Utf8Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_path).context("Failed to create config directory")?;
    std::fs::create_dir_all(config_path.join("files"))?;

    // Create skeleton main script
    let main_script = config_path.join("main.rn");
    if !main_script.exists() {
        std::fs::write(&main_script, include_bytes!("../data/template/main.rn"))?;
    }
    // Create skeleton unsorted script
    let unsorted_script = config_path.join("unsorted.rn");
    if !unsorted_script.exists() {
        std::fs::write(
            &unsorted_script,
            include_bytes!("../data/template/unsorted.rn"),
        )?;
    }
    // Gitignore
    let gitignore = config_path.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, include_bytes!("../data/template/_gitignore"))?;
    }

    // Add an empty Rune.toml
    let runetoml = config_path.join("Rune.toml");
    if !runetoml.exists() {
        std::fs::write(&runetoml, b"")?;
    }

    Ok(())
}
