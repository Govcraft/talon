//! Streaming token support for Telegram
//!
//! Buffers incoming tokens and periodically edits the Telegram message
//! to show progressive responses. Uses debouncing to respect rate limits.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use talon_core::ConversationId;
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId, ParseMode};

use crate::error::{ChannelError, ChannelResult};
use crate::telegram::mapping::IdMapper;

/// Telegram's maximum message length in characters
pub const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;

/// Streaming indicator appended to in-progress messages
const STREAMING_INDICATOR: &str = "...";

/// State for an active streaming response
#[derive(Debug)]
struct StreamState {
    /// Telegram message ID being edited
    message_id: MessageId,
    /// Chat ID where message is located
    chat_id: ChatId,
    /// Accumulated content buffer
    content: String,
    /// Last time the message was edited
    last_update: Instant,
}

/// Manages streaming responses across multiple conversations
pub struct StreamingManager {
    /// Active streams by conversation ID string
    streams: HashMap<String, StreamState>,
    /// Minimum time between message edits
    debounce_interval: Duration,
}

impl StreamingManager {
    /// Create a new streaming manager with the given debounce interval
    #[must_use]
    pub fn new(debounce_interval: Duration) -> Self {
        Self {
            streams: HashMap::new(),
            debounce_interval,
        }
    }

    /// Append a token to the stream for a conversation
    ///
    /// Creates a new message if no stream exists, or edits the existing
    /// message if the debounce interval has elapsed.
    ///
    /// # Errors
    ///
    /// Returns error if Telegram API calls fail or if ChatId mapping is missing.
    pub async fn append_token(
        &mut self,
        conversation_id: &ConversationId,
        token: &str,
        bot: &Bot,
        id_mapper: &IdMapper,
    ) -> ChannelResult<()> {
        let conv_key = conversation_id.to_string();

        // Check if stream exists and needs update
        if let Some(state) = self.streams.get_mut(&conv_key) {
            state.content.push_str(token);

            // Check if we should update the message
            let elapsed = state.last_update.elapsed();
            if elapsed >= self.debounce_interval {
                // Extract data needed for update (copy to avoid borrow conflict)
                let content_clone = state.content.clone();
                let display_content = format_streaming_content(&content_clone);
                let chat_id = state.chat_id;
                let message_id = state.message_id;

                bot.edit_message_text(chat_id, message_id, display_content)
                    .await
                    .map_err(map_teloxide_error)?;

                state.last_update = Instant::now();
            }
            return Ok(());
        }

        // No existing stream - create a new message
        let chat_id = id_mapper.get_chat_id(conversation_id).ok_or_else(|| {
            ChannelError::Send {
                message: format!("no chat mapping for conversation {}", conversation_id),
            }
        })?;

        // Send initial message with token
        let initial_content = format!("{token}{STREAMING_INDICATOR}");
        let sent = bot
            .send_message(chat_id, &initial_content)
            .await
            .map_err(map_teloxide_error)?;

        let state = StreamState {
            message_id: sent.id,
            chat_id,
            content: token.to_string(),
            last_update: Instant::now(),
        };

        self.streams.insert(conv_key, state);
        Ok(())
    }


    /// Finalize a stream, sending the complete message without indicator
    ///
    /// # Errors
    ///
    /// Returns error if the Telegram API call fails.
    pub async fn finalize(
        &mut self,
        conversation_id: &ConversationId,
        bot: &Bot,
    ) -> ChannelResult<()> {
        let conv_key = conversation_id.to_string();

        if let Some(mut state) = self.streams.remove(&conv_key) {
            self.finalize_message(bot, &mut state).await?;
        }

        Ok(())
    }

