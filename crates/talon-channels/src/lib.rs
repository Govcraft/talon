//! Talon Channels - Multi-platform communication adapters
//!
//! This crate provides channel implementations for various platforms:
//! - Terminal (TUI) - enabled by default
//! - Telegram - requires "telegram" feature
//! - Discord - requires "discord" feature

pub mod channel;
pub mod error;

#[cfg(feature = "terminal")]
pub mod terminal;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

pub use channel::Channel;
pub use error::{ChannelError, ChannelResult};
