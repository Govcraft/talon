//! Talon Telegram Bot
//!
//! Telegram channel binary that connects to the core daemon via IPC.
//!
//! # Configuration
//!
//! The bot token and IPC auth token are stored using systemd credentials (encrypted at rest):
//!
//! ```bash
//! # Store tokens (requires sudo)
//! sudo talon channel add telegram --token <bot-token>
//! sudo talon channel set-ipc-token telegram --token <ipc-token>
//!
//! # Run the bot
//! cargo run --bin talon-telegram
//! ```
//!
//! The tokens are encrypted using the host key and stored in
//! `/etc/credstore.encrypted/`.

use std::sync::Arc;

use talon_channels::ipc::{IpcClient, IpcClientConfig};
use talon_channels::{Channel, InboundMessage, TelegramChannel};
use talon_core::{ChannelId, TalonError, TalonResult};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{Notify, mpsc};
use tracing_subscriber::EnvFilter;

/// Load IPC auth token from systemd credentials
fn load_ipc_token() -> Result<String, TalonError> {
    // Try systemd credentials first
    if let Ok(creds_dir) = std::env::var("CREDENTIALS_DIRECTORY") {
        let token_path = std::path::Path::new(&creds_dir).join("telegram-ipc-token");
        if token_path.exists() {
            return std::fs::read_to_string(&token_path)
                .map(|s| s.trim().to_string())
                .map_err(|e| TalonError::Config {
                    message: format!("failed to read IPC token: {e}"),
                });
        }
    }

    // Fallback to environment variable for development
    if let Ok(token) = std::env::var("TALON_IPC_TOKEN") {
        return Ok(token);
    }

    // For development: generate a token using the same dev secret as the daemon
    // This allows testing without manual token setup
    tracing::warn!("Using development IPC token - not for production!");
    let dev_secret = b"talon-development-secret-key-32b";
    let authenticator = talon_core::ipc::TokenAuthenticator::new(dev_secret);
    let token = authenticator.issue_token(&ChannelId::new("telegram"));
    Ok(token.to_string())
}

