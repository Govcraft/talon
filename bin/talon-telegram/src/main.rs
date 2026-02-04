//! Talon Telegram Bot
//!
//! Telegram channel binary that connects to the core daemon via IPC.

use tracing_subscriber::EnvFilter;

use talon_core::TalonResult;

#[tokio::main]
async fn main() -> TalonResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Talon Telegram bot starting");

    // Stub: Will connect to daemon and start Telegram bot
    tracing::info!("Telegram bot not yet implemented");

    Ok(())
}
