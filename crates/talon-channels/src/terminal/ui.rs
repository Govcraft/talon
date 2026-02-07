//! Terminal UI implementation

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use talon_core::{ChannelId, ConversationId};
use tokio::sync::mpsc;

use crate::channel::{Channel, InboundMessage, OutboundMessage};
use crate::error::{ChannelError, ChannelResult};

/// Terminal channel for local TUI interface
pub struct TerminalChannel {
    id: ChannelId,
    running: Arc<AtomicBool>,
}

impl TerminalChannel {
    /// Create a new terminal channel
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: ChannelId::new("terminal"),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for TerminalChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for TerminalChannel {
    fn id(&self) -> ChannelId {
        self.id.clone()
    }

    async fn start(&self, _sender: mpsc::Sender<InboundMessage>) -> ChannelResult<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(ChannelError::AlreadyStarted);
        }

        // Stub: Would initialize ratatui terminal here
        tracing::info!("Terminal channel started");
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> ChannelResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        // Stub: Would render message to terminal
        tracing::debug!(
            conversation = %message.conversation_id,
            content = message.content.as_text(),
            "Sending message"
        );
        Ok(())
    }

    async fn send_token(&self, conversation_id: &ConversationId, token: &str) -> ChannelResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        // Stub: Would append token to current output
        tracing::trace!(
            conversation = %conversation_id,
            token = token,
            "Streaming token"
        );
        Ok(())
    }

    async fn stop(&self) -> ChannelResult<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        // Stub: Would restore terminal
        tracing::info!("Terminal channel stopped");
        Ok(())
    }
}
