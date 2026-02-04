//! Terminal channel using ratatui TUI
//!
//! Provides a terminal-based interface for local interaction.

#[cfg(feature = "terminal")]
mod ui;

#[cfg(feature = "terminal")]
pub use ui::TerminalChannel;
