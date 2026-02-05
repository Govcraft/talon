//! Telegram message handlers using teloxide 0.17 dptree pattern
//!
//! Defines the update handler tree for processing incoming Telegram messages.

use std::sync::Arc;
use talon_core::ChannelId;
use teloxide::dispatching::{DpHandlerDescription, UpdateFilterExt};
use teloxide::dptree::Handler;
use teloxide::prelude::*;
use teloxide::types::Message;
use tokio::sync::mpsc;

use crate::channel::{InboundMessage, MessageContent};
use crate::telegram::mapping::IdMapper;

/// Handler error type
pub type HandlerError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Handler result type
pub type HandlerResult = Result<(), HandlerError>;

/// Update handler type alias matching teloxide 0.17
/// Handler<'static, Result<(), Err>, DpHandlerDescription>
pub type UpdateHandler = Handler<'static, HandlerResult, DpHandlerDescription>;

/// Build the main message handler using teloxide 0.17 dptree pattern
///
/// This creates a handler tree that processes incoming Telegram updates
/// and converts them to `InboundMessage` for the Talon core.
#[must_use]
pub fn build_message_handler() -> UpdateHandler {
    Update::filter_message()
        .filter(|msg: Message| msg.text().is_some())
        .endpoint(handle_text_message)
}

/// Handle incoming text messages
///
/// Converts a Telegram message to an `InboundMessage` and sends it
/// through the provided channel sender.
async fn handle_text_message(
    _bot: Bot,
    msg: Message,
    sender: mpsc::Sender<InboundMessage>,
    id_mapper: Arc<IdMapper>,
    channel_id: ChannelId,
) -> HandlerResult {
    // Extract text content
    let text = msg.text().ok_or("message has no text")?;

    // Extract sender info
    let user = msg.from.as_ref().ok_or("message has no sender")?;

    // Map IDs
    let conversation_id = id_mapper.get_or_create_conversation(msg.chat.id);
    let sender_id = IdMapper::user_to_sender(&channel_id, user);

    tracing::debug!(
        conversation = %conversation_id,
        user_id = %sender_id.user_id,
        display_name = ?sender_id.display_name,
        text_length = text.len(),
        "received message"
    );

    // Create and send inbound message
    let inbound = InboundMessage::new(conversation_id, sender_id, MessageContent::text(text));

    sender.send(inbound).await.map_err(|e| {
        tracing::error!(error = %e, "failed to send inbound message to channel");
        Box::new(e) as HandlerError
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_message_handler_creates_handler() {
        // Smoke test - just verify it builds without panic
        let _ = build_message_handler();
    }
}
