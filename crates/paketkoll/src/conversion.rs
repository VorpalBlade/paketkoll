use ahash::AHashSet;

use crate::cli::Backend;
use crate::cli::Cli;
use crate::cli::Commands;
use crate::cli::ConfigFiles;

impl TryFrom<Backend> for paketkoll_core::backend::ConcreteBackend {
    type Error = eyre::Error;

    fn try_from(value: Backend) -> Result<Self, Self::Error> {
        match value {
            Backend::Auto => {
                let info = os_info::get();
                match info.os_type() {
                    #[cfg(feature = "arch_linux")]
                    os_info::Type::Arch | os_info::Type::EndeavourOS | os_info::Type::Manjaro => {
                        Ok(Self::Pacman)
                    }
                    #[cfg(feature = "debian")]
                    os_info::Type::Debian
                    | os_info::Type::Mint
                    | os_info::Type::Pop
                    | os_info::Type::Raspbian
                    | os_info::Type::Ubuntu => Ok(Self::Apt),
                    _ => Err(eyre::eyre!(
                        "Unknown or unsupported distro: {} (try passing a specific backend if you \
                         think it should work)",
                        info.os_type()
                    )),
                }
            }
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => Ok(Self::Pacman),
            #[cfg(feature = "debian")]
            Backend::Debian => Ok(Self::Apt),
            Backend::Flatpak => Ok(Self::Flatpak),
            #[cfg(feature = "systemd_tmpfiles")]
            Backend::SystemdTmpfiles => Ok(Self::SystemdTmpfiles),
        }
    }
}

impl From<ConfigFiles> for paketkoll_core::config::ConfigFiles {
    fn from(value: ConfigFiles) -> Self {
        match value {
            ConfigFiles::Include => Self::Include,
            ConfigFiles::Exclude => Self::Exclude,
            ConfigFiles::Only => Self::Only,
        }
    }
}

impl TryFrom<&Cli> for paketkoll_core::backend::BackendConfiguration {
    type Error = eyre::Error;

    fn try_from(value: &Cli) -> Result<Self, Self::Error> {
        let mut builder = Self::builder();

        match value.command {
            Commands::Check { ref packages } => {
                builder.package_filter(convert_filter(packages.clone()));
            }
            Commands::CheckUnexpected {
                ignore: _,
                canonicalize: _,
            } => {}
            Commands::InstalledPackages => {}
            Commands::OriginalFile { .. } => {}
            Commands::Owns { .. } => {}
            Commands::DebugPackageFileData { .. } => {}
        }
        Ok(builder.build()?)
    }
}

impl TryFrom<&Cli> for paketkoll_core::config::CommonFileCheckConfiguration {
    type Error = eyre::Error;

    fn try_from(value: &Cli) -> Result<Self, Self::Error> {
        let mut builder = Self::builder();

        builder.trust_mtime(value.trust_mtime);
        builder.config_files(value.config_files.into());

        Ok(builder.build()?)
    }
}

/// Produce a 'static reference to a package filter that will live long enough.
///
/// We intentionally "leak" memory here, it will live as long as the program
/// runs, which is fine.
fn convert_filter(packages: Vec<String>) -> &'static paketkoll_core::backend::PackageFilter {
    let packages: AHashSet<String> = AHashSet::from_iter(packages);
    let boxed = Box::new(if packages.is_empty() {
        paketkoll_core::backend::PackageFilter::Everything
    } else {
        paketkoll_core::backend::PackageFilter::FilterFunction(Box::new(move |pkg| {
            if packages.contains(pkg) {
                paketkoll_core::backend::FilterAction::Include
            } else {
                paketkoll_core::backend::FilterAction::Exclude
            }
        }))
    });
    Box::<paketkoll_core::backend::PackageFilter>::leak(boxed)
}
