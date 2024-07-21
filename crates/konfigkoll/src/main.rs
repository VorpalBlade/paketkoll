use ahash::AHashSet;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;
use compact_str::CompactString;
use either::Either;
use itertools::Itertools;
use konfigkoll::cli::Cli;
use konfigkoll::cli::Commands;
use konfigkoll::cli::Paranoia;
use konfigkoll_core::apply::apply_files;
use konfigkoll_core::apply::apply_packages;
use konfigkoll_core::apply::Applicator;
use konfigkoll_core::diff::show_fs_instr_diff;
use konfigkoll_core::utils::safe_path_join;
use konfigkoll_script::Phase;
use konfigkoll_types::PkgInstructions;
use konfigkoll_types::{FileContents, FsInstruction};
use paketkoll_cache::FilesCache;
use paketkoll_core::backend::ConcreteBackend;
use paketkoll_core::config::CheckAllFilesConfiguration;
use paketkoll_core::config::CommonFileCheckConfiguration;
use paketkoll_core::config::ConfigFiles;
use paketkoll_core::file_ops::mismatching_and_unexpected_files;
use paketkoll_core::paketkoll_types::intern::Interner;
use paketkoll_types::backend::FilesBackendMap;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::Packages;
use paketkoll_types::backend::{Backend, Files};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::io::BufWriter;
use std::io::Write;
use std::sync::Arc;

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
    let enabled_file_backends: Vec<_> = script_engine
        .state()
        .settings()
        .enabled_file_backends()
        .collect();
    let enabled_pkg_backends: Vec<_> = script_engine
        .state()
        .settings()
        .enabled_pkg_backends()
        .collect();
    let backend_cfg = paketkoll_core::backend::BackendConfiguration::builder()
        .build()
        .context("Failed to build backend config")?;
    let backends_pkg: Arc<PackageBackendMap> = Arc::new(
        enabled_pkg_backends
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

    let backends_files: Arc<FilesBackendMap> = Arc::new(
        enabled_file_backends
            .iter()
            .map(|b| {
                let b: ConcreteBackend = (*b)
                    .try_into()
                    .context("Backend is not supported by current build")?;
                let backend = b
                    .create_files(&backend_cfg, &interner)
                    .with_context(|| format!("Failed to create backend {b}"))?;
                let backend = FilesCache::from_path(backend, proj_dirs.cache_dir())
                    .context("Failed to create disk cache")?;
                let b: Arc<dyn Files> = Arc::new(backend);
                Ok(b)
            })
            .map(|b| b.map(|b| (b.as_backend_enum(), b)))
            .collect::<anyhow::Result<_>>()?,
    );

    // Load installed packages
    tracing::info!("Starting package loading background job");
    let package_loader = {
        let interner = interner.clone();
        let backends_pkg = backends_pkg.clone();
        tokio::task::spawn_blocking(move || load_packages(&interner, &backends_pkg))
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
        let backends_files = backends_files.clone();
        tokio::task::spawn_blocking(move || {
            scan_fs(&interner, &backends_files, &ignores, trust_mtime)
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
        &backends_files,
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
            script_engine.state().settings().diff(),
            script_engine.state().settings().pager(),
        );
        let pkg_diff = package_diff(&pkgs_sys, &script_engine);
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
    let fs_instructions_sys = fs_instructions_sys.await??;
    tracing::info!("Got file system scan results");

    // Compare expected to system
    let mut script_fs = konfigkoll_core::state::FsEntries::default();
    let mut sys_fs = konfigkoll_core::state::FsEntries::default();
    let fs_actions = std::mem::take(&mut script_engine.state_mut().commands_mut().fs_actions);
    script_fs.apply_instructions(fs_actions.into_iter(), true);
    sys_fs.apply_instructions(fs_instructions_sys.into_iter(), false);

    // Packages are so much easier
    let pkg_diff = package_diff(&pkgs_sys, &script_engine);

    // At the end, decide what we want to do with the results
    match cli.command {
        Commands::Save {} => {
            tracing::info!("Saving changes");
            // Split out additions and removals
            let mut fs_additions = konfigkoll_core::state::diff(script_fs, sys_fs).collect_vec();
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
                    true => noop_file_data_saver(path),
                    false => file_data_saver(&files_path, path, contents),
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
            let mut fs_changes = konfigkoll_core::state::diff(sys_fs, script_fs).collect_vec();
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
            let mut fs_changes = konfigkoll_core::state::diff(sys_fs, script_fs).collect_vec();
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

fn scan_fs(
    interner: &Arc<Interner>,
    backends_files: &FilesBackendMap,
    ignores: &[CompactString],
    trust_mtime: bool,
) -> anyhow::Result<Vec<FsInstruction>> {
    let mut fs_instructions_sys = vec![];
    for backend in backends_files.values() {
        let files = backend.files(interner).with_context(|| {
            format!(
                "Failed to collect information from backend {}",
                backend.name()
            )
        })?;
        let common_config = CommonFileCheckConfiguration::builder()
            .trust_mtime(trust_mtime)
            .config_files(ConfigFiles::Include)
            .build()?;
        let unexpected_config = CheckAllFilesConfiguration::builder()
            .canonicalize_paths(backend.may_need_canonicalization())
            .ignored_paths(ignores.to_owned())
            .build()?;
        let issues = mismatching_and_unexpected_files(files, &common_config, &unexpected_config)?;

        // Convert issues to an instruction stream
        fs_instructions_sys
            .extend(konfigkoll_core::conversion::convert_issues_to_fs_instructions(issues)?);
    }
    // Ensure instructions are sorted
    fs_instructions_sys.sort();
    Ok(fs_instructions_sys)
}

fn load_packages(
    interner: &Arc<Interner>,
    backends_pkg: &PackageBackendMap,
) -> anyhow::Result<(PkgInstructions, BTreeMap<Backend, Arc<PackageMap>>)> {
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

type PackagePair<'a> = (
    &'a konfigkoll_types::PkgIdent,
    &'a konfigkoll_types::PkgInstruction,
);

/// Get a diff of packages
fn package_diff<'input>(
    sorted_pkgs_sys: &'input PkgInstructions,
    script_engine: &'input konfigkoll_script::ScriptEngine,
) -> impl Iterator<Item = itertools::EitherOrBoth<PackagePair<'input>, PackagePair<'input>>> {
    let left = sorted_pkgs_sys.iter();
    let right = script_engine
        .state()
        .commands()
        .package_actions
        .iter()
        .sorted();

    konfigkoll_core::diff::comm(left, right)
}

fn create_applicator(
    confirmation: Paranoia,
    force_dry_run: bool,
    backend_map: &PackageBackendMap,
    diff_command: Vec<String>,
    pager_command: Vec<String>,
) -> Box<dyn Applicator> {
    // TODO: This is where privilege separation would happen (well, one of the locations)
    let inner_applicator = if force_dry_run {
        Either::Left(konfigkoll_core::apply::NoopApplicator::default())
    } else {
        Either::Right(konfigkoll_core::apply::InProcessApplicator::new(
            backend_map.clone(),
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

/// Copy files to the config directory, under the "files/".
fn file_data_saver(
    files_path: &Utf8Path,
    path: &Utf8Path,
    contents: &FileContents,
) -> Result<(), anyhow::Error> {
    tracing::info!("Saving file data for {}", path);
    let full_path = safe_path_join(files_path, path);
    std::fs::create_dir_all(full_path.parent().with_context(|| {
        format!("Impossible error: joined path should always below config dir: {full_path}")
    })?)?;
    match contents {
        FileContents::Literal { checksum: _, data } => {
            let mut file = std::fs::File::create(&full_path)?;
            file.write_all(data)?;
        }
        FileContents::FromFile { checksum: _, path } => {
            std::fs::copy(path, &full_path)?;
        }
    }
    Ok(())
}

fn noop_file_data_saver(path: &Utf8Path) -> Result<(), anyhow::Error> {
    tracing::info!("Would save file data for {}", path);
    Ok(())
}

fn removal_file_data_saver(path: &Utf8Path) -> Result<(), anyhow::Error> {
    tracing::info!("The file {} is gone", path);
    Ok(())
}
