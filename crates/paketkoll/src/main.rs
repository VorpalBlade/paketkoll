//! Implements the CLI for paketkoll

use ahash::AHashSet;
use clap::Parser;
use eyre::WrapErr;
use paketkoll::cli::Cli;
use paketkoll::cli::Commands;
use paketkoll::cli::Format;
use paketkoll_core::config::CheckAllFilesConfiguration;
use paketkoll_core::file_ops;
use paketkoll_core::package_ops;
use paketkoll_core::paketkoll_types::intern::Interner;
use paketkoll_core::paketkoll_types::intern::PackageRef;
use paketkoll_core::paketkoll_types::issue::Issue;
use paketkoll_core::paketkoll_types::package::InstallReason;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::package::PackageInterned;
use proc_exit::Code;
use proc_exit::Exit;
use rayon::prelude::*;
use std::io::BufWriter;
use std::io::Write;
use std::io::stdout;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[cfg(target_env = "musl")]
mod _musl {
    use mimalloc::MiMalloc;
    #[global_allocator]
    static GLOBAL: MiMalloc = MiMalloc;
}

fn main() -> eyre::Result<Exit> {
    color_eyre::install()?;
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .with(tracing_error::ErrorLayer::default())
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { .. } | Commands::CheckUnexpected { .. } => run_file_checks(&cli),
        Commands::InstalledPackages => {
            let (interner, packages) =
                package_ops::installed_packages(cli.backend.try_into()?, &(&cli).try_into()?)?;
            let mut stdout = BufWriter::new(stdout().lock());

            print_packages(&cli, packages, &interner, &mut stdout)?;

            Ok(Exit::new(Code::SUCCESS))
        }
        Commands::OriginalFile {
            ref package,
            ref path,
        } => {
            let interner = Interner::new();
            let backend: paketkoll_core::backend::ConcreteBackend = cli.backend.try_into()?;
            let backend_impl = backend
                .create_full(&(&cli).try_into()?, &interner)
                .wrap_err("Failed to create backend")?;

            let package_map = backend_impl
                .package_map_complete(&interner)
                .wrap_err_with(|| {
                    format!("Failed to collect information from backend {backend}")
                })?;

            let package: &str = match package {
                Some(p) => p,
                None => {
                    let mut inputs = AHashSet::default();
                    inputs.insert(Path::new(path));
                    let file_map = backend_impl.owning_packages(&inputs, &interner)?;
                    if file_map.len() != 1 {
                        return Err(eyre::eyre!(
                            "Expected exactly one package to own the file, found {}",
                            file_map.len()
                        ));
                    }
                    let (_path, owner) = file_map
                        .into_iter()
                        .next()
                        .expect("Impossible with check above");
                    if let Some(package) = owner {
                        package.as_str(&interner)
                    } else {
                        return Err(eyre::eyre!("No package owns the given file"));
                    }
                }
            };
            let queries = vec![OriginalFileQuery {
                package: package.into(),
                path: path.into(),
            }];
            let results = backend_impl
                .original_files(queries.as_slice(), &package_map, &interner)
                .wrap_err_with(|| {
                    format!("Failed to collect original files from backend {backend}")
                })?;

            for (_query, result) in results {
                stdout().write_all(&result)?;
            }
            Ok(Exit::new(Code::SUCCESS))
        }
        Commands::Owns { ref paths } => {
            let interner = Interner::new();
            let backend: paketkoll_core::backend::ConcreteBackend = cli.backend.try_into()?;
            let backend_impl = backend
                .create_files(&(&cli).try_into()?, &interner)
                .wrap_err("Failed to create backend")?;

            let inputs = AHashSet::from_iter(paths.iter().map(Path::new));
            let file_map = backend_impl.owning_packages(&inputs, &interner)?;

            for (path, owner) in file_map {
                if let Some(package) = owner {
                    println!("{}: {}", package.as_str(&interner), path.to_string_lossy());
                } else {
                    println!("No package owns this file");
                }
            }
            Ok(Exit::new(Code::SUCCESS))
        }
        Commands::DebugPackageFileData { ref package } => {
            let interner = Interner::new();
            let backend: paketkoll_core::backend::ConcreteBackend = cli.backend.try_into()?;
            let backend_impl = backend
                .create_full(&(&cli).try_into()?, &interner)
                .wrap_err("Failed to create backend")?;

            let package_map = backend_impl
                .package_map_complete(&interner)
                .wrap_err_with(|| {
                    format!("Failed to collect information from backend {backend}")
                })?;

            let pkg_ref = PackageRef::get_or_intern(&interner, package);

            let files = backend_impl
                .files_from_archives(&[pkg_ref], &package_map, &interner)
                .wrap_err_with(|| {
                    format!(
                        "Failed to collect file information for package {package} from backend \
                         {backend}"
                    )
                })?;

            println!("{files:?}");

            Ok(Exit::new(Code::SUCCESS))
        }
    }
}

