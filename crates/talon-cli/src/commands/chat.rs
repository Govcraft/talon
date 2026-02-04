//! Chat command implementation

use clap::Args;
use talon_core::TalonResult;

/// Arguments for the chat command
#[derive(Args, Default)]
pub struct ChatArgs {
    /// Initial message to send
    #[arg(short, long)]
    pub message: Option<String>,

    /// Conversation ID to resume
    #[arg(short, long)]
    pub resume: Option<String>,
}

/// Run the chat command
///
/// # Errors
///
/// Returns error if chat fails
pub async fn chat(args: ChatArgs) -> TalonResult<()> {
    tracing::info!(message = ?args.message, resume = ?args.resume, "Starting chat session");

    // Stub: Will connect to daemon and start terminal channel
    println!("Chat mode not yet implemented");

    Ok(())
}
