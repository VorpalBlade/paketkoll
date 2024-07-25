use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;

use cli::Commands;

mod cli;

fn main() -> anyhow::Result<()> {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.init();
    let cli = cli::Cli::parse();

    match cli.command {
        Commands::Man { output, cmd } => {
            let cmd = match cmd {
                cli::CommandName::Paketkoll => paketkoll::cli::Cli::command(),
                cli::CommandName::Konfigkoll => konfigkoll::cli::Cli::command(),
            };
            std::fs::create_dir_all(&output)?;
            clap_mangen::generate_to(cmd, &output)?;
        }
        Commands::Completions { output, cmd } => {
            let bin_name = cmd.to_string();
            let mut cmd = match cmd {
                cli::CommandName::Paketkoll => paketkoll::cli::Cli::command(),
                cli::CommandName::Konfigkoll => konfigkoll::cli::Cli::command(),
            };
            std::fs::create_dir_all(&output)?;
            for &shell in Shell::value_variants() {
                clap_complete::generate_to(shell, &mut cmd, &bin_name, &output)?;
            }
        }
    }
    Ok(())
}
