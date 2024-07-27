//! Implements the CLI for paketkoll

use std::io::stdout;
use std::io::BufWriter;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use ahash::AHashSet;
use anyhow::Context;
use clap::Parser;
use proc_exit::Code;
use proc_exit::Exit;
use rayon::prelude::*;

#[cfg(target_env = "musl")]
use mimalloc::MiMalloc;
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

#[cfg(target_env = "musl")]
#[cfg_attr(target_env = "musl", global_allocator)]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> anyhow::Result<Exit> {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    builder.init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { .. } | Commands::CheckUnexpected { .. } => run_file_checks(&cli),
        Commands::InstalledPackages => {
            let (interner, packages) =
                package_ops::installed_packages(&(cli.backend.try_into()?), &(&cli).try_into()?)?;
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
                .context("Failed to create backend")?;

            let package_map = backend_impl
                .package_map_complete(&interner)
                .with_context(|| format!("Failed to collect information from backend {backend}"))?;

            let package: &str = match package {
                Some(p) => p,
                None => {
                    let mut inputs = AHashSet::default();
                    inputs.insert(Path::new(path));
                    let file_map = backend_impl.owning_packages(&inputs, &interner)?;
                    if file_map.len() != 1 {
                        return Err(anyhow::anyhow!(
                            "Expected exactly one package to own the file, found {}",
                            file_map.len()
                        ));
                    }
                    let (_path, owner) = file_map
                        .into_iter()
                        .next()
                        .expect("Impossible with check above");
                    if let Some(package) = owner {
                        package.to_str(&interner)
                    } else {
                        return Err(anyhow::anyhow!("No package owns the given file"));
                    }
                }
            };
            let queries = vec![OriginalFileQuery {
                package: package.into(),
                path: path.into(),
            }];
            let results = backend_impl
                .original_files(queries.as_slice(), &package_map, &interner)
                .with_context(|| {
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
                .context("Failed to create backend")?;

            let inputs = AHashSet::from_iter(paths.iter().map(Path::new));
            let file_map = backend_impl.owning_packages(&inputs, &interner)?;

            for (path, owner) in file_map {
                if let Some(package) = owner {
                    println!("{}: {}", package.to_str(&interner), path.to_string_lossy());
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
                .context("Failed to create backend")?;

            let package_map = backend_impl
                .package_map_complete(&interner)
                .with_context(|| format!("Failed to collect information from backend {backend}"))?;

            let pkg_ref = PackageRef::get_or_intern(&interner, package);

            let files = backend_impl
                .files_from_archives(&[pkg_ref], &package_map, &interner)
                .with_context(|| {
                    format!(
                        "Failed to collect file information for package {package} from backend \
                         {backend}"
                    )
                })?;

            println!("{:?}", files);

            Ok(Exit::new(Code::SUCCESS))
        }
    }
}

fn print_packages(
    cli: &Cli,
    packages: Vec<PackageInterned>,
    interner: &Interner,
    stdout: &mut BufWriter<std::io::StdoutLock<'_>>,
) -> Result<(), anyhow::Error> {
    match cli.format {
        Format::Human => {
            for pkg in packages {
                let pkg_name = interner
                    .try_resolve(&pkg.name.as_interner_ref())
                    .ok_or_else(|| anyhow::anyhow!("No package name for package"))?;
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

fn run_file_checks(cli: &Cli) -> Result<Exit, anyhow::Error> {
    let (interner, mut found_issues) = match cli.command {
        Commands::Check { .. } => file_ops::check_installed_files(
            &(cli.backend.try_into()?),
            &cli.try_into()?,
            &cli.try_into()?,
        )?,
        Commands::CheckUnexpected {
            ref ignore,
            canonicalize,
        } => file_ops::check_all_files(
            &(cli.backend.try_into()?),
            &cli.try_into()?,
            &cli.try_into()?,
            &{
                let mut builder = CheckAllFilesConfiguration::builder();
                builder.ignored_paths(ignore.clone());
                builder.canonicalize_paths(canonicalize);
                builder.build()?
            },
        )?,
        _ => unreachable!(),
    };

    let key_extractor = |(pkg, issue): &(Option<PackageRef>, Issue)| {
        (
            pkg.and_then(|e| e.try_to_str(&interner)),
            issue.path().to_path_buf(),
        )
    };

    if found_issues.len() > 1000 {
        found_issues.par_sort_by_key(key_extractor);
    } else {
        found_issues.sort_by_key(key_extractor);
    }

    let has_issues = !found_issues.is_empty();

    match cli.format {
        Format::Human => {
            let mut stdout = BufWriter::new(stdout().lock());
            for (pkg, issue) in found_issues.iter() {
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
