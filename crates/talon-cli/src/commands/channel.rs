//! Channel command implementation
//!
//! Manages channel configurations using systemd credentials.
//! Tokens are encrypted at rest using the host key (or TPM2 if available).

use clap::{Args, Subcommand};
use talon_core::TalonResult;

/// Arguments for the channel command
#[derive(Args)]
pub struct ChannelArgs {
    #[command(subcommand)]
    pub command: ChannelCommand,
}

/// Channel subcommands
#[derive(Subcommand)]
pub enum ChannelCommand {
    /// Add a channel and store its credentials (requires sudo)
    Add {
        /// Channel type (telegram, discord)
        channel: String,
        /// Bot token (if not provided, will prompt)
        #[arg(long)]
        token: Option<String>,
    },
    /// Remove a channel and delete its credentials (requires sudo)
    Remove {
        /// Channel type (telegram, discord)
        channel: String,
    },
    /// List configured channels
    List,
}

/// Run the channel command
///
/// # Errors
///
/// Returns error if command fails
pub async fn channel(args: ChannelArgs) -> TalonResult<()> {
    match args.command {
        ChannelCommand::Add { channel, token } => add_channel(&channel, token).await,
        ChannelCommand::Remove { channel } => remove_channel(&channel).await,
        ChannelCommand::List => list_channels().await,
    }
}

async fn add_channel(channel_type: &str, token: Option<String>) -> TalonResult<()> {
    match channel_type.to_lowercase().as_str() {
        "telegram" => add_telegram(token).await,
        "discord" => {
            println!("Discord channel not yet implemented");
            Ok(())
        }
        other => {
            eprintln!("Unknown channel type: {other}");
            eprintln!("Available channels: telegram, discord");
            Err(talon_core::TalonError::Config {
                message: format!("unknown channel type: {other}"),
            })
        }
    }
}

async fn add_telegram(token: Option<String>) -> TalonResult<()> {
    use std::io::{self, BufRead, Write};
    use talon_channels::TelegramConfig;

    // Get token from argument or prompt
    let token = match token {
        Some(t) => t,
        None => {
            print!("Enter Telegram bot token: ");
            io::stdout().flush().map_err(|e| talon_core::TalonError::Io {
                message: e.to_string(),
            })?;
            let mut token = String::new();
            io::stdin().lock().read_line(&mut token).map_err(|e| {
                talon_core::TalonError::Io {
                    message: format!("failed to read token: {e}"),
                }
            })?;
            token.trim().to_string()
        }
    };

    if token.is_empty() {
        return Err(talon_core::TalonError::Config {
            message: "token cannot be empty".to_string(),
        });
    }

    // Store using systemd-creds (requires root)
    TelegramConfig::store_token(&token).map_err(|e| talon_core::TalonError::Config {
        message: e.to_string(),
    })?;

    println!("Telegram token encrypted and stored in /etc/credstore.encrypted/");
    Ok(())
}

async fn remove_channel(channel_type: &str) -> TalonResult<()> {
    match channel_type.to_lowercase().as_str() {
        "telegram" => remove_telegram().await,
        "discord" => {
            println!("Discord channel not yet implemented");
            Ok(())
        }
        other => {
            eprintln!("Unknown channel type: {other}");
            Err(talon_core::TalonError::Config {
                message: format!("unknown channel type: {other}"),
            })
        }
    }
}

async fn remove_telegram() -> TalonResult<()> {
    use talon_channels::TelegramConfig;

    TelegramConfig::delete_token().map_err(|e| talon_core::TalonError::Config {
        message: e.to_string(),
    })?;

    println!("Telegram bot token removed from OS keyring");
    Ok(())
}

async fn list_channels() -> TalonResult<()> {
    use std::path::Path;

    println!("Configured channels (in /etc/credstore.encrypted/):");
    println!();

    // Check Telegram
    let telegram_cred = Path::new("/etc/credstore.encrypted/telegram-token");
    if telegram_cred.exists() {
        println!("  telegram: configured");
    } else {
        println!("  telegram: not configured");
    }

    // Discord placeholder
    println!("  discord: not implemented");

    Ok(())
}
