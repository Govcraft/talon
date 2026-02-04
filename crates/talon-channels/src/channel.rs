//! Core Channel trait

use async_trait::async_trait;
use talon_core::{ChannelId, ConversationId, SenderId};
use tokio::sync::mpsc;

use crate::error::ChannelResult;

/// Message content variants
#[derive(Clone, Debug)]
pub enum MessageContent {
    /// Plain text message
    Text(String),
    /// Markdown-formatted message
    Markdown(String),
    /// Image with optional caption
    Image {
        /// Image URL
        url: String,
        /// Optional caption
        caption: Option<String>,
    },
}

impl MessageContent {
    /// Create a text message
    #[must_use]
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    /// Create a markdown message
    #[must_use]
    pub fn markdown(content: impl Into<String>) -> Self {
        Self::Markdown(content.into())
    }

    /// Get the text content regardless of variant
    #[must_use]
    pub fn as_text(&self) -> &str {
        match self {
            Self::Text(s) | Self::Markdown(s) => s,
            Self::Image { caption, .. } => caption.as_deref().unwrap_or(""),
        }
    }
}

/// Inbound message from user
#[derive(Clone, Debug)]
pub struct InboundMessage {
    /// Conversation identifier
    pub conversation_id: ConversationId,
    /// Sender identity
    pub sender: SenderId,
    /// Message content
    pub content: MessageContent,
    /// Message timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl InboundMessage {
    /// Create a new inbound message
    #[must_use]
    pub fn new(
        conversation_id: ConversationId,
        sender: SenderId,
        content: MessageContent,
    ) -> Self {
        Self {
            conversation_id,
            sender,
            content,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Outbound message to user
#[derive(Clone, Debug)]
pub struct OutboundMessage {
    /// Conversation identifier
    pub conversation_id: ConversationId,
    /// Message content
    pub content: MessageContent,
    /// Optional message ID to reply to
    pub reply_to: Option<String>,
}

impl OutboundMessage {
    /// Create a new outbound message
    #[must_use]
    pub fn new(conversation_id: ConversationId, content: MessageContent) -> Self {
        Self {
            conversation_id,
            content,
            reply_to: None,
        }
    }

    /// Set the message to reply to
    #[must_use]
    pub fn with_reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.reply_to = Some(message_id.into());
        self
    }
}

/// Channel trait - implement for each platform
///
/// Channels handle platform-specific communication, translating between
/// the platform's native message format and Talon's internal format.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique channel identifier
    fn id(&self) -> ChannelId;

    /// Start receiving messages
    ///
    /// Spawns a background task that sends incoming messages to the provided sender.
    ///
    /// # Errors
    ///
    /// Returns error if channel fails to start
    async fn start(&self, sender: mpsc::Sender<InboundMessage>) -> ChannelResult<()>;

    /// Send a message to a conversation
    ///
    /// # Errors
    ///
    /// Returns error if message fails to send
    async fn send(&self, message: OutboundMessage) -> ChannelResult<()>;

    /// Send a streaming token
    ///
    /// For channels that support streaming, this sends a partial response.
    /// Channels that don't support streaming may buffer tokens.
    ///
    /// # Errors
    ///
    /// Returns error if token fails to send
    async fn send_token(&self, conversation_id: &ConversationId, token: &str) -> ChannelResult<()>;

    /// Stop the channel
    ///
    /// # Errors
    ///
    /// Returns error if channel fails to stop cleanly
    async fn stop(&self) -> ChannelResult<()>;
}
