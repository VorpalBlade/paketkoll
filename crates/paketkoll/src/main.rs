//! Implements the CLI for paketkoll

mod cli;

use std::{
    io::{stdout, BufWriter, Write},
    os::unix::ffi::OsStrExt,
};

use ahash::AHashSet;
use clap::Parser;
use cli::{Backend, Cli};
use paketkoll_core::{
    backend,
    config::{self, CheckUnexpectedConfigurationBuilder, PackageFilter},
    types::{Issue, PackageRef},
};
use proc_exit::{Code, Exit};
use rayon::slice::ParallelSliceMut;

#[cfg(target_env = "musl")]
use mimalloc::MiMalloc;

#[cfg(target_env = "musl")]
#[cfg_attr(target_env = "musl", global_allocator)]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> anyhow::Result<Exit> {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    builder.init();
    let cli = Cli::parse();

    let (interner, mut found_issues) = match cli.command {
        cli::Commands::Check { .. } => backend::check(&(&cli).try_into()?)?,
        cli::Commands::CheckUnexpected {
            ref ignore,
            canonicalize,
        } => backend::check_unexpected(&(&cli).try_into()?, &{
            let mut builder = CheckUnexpectedConfigurationBuilder::default();
            builder.ignored_paths(ignore.clone());
            builder.canonicalize_paths(canonicalize);
            builder.build()?
        })?,
    };

    let key_extractor = |(pkg, issue): &(Option<PackageRef>, Issue)| {
        (
            pkg.and_then(|e| interner.try_resolve(&e.as_interner_ref())),
            issue.path().to_path_buf(),
        )
    };

    if found_issues.len() > 1000 {
        found_issues.par_sort_by_key(key_extractor);
    } else {
        found_issues.sort_by_key(key_extractor);
    }

    let has_issues = !found_issues.is_empty();

    {
        let mut stdout = BufWriter::new(stdout().lock());
        for (pkg, issue) in found_issues.iter() {
            let pkg = pkg.and_then(|e| interner.try_resolve(&e.as_interner_ref()));
            for kind in issue.kinds() {
                if let Some(pkg) = pkg {
                    write!(stdout, "{pkg}: ")?;
                }
                // Prefer to not do any escaping. This doesn't assume unicode.
                // Also it is faster.
                stdout.write_all(issue.path().as_os_str().as_bytes())?;
                writeln!(stdout, " {kind}")?;
            }
        }
    }
    Ok(if has_issues {
        Exit::new(Code::FAILURE)
    } else {
        Exit::new(Code::SUCCESS)
    })
}

impl TryFrom<Backend> for paketkoll_core::config::Backend {
    type Error = anyhow::Error;

    fn try_from(value: Backend) -> Result<Self, Self::Error> {
        match value {
            Backend::Auto => {
                let info = os_info::get();
                match info.os_type() {
                    #[cfg(feature = "arch_linux")]
                    os_info::Type::Arch | os_info::Type::EndeavourOS |
                    os_info::Type::Manjaro => Ok(Self::ArchLinux),
                    #[cfg(feature = "debian")]
                    os_info::Type::Debian | os_info::Type::Mint |
                    os_info::Type::Pop | os_info::Type::Raspbian |
                    os_info::Type::Ubuntu => Ok(Self::Debian),
                    _ => Err(anyhow::anyhow!(
                        "Unknown or unsupported distro: {} (try passing a specific backend if you think it should work)",
                        info.os_type())),
                }
            }
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Self::ArchLinux),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Self::Debian),
        }
    }
}

impl From<cli::ConfigFiles> for config::ConfigFiles {
    fn from(value: cli::ConfigFiles) -> Self {
        match value {
            cli::ConfigFiles::Include => Self::Include,
            cli::ConfigFiles::Exclude => Self::Exclude,
            cli::ConfigFiles::Only => Self::Only,
        }
    }
}

impl TryFrom<&Cli> for config::CommonConfiguration {
    type Error = anyhow::Error;

    fn try_from(value: &Cli) -> Result<Self, Self::Error> {
        let mut builder = Self::builder();

        builder.trust_mtime(value.trust_mtime);
        builder.config_files(value.config_files.into());
        builder.backend(value.backend.try_into()?);

        match value.command {
            cli::Commands::Check { ref packages } => {
                builder.package_filter(convert_filter(packages.clone()));
            }
            cli::Commands::CheckUnexpected {
                ignore: _,
                canonicalize: _,
            } => {}
        }
        Ok(builder.build()?)
    }
}

/// Produce a 'static reference to a package filter that will live long enough.
///
/// We intentionally "leak" memory here, it will live as long as the program runs, which is fine.
fn convert_filter(packages: Vec<String>) -> &'static config::PackageFilter {
    let packages: AHashSet<String> = AHashSet::from_iter(packages);
    let boxed = Box::new(if packages.is_empty() {
        config::PackageFilter::Everything
    } else {
        config::PackageFilter::FilterFunction(Box::new(move |pkg| {
            if packages.contains(pkg) {
                config::FilterAction::Include
            } else {
                config::FilterAction::Exclude
            }
        }))
    });
    Box::<PackageFilter>::leak(boxed)
}
