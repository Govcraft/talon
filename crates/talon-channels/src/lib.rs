//! Talon Channels - Multi-platform communication adapters
//!
//! This crate provides channel implementations for various platforms:
//! - Terminal (TUI) - enabled by default
//! - Telegram - requires "telegram" feature
//! - Discord - requires "discord" feature
//!
//! # IPC Client
//!
//! All channels communicate with the core daemon via the IPC client module.
//! See [`ipc`] for details.

pub mod channel;
pub mod error;
pub mod ipc;

#[cfg(feature = "terminal")]
pub mod terminal;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

pub use channel::{Channel, InboundMessage, MessageContent, OutboundMessage};
pub use error::{ChannelError, ChannelResult};

// Re-export telegram types when feature is enabled
#[cfg(feature = "telegram")]
pub use telegram::{TelegramChannel, TelegramConfig, TelegramConfigError};
