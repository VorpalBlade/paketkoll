//! Implements the CLI for paketkoll

mod cli;

use ahash::AHashSet;
use clap::Parser;
use cli::{Backend, Cli};
use paketkoll_core::{
    backend,
    config::{self, PackageFilter},
};
use proc_exit::{Code, Exit};

fn main() -> anyhow::Result<Exit> {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    builder.init();
    let cli = Cli::parse();

    let (interner, found_issues) = backend::check(&cli.try_into()?)?;

    let mut found_issues: Vec<_> = found_issues.collect();
    found_issues.sort_by_key(|(pkg, issue)| {
        (
            pkg.and_then(|e| interner.try_resolve(&e.as_interner_ref())),
            issue.path().to_path_buf(),
        )
    });

    let has_issues = !found_issues.is_empty();

    for (pkg, issue) in found_issues.into_iter() {
        let pkg = pkg
            .and_then(|e| interner.try_resolve(&e.as_interner_ref()))
            .unwrap_or("UNKNOWN_PKG");
        for kind in issue.kinds() {
            println!("{pkg}: {:?} {kind}", issue.path());
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

impl TryFrom<Cli> for config::CheckConfiguration {
    type Error = anyhow::Error;

    fn try_from(value: Cli) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .trust_mtime(value.trust_mtime)
            .config_files(value.config_files.into())
            .backend(value.backend.try_into()?)
            .package_filter(convert_filter(value.packages))
            .build())
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
