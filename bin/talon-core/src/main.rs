//! Talon Daemon - Core background service
//!
//! The daemon manages conversations and provides the IPC endpoint
//! for channel binaries to connect to.

use tracing_subscriber::EnvFilter;

use talon_core::{TalonConfig, TalonResult};

#[tokio::main]
async fn main() -> TalonResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Talon daemon starting");

    // Load configuration
    let config = TalonConfig::load()?;
    tracing::info!(socket = %config.core.socket_path, "Configuration loaded");

    // Stub: Will initialize acton-reactive runtime and start actors
    tracing::info!("Daemon initialization not yet implemented");

    Ok(())
}
