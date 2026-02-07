//! Conversation management
//!
//! Each conversation is managed by its own actor, maintaining state
//! across multiple turns and handling tool calls. Messages are persisted
//! to the MemoryStore for durability across restarts.

mod actor;
pub mod messages;

pub use actor::{ConversationActor, spawn_conversation};
pub use messages::{
    ConversationResponse, ConversationUserMessage, EndConversation, SetupConversation,
};
