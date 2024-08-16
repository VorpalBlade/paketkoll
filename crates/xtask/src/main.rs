use clap::CommandFactory;
use clap::Parser;
use clap::ValueEnum;
use clap_complete::Shell;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use cli::Commands;

mod cli;

fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env()?;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();
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
