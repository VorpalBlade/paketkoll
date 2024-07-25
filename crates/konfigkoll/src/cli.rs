use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
#[clap(disable_help_subcommand = true)]
pub struct Cli {
    /// Path to config directory (if not the current directory)
    #[arg(long, short = 'c')]
    pub config_path: Option<Utf8PathBuf>,
    /// Trust mtime (don't check checksum if mtime matches (not supported on Debian))
    #[arg(long)]
    pub trust_mtime: bool,
    /// How much to ask for confirmation
    #[arg(long, short = 'p', default_value = "ask")]
    pub confirmation: Paranoia,
    /// For debugging: force a dry run applicator
    #[arg(long, hide = true, default_value = "false")]
    pub debug_force_dry_run: bool,
    /// Operation to perform
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create a new template config directory
    Init {},
    /// Save config to unsorted.rn script (for you to merge into your config)
    Save {},
    /// Check package files and search for unexpected files
    Apply {},
    /// Check for syntax errors and other issues
    Check {},
    /// Diff a specific path
    Diff {
        /// Path to diff
        path: Utf8PathBuf,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Paranoia {
    /// Don't ask, just do it
    Yolo,
    /// Ask for groups of changes
    #[default]
    Ask,
    /// Dry run, don't do anything
    DryRun,
}
