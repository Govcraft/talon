//! Message types for the conversation actor

use acton_reactive::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use acton_ai::prelude::ActonAI;

use crate::types::{ChannelId, ConversationId, CorrelationId, SenderId};

/// Initialization message sent to a newly spawned ConversationActor
#[acton_message]
pub struct SetupConversation {
    /// Shared ActonAI runtime for LLM interaction
    pub acton_ai: Arc<ActonAI>,
    /// Handle to the MemoryStore actor for persistence
    pub store: ActorHandle,
    /// Optional system prompt for this conversation
    pub system_prompt: Option<String>,
    /// Channel this conversation belongs to
    pub channel_id: ChannelId,
}

/// Incoming user message to be processed by the conversation actor
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct ConversationUserMessage {
    /// Correlation ID for request/response tracking
    pub correlation_id: CorrelationId,
    /// Message content from the user
    pub content: String,
    /// Sender identity
    pub sender: SenderId,
}

/// Response from the conversation actor after processing a user message
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct ConversationResponse {
    /// Correlation ID matching the request
    pub correlation_id: CorrelationId,
    /// Response content from the LLM
    pub content: String,
}

/// Message to end a conversation and clean up resources
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct EndConversation {
    /// The conversation to end
    pub conversation_id: ConversationId,
}
