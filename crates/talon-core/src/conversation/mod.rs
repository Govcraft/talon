//! Conversation management
//!
//! Each conversation is managed by its own actor, maintaining state
//! across multiple turns and handling tool calls.

mod actor;

pub use actor::Conversation;
