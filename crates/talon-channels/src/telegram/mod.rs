//! Telegram channel using teloxide
//!
//! Provides a Telegram bot interface for Talon. The channel receives messages
//! from Telegram users and forwards them to talon-core via IPC, streaming
//! responses back with progressive message editing.
//!
//! # Security
//!
//! Bot tokens are stored in the OS keyring (macOS Keychain, Linux Secret Service,
//! or Windows Credential Manager). Tokens are encrypted at rest and require
//! user authentication to access.
//!
//! # Example
//!
//! ```ignore
//! use talon_channels::telegram::{TelegramChannel, TelegramConfig};
//!
//! // Load config from OS keyring
//! let channel = TelegramChannel::from_env()?;
//!
//! // Start receiving messages
//! let (tx, rx) = tokio::sync::mpsc::channel(100);
//! channel.start(tx).await?;
//! ```

mod channel;
mod config;
mod handlers;
mod mapping;
mod streaming;

pub use channel::TelegramChannel;
pub use config::{TelegramConfig, TelegramConfigError};
pub use mapping::IdMapper;
pub use streaming::{split_message, StreamingManager, TELEGRAM_MAX_MESSAGE_LENGTH};
