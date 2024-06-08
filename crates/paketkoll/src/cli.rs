use std::fmt::Display;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
#[clap(disable_help_subcommand = true)]
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
    /// Output format to use
    #[arg(short, long, default_value_t = Format::Human, hide = true)]
    pub(crate) format: Format,
    /// Operation to perform
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Check package files
    Check {
        /// Packages to check (default: all of them)
        packages: Vec<String>,
    },
    /// Check package files and search for unexpected files
    CheckUnexpected {
        /// Paths to ignore (apart from built in ones). Basic globs are supported.
        /// Use ** to match any number of path components.
        #[arg(long)]
        ignore: Vec<String>,
        /// Should paths be canonicalized before checking? If you get many false positives, try this.
        /// Required on Debian due to lack of /usr merge.
        #[arg(long)]
        canonicalize: bool,
    },
    /// Get a list of installed packages
    InstalledPackages,
}

/// Output format to use
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Format {
    /// Human readable output
    Human,
    /// JSON formatted output
    Json,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Human => write!(f, "human"),
            Format::Json => write!(f, "json"),
        }
    }
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
    /// Backend for Flatpak
    Flatpak,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::Auto => write!(f, "auto"),
            #[cfg(feature = "arch_linux")]
            Backend::ArchLinux => write!(f, "arch-linux"),
            #[cfg(feature = "debian")]
            Backend::Debian => write!(f, "debian"),
            Backend::Flatpak => write!(f, "flatpak"),
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
