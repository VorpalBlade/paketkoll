use std::fmt::Display;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Trust mtime (don't check checksum if it matches)
    #[arg(long)]
    pub(crate) trust_mtime: bool,
    /// Include config files in the check
    #[arg(long, default_value_t = ConfigFiles::Exclude)]
    pub(crate) config_files: ConfigFiles,
    /// Which package manager backend to use
    #[arg(short, long, default_value_t = Backend::Auto)]
    pub(crate) backend: Backend,
}

/// Determine which package manager backend to use
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Backend {
    /// Select based on current distro
    Auto,
    /// Backend for Arch Linux and derived distros (pacman)
    #[cfg(feature = "arch_linux")]
    ArchLinux,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[cfg(feature = "debian")]
    Debian,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::Auto => write!(f, "auto"),
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => write!(f, "arch-linux"),
            #[cfg(feature = "debian")]
            Backend::Debian => write!(f, "debian"),
        }
    }
}

/// Describe how to check config files
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, clap::ValueEnum)]
pub enum ConfigFiles {
    /// Include config files in check
    Include,
    /// Exclude config files in check
    Exclude,
    /// Only check config files
    Only,
}

impl Display for ConfigFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigFiles::Include => write!(f, "include"),
            ConfigFiles::Exclude => write!(f, "exclude"),
            ConfigFiles::Only => write!(f, "only"),
        }
    }
}
