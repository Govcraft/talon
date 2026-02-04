//! Talon CLI - Interactive AI assistant interface

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use talon_cli::commands;

/// Talon - Secure multi-channel AI assistant
#[derive(Parser)]
#[command(name = "talon")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session
    Chat(commands::ChatArgs),
    /// Manage skills
    Skills(commands::SkillsArgs),
    /// Manage configuration
    Config(commands::ConfigArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.command {
        Some(Commands::Chat(args)) => commands::chat(args).await?,
        Some(Commands::Skills(args)) => commands::skills(args).await?,
        Some(Commands::Config(args)) => commands::config(args).await?,
        None => {
            // Default to interactive chat
            commands::chat(commands::ChatArgs::default()).await?;
        }
    }

    Ok(())
}