    /// Finalize all active streams (for shutdown)
    ///
    /// # Errors
    ///
    /// Returns error if any Telegram API calls fail.
    pub async fn finalize_all(&mut self, bot: &Bot) -> ChannelResult<()> {
        let keys: Vec<String> = self.streams.keys().cloned().collect();

        for key in keys {
            if let Some(mut state) = self.streams.remove(&key) {
                if let Err(e) = self.finalize_message(bot, &mut state).await {
                    tracing::warn!(conversation = %key, error = %e, "failed to finalize stream");
                }
            }
        }

        Ok(())
    }

    /// Check if a conversation has an active stream
    #[must_use]
    pub fn has_stream(&self, conversation_id: &ConversationId) -> bool {
        self.streams.contains_key(&conversation_id.to_string())
    }

    /// Finalize the message with complete content (no indicator)
    async fn finalize_message(&self, bot: &Bot, state: &mut StreamState) -> ChannelResult<()> {
        // Handle messages that exceed max length
        let messages = split_message(&state.content);

        // Edit first message with first chunk (or full content if no split needed)
        if let Some(first) = messages.first() {
            bot.edit_message_text(state.chat_id, state.message_id, first)
                .await
                .map_err(map_teloxide_error)?;
        }

        // Send additional messages for remaining chunks
        for chunk in messages.iter().skip(1) {
            bot.send_message(state.chat_id, chunk)
                .await
                .map_err(map_teloxide_error)?;
        }

        Ok(())
    }
}

/// Split a message into chunks that fit Telegram's character limit
#[must_use]
pub fn split_message(content: &str) -> Vec<String> {
    if content.len() <= TELEGRAM_MAX_MESSAGE_LENGTH {
        return vec![content.to_string()];
    }

    content
        .chars()
        .collect::<Vec<_>>()
        .chunks(TELEGRAM_MAX_MESSAGE_LENGTH)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Send a message that may need splitting
///
/// # Errors
///
/// Returns error if any Telegram API call fails.
pub async fn send_split_message(
    bot: &Bot,
    chat_id: ChatId,
    content: &str,
    parse_mode: Option<ParseMode>,
) -> ChannelResult<()> {
    let messages = split_message(content);

    for chunk in messages {
        let mut request = bot.send_message(chat_id, chunk);
        if let Some(mode) = parse_mode {
            request = request.parse_mode(mode);
        }
        request.await.map_err(map_teloxide_error)?;
    }

    Ok(())
}

/// Format content for streaming display (with indicator and length limit)
fn format_streaming_content(content: &str) -> String {
    if content.len() + STREAMING_INDICATOR.len() > TELEGRAM_MAX_MESSAGE_LENGTH {
        let max_len = TELEGRAM_MAX_MESSAGE_LENGTH - STREAMING_INDICATOR.len();
        let truncated: String = content.chars().take(max_len).collect();
        format!("{truncated}{STREAMING_INDICATOR}")
    } else {
        format!("{content}{STREAMING_INDICATOR}")
    }
}

/// Map teloxide errors to channel errors
fn map_teloxide_error(e: teloxide::RequestError) -> ChannelError {
    ChannelError::Platform {
        platform: "telegram".to_string(),
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_returns_single_for_short_content() {
        let content = "Hello, world!";
        let result = split_message(content);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], content);
    }

    #[test]
    fn split_message_splits_at_boundary() {
        let content = "a".repeat(TELEGRAM_MAX_MESSAGE_LENGTH + 100);
        let result = split_message(&content);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), TELEGRAM_MAX_MESSAGE_LENGTH);
        assert_eq!(result[1].len(), 100);
    }

    #[test]
    fn split_message_handles_exact_boundary() {
        let content = "a".repeat(TELEGRAM_MAX_MESSAGE_LENGTH);
        let result = split_message(&content);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), TELEGRAM_MAX_MESSAGE_LENGTH);
    }

    #[test]
    fn streaming_manager_default_debounce() {
        let manager = StreamingManager::new(Duration::from_millis(500));
        assert_eq!(manager.debounce_interval, Duration::from_millis(500));
    }
}
