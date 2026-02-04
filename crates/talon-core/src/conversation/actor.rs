//! Conversation actor implementation

use acton_reactive::prelude::*;

use crate::types::{ConversationId, SenderId};

/// Conversation actor state
///
/// Each conversation is managed by its own actor instance.
#[acton_actor]
pub struct Conversation {
    /// Unique conversation identifier
    id: ConversationId,
    /// Sender identity
    sender: Option<SenderId>,
    /// Conversation turn count
    turn_count: usize,
}

impl Conversation {
    /// Create a new conversation with a specific ID
    #[must_use]
    pub fn with_id(id: ConversationId) -> Self {
        Self {
            id,
            sender: None,
            turn_count: 0,
        }
    }

    /// Get the conversation ID
    #[must_use]
    pub fn id(&self) -> &ConversationId {
        &self.id
    }

    /// Get the sender if set
    #[must_use]
    pub fn sender(&self) -> Option<&SenderId> {
        self.sender.as_ref()
    }

    /// Set the sender
    pub fn set_sender(&mut self, sender: SenderId) {
        self.sender = Some(sender);
    }

    /// Get the turn count
    #[must_use]
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// Increment the turn count
    pub fn increment_turn(&mut self) {
        self.turn_count += 1;
    }
}
