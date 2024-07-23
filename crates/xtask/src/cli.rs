use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
#[clap(disable_help_subcommand = true)]
pub(crate) struct Cli {
    /// Operation to perform
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Generate man page
    Man {
        /// Output directory
        #[arg(short, long)]
        output: Utf8PathBuf,
        /// Command to generate for
        cmd: CommandName,
    },
    /// Generate shell completions
    Completions {
        /// Output directory
        #[arg(short, long)]
        output: Utf8PathBuf,
        /// Command to generate for
        cmd: CommandName,
    },
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CommandName {
    Paketkoll,
    Konfigkoll,
}

impl std::fmt::Display for CommandName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandName::Paketkoll => write!(f, "paketkoll"),
            CommandName::Konfigkoll => write!(f, "konfigkoll"),
        }
    }
}
