use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::SessionKey;

/// A message arriving from a channel into the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub session_key: SessionKey,
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub timestamp: DateTime<Utc>,
    /// Whether the client wants streaming tokens.
    #[serde(default)]
    pub stream: bool,
}

/// A response from the gateway back to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub session_key: SessionKey,
    pub text: String,
    pub token_count: u32,
    pub model: String,
    pub timestamp: DateTime<Utc>,
}

/// File or media attachment on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// A streaming token chunk sent during inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamChunk {
    Token(String),
    ToolCall { name: String, arguments: String },
    Done(OutboundMessage),
    Error(String),
}

/// Simple chat request for the HTTP API.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub stream: bool,
}

/// Simple chat response from the HTTP API.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub text: String,
    pub session_id: String,
    pub model: String,
    pub token_count: u32,
}
