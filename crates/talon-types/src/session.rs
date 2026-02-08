use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AgentId, SessionId, SessionKey};

/// Status of a conversation session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    #[default]
    Active,
    Idle,
    Closed,
}

/// A conversation session between a sender and an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub session_key: SessionKey,
    pub agent_id: Option<AgentId>,
    pub status: SessionStatus,
    pub message_count: u32,
    pub total_tokens: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    pub fn new(session_key: SessionKey) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::generate(),
            session_key,
            agent_id: None,
            status: SessionStatus::Active,
            message_count: 0,
            total_tokens: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub session_id: SessionId,
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<serde_json::Value>,
    pub token_count: u32,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Role of a message in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}