fn print_packages(
    cli: &Cli,
    packages: Vec<PackageInterned>,
    interner: &Interner,
    stdout: &mut BufWriter<std::io::StdoutLock<'_>>,
) -> eyre::Result<()> {
    match cli.format {
        Format::Human => {
            for pkg in packages {
                let pkg_name = interner
                    .try_resolve(&pkg.name.as_interner_ref())
                    .ok_or_else(|| eyre::eyre!("No package name for package"))?;
                match pkg.reason {
                    Some(InstallReason::Explicit) => {
                        writeln!(stdout, "{} {}", pkg_name, pkg.version)?;
                    }
                    Some(InstallReason::Dependency) => {
                        writeln!(stdout, "{} {} (as dep)", pkg_name, pkg.version)?;
                    }
                    None => writeln!(
                        stdout,
                        "{} {} (unknown install reason)",
                        pkg_name, pkg.version
                    )?,
                }
            }
        }
        #[cfg(feature = "json")]
        Format::Json => {
            let packages: Vec<_> = packages
                .into_par_iter()
                .map(|pkg| pkg.into_direct(interner))
                .collect();
            serde_json::to_writer_pretty(stdout, &packages)?;
        }
    };
    Ok(())
}

fn run_file_checks(cli: &Cli) -> eyre::Result<Exit> {
    let (interner, mut found_issues) = match cli.command {
        Commands::Check { .. } => file_ops::check_installed_files(
            cli.backend.try_into()?,
            &cli.try_into()?,
            &cli.try_into()?,
        )?,
        Commands::CheckUnexpected { canonicalize } => file_ops::check_all_files(
            cli.backend.try_into()?,
            &cli.try_into()?,
            &cli.try_into()?,
            &{
                let mut builder = CheckAllFilesConfiguration::builder();
                builder.ignored_paths(cli.ignore.clone());
                builder.canonicalize_paths(canonicalize);
                builder.build()?
            },
        )?,
        _ => unreachable!(),
    };

    let key_extractor = |(pkg, issue): &(Option<PackageRef>, Issue)| {
        (
            pkg.and_then(|e| e.try_as_str(&interner)),
            issue.path().to_path_buf(),
        )
    };

    if found_issues.len() > 1000 {
        found_issues.par_sort_by_key(key_extractor);
    } else {
        found_issues.sort_by_key(key_extractor);
    }

    if let Commands::Check { .. } = cli.command {
        if !cli.ignore.is_empty() {
            // Do post-processing of ignores as the check command doesn't have that built
            // in.
            let ignores = file_ops::build_ignore_overrides(&cli.ignore)?;
            found_issues.retain(|(_, issue)| {
                let path = issue.path();
                match ignores.matched(path, path.is_dir()) {
                    ignore::Match::None => (),
                    ignore::Match::Ignore(_) => {
                        return false;
                    }
                    ignore::Match::Whitelist(_) => (),
                }
                true
            });
        }
    }

    let has_issues = !found_issues.is_empty();

    match cli.format {
        Format::Human => {
            let mut stdout = BufWriter::new(stdout().lock());
            for (pkg, issue) in &found_issues {
                let pkg = pkg.and_then(|e| interner.try_resolve(&e.as_interner_ref()));
                for kind in issue.kinds() {
                    if let Some(pkg) = pkg {
                        write!(stdout, "{pkg}: ")?;
                    }
                    // Prefer to not do any escaping. This doesn't assume unicode.
                    // Also, it is faster.
                    stdout.write_all(issue.path().as_os_str().as_bytes())?;
                    writeln!(stdout, " {kind}")?;
                }
            }
        }
        #[cfg(feature = "json")]
        Format::Json => {
            let mut stdout = BufWriter::new(stdout().lock());
            let found_issues: Vec<_> = found_issues
                .into_par_iter()
                .map(|(package, issue)| {
                    let package = package.and_then(|e| interner.try_resolve(&e.as_interner_ref()));
                    IssueReport { package, issue }
                })
                .collect();
            serde_json::to_writer_pretty(&mut stdout, &found_issues)?;
        }
    }

    Ok(if has_issues {
        Exit::new(Code::FAILURE)
    } else {
        Exit::new(Code::SUCCESS)
    })
}

#[cfg(feature = "json")]
#[derive(Debug, serde::Serialize)]
struct IssueReport<'interner> {
    package: Option<&'interner str>,
    issue: Issue,
}
