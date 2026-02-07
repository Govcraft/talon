//! Talon Daemon - Core background service
//!
//! The daemon manages conversations and provides the IPC endpoint
//! for channel binaries to connect to.

use tracing_subscriber::EnvFilter;

use talon_core::{ChannelId, RuntimeConfigBuilder, TalonResult, TalonRuntime};

#[tokio::main]
async fn main() -> TalonResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Talon daemon starting");

    // Build runtime configuration
    let config = RuntimeConfigBuilder::new().max_conversations(100).build();

    tracing::info!(
        socket_path = %config.ipc_socket_path.display(),
        max_conversations = config.max_conversations,
        "Configuration loaded"
    );

    // Create and start the runtime
    let mut runtime = TalonRuntime::new(config).await?;

    // Start IPC server
    runtime.start_ipc().await?;

    // Print development tokens for testing
    let telegram_token = runtime.issue_channel_token(&ChannelId::new("telegram"));
    tracing::info!("=== DEVELOPMENT TOKENS (for testing) ===");
    tracing::info!("Telegram IPC token: {}", telegram_token);
    tracing::info!("Set with: export TALON_IPC_TOKEN=\"{}\"", telegram_token);
    tracing::info!("=========================================");

    tracing::info!("Talon daemon is running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| talon_core::TalonError::Io {
            message: format!("failed to listen for ctrl-c: {e}"),
        })?;

    tracing::info!("Shutdown signal received");

    // Graceful shutdown
    runtime.shutdown().await?;

    tracing::info!("Talon daemon stopped");
    Ok(())
}
