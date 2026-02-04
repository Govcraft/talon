//! Config command implementation

use clap::{Args, Subcommand};
use talon_core::TalonResult;

/// Arguments for the config command
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// Config subcommands
#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show current configuration
    Show,
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Initialize configuration
    Init,
}

/// Run the config command
///
/// # Errors
///
/// Returns error if command fails
pub async fn config(args: ConfigArgs) -> TalonResult<()> {
    match args.command {
        ConfigCommand::Show => {
            tracing::info!("Showing configuration");
            let config = talon_core::TalonConfig::load()?;
            println!("{:#?}", config);
        }
        ConfigCommand::Set { key, value } => {
            tracing::info!(key = %key, value = %value, "Setting configuration");
            println!("Configuration set not yet implemented");
        }
        ConfigCommand::Get { key } => {
            tracing::info!(key = %key, "Getting configuration");
            println!("Configuration get not yet implemented");
        }
        ConfigCommand::Init => {
            tracing::info!("Initializing configuration");
            println!("Configuration init not yet implemented");
        }
    }

    Ok(())
}