#[tokio::main]
async fn main() -> TalonResult<()> {
    // Initialize tracing (default to info level)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Set up shutdown signal FIRST, before any other async tasks
    // This ensures our handler gets registered before anything else
    let shutdown = Arc::new(Notify::new());
    let shutdown_signal = Arc::clone(&shutdown);

    tokio::spawn(async move {
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "Failed to install SIGINT handler");
                return;
            }
        };
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "Failed to install SIGTERM handler");
                return;
            }
        };

        tokio::select! {
            _ = sigint.recv() => {
                tracing::info!("SIGINT received");
            }
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received");
            }
        }

        shutdown_signal.notify_waiters();
    });

    tracing::info!("Talon Telegram bot starting");

    // Create TelegramChannel from systemd credentials
    let channel = match TelegramChannel::from_env() {
        Ok(ch) => Arc::new(ch),
        Err(e) => {
            tracing::error!(error = %e, "Failed to create Telegram channel");
            tracing::info!(
                "To configure the bot token, run:\n  \
                 sudo talon channel add telegram --token <token>"
            );
            return Err(TalonError::Config {
                message: e.to_string(),
            });
        }
    };

    // Load IPC token
    let ipc_token = match load_ipc_token() {
        Ok(token) => token,
        Err(e) => {
            tracing::error!(error = %e, "Failed to load IPC token");
            tracing::info!(
                "To configure the IPC token, run:\n  \
                 sudo talon channel set-ipc-token telegram --token <token>\n  \
                 Or set TALON_IPC_TOKEN environment variable for development."
            );
            return Err(e);
        }
    };

    // Create IPC client
    let ipc_config = IpcClientConfig::new(ChannelId::new("telegram"), ipc_token);
    let ipc_client = Arc::new(IpcClient::new(ipc_config));

    // Connect to core daemon
    tracing::info!("Connecting to core daemon...");
    if let Err(e) = ipc_client.connect().await {
        tracing::error!(error = %e, "Failed to connect to core daemon");
        tracing::info!(
            "Make sure talon-core daemon is running:\n  \
             sudo cargo run --bin talon-core"
        );
        return Err(TalonError::Ipc {
            message: e.to_string(),
        });
    }

    // Authenticate
    tracing::info!("Authenticating...");
    if let Err(e) = ipc_client.authenticate().await {
        tracing::error!(error = %e, "Authentication failed");
        return Err(TalonError::Ipc {
            message: e.to_string(),
        });
    }

    // Register
    tracing::info!("Registering channel...");
    if let Err(e) = ipc_client.register().await {
        tracing::error!(error = %e, "Registration failed");
        return Err(TalonError::Ipc {
            message: e.to_string(),
        });
    }

    tracing::info!("Connected to core daemon");

    // Set up streaming callbacks
    let channel_for_tokens = Arc::clone(&channel);
    ipc_client
        .set_token_callback(Arc::new(move |conv_id, token| {
            let ch = Arc::clone(&channel_for_tokens);
            tokio::spawn(async move {
                if let Err(e) = ch.send_token(&conv_id, &token).await {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to send token to Telegram"
                    );
                }
            });
        }))
        .await;

    let channel_for_complete = Arc::clone(&channel);
    ipc_client
        .set_complete_callback(Arc::new(move |conv_id, content| {
            let ch = Arc::clone(&channel_for_complete);
            tokio::spawn(async move {
                // Send the final complete message
                let msg = talon_channels::OutboundMessage::new(
                    conv_id.clone(),
                    talon_channels::MessageContent::text(content),
                );
                if let Err(e) = ch.send(msg).await {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to send complete message to Telegram"
                    );
                }
            });
        }))
        .await;

    ipc_client
        .set_error_callback(Arc::new(|corr_id, message| {
            tracing::error!(
                correlation_id = %corr_id,
                error = %message,
                "Error from core"
            );
        }))
        .await;

    // Start the receive loop for streaming responses
    let _receive_handle = ipc_client.start_receive_loop();

    // Create message channel for inbound messages
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(100);

    // Start the Telegram channel
    if let Err(e) = channel.start(inbound_tx).await {
        tracing::error!(error = %e, "Failed to start Telegram channel");
        return Err(TalonError::Channel {
            channel: "telegram".to_string(),
            message: e.to_string(),
        });
    }

    tracing::info!("Telegram bot is running. Press Ctrl+C to stop.");

    // Main event loop - select on shutdown signal or incoming messages
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                tracing::info!("Shutdown signal received, stopping...");
                break;
            }

            Some(inbound) = inbound_rx.recv() => {
                tracing::info!(
                    conversation_id = %inbound.conversation_id,
                    sender = %inbound.sender.user_id,
                    display_name = ?inbound.sender.display_name,
                    content_length = inbound.content.as_text().len(),
                    "Received message"
                );

                // Forward to core via IPC
                match ipc_client.send_message(
                    inbound.conversation_id.clone(),
                    inbound.sender.clone(),
                    inbound.content.as_text().to_string(),
                ).await {
                    Ok(correlation_id) => {
                        tracing::debug!(
                            correlation_id = %correlation_id,
                            conversation_id = %inbound.conversation_id,
                            "Message forwarded to core"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            conversation_id = %inbound.conversation_id,
                            error = %e,
                            "Failed to forward message to core"
                        );
                    }
                }
            }
        }
    }

    // Disconnect from core
    if let Err(e) = ipc_client.disconnect().await {
        tracing::warn!(error = %e, "Error during IPC disconnect");
    }

    // Stop the channel gracefully
    if let Err(e) = channel.stop().await {
        tracing::warn!(error = %e, "Error during channel shutdown");
    }

    tracing::info!("Talon Telegram bot stopped");
    Ok(())
}
