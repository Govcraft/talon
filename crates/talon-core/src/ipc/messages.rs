//! IPC message definitions

use serde::{Deserialize, Serialize};

use crate::types::{ChannelId, ConversationId, CorrelationId, SenderId};

/// Message from channel to core
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChannelToCore {
    /// New user message
    UserMessage {
        /// Correlation ID for tracking
        correlation_id: CorrelationId,
        /// Conversation ID (boxed to reduce enum size)
        conversation_id: Box<ConversationId>,
        /// Sender identity (boxed to reduce enum size)
        sender: Box<SenderId>,
        /// Message content
        content: String,
    },
    /// Channel registration
    Register {
        /// Channel identifier
        channel_id: ChannelId,
    },
    /// Channel disconnecting
    Disconnect {
        /// Channel identifier
        channel_id: ChannelId,
    },
    /// Channel authentication request
    Authenticate {
        /// Channel identifier
        channel_id: ChannelId,
        /// Authentication token
        token: String,
    },
}

/// Message from core to channel
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CoreToChannel {
    /// Processing indication - message is being processed by LLM
    Processing {
        /// Correlation ID for tracking
        correlation_id: CorrelationId,
        /// Conversation ID (boxed to reduce enum size)
        conversation_id: Box<ConversationId>,
    },
    /// Streaming token
    Token {
        /// Correlation ID for tracking
        correlation_id: CorrelationId,
        /// Conversation ID (boxed to reduce enum size)
        conversation_id: Box<ConversationId>,
        /// Token content
        token: String,
    },
    /// Stream complete
    Complete {
        /// Correlation ID for tracking
        correlation_id: CorrelationId,
        /// Conversation ID (boxed to reduce enum size)
        conversation_id: Box<ConversationId>,
        /// Full response content
        content: String,
    },
    /// Error occurred
    Error {
        /// Correlation ID for tracking
        correlation_id: CorrelationId,
        /// Error message
        message: String,
    },
    /// Registration acknowledged
    Registered {
        /// Channel identifier
        channel_id: ChannelId,
    },
    /// Authentication result
    AuthenticationResult {
        /// Whether authentication succeeded
        success: bool,
        /// Error message if failed
        error: Option<String>,
    },
}
