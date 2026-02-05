//! ID mapping between Telegram and Talon identifiers
//!
//! Maps Telegram ChatId to Talon ConversationId and provides
//! user identity conversion.

use std::collections::HashMap;
use std::sync::RwLock;
use talon_core::{ChannelId, ConversationId, SenderId};
use teloxide::types::{ChatId, User};

/// Bidirectional mapping between Telegram and Talon identifiers
pub struct IdMapper {
    /// ChatId -> ConversationId mapping
    chat_to_conversation: RwLock<HashMap<ChatId, ConversationId>>,
    /// ConversationId string -> ChatId reverse mapping
    conversation_to_chat: RwLock<HashMap<String, ChatId>>,
}

impl IdMapper {
    /// Create a new ID mapper
    #[must_use]
    pub fn new() -> Self {
        Self {
            chat_to_conversation: RwLock::new(HashMap::new()),
            conversation_to_chat: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a ConversationId for a Telegram ChatId
    ///
    /// If no mapping exists, creates a new ConversationId using UUIDv7.
    /// Subsequent calls with the same ChatId return the same ConversationId.
    #[must_use]
    pub fn get_or_create_conversation(&self, chat_id: ChatId) -> ConversationId {
        // Fast path: check read lock first
        {
            let read_guard = self
                .chat_to_conversation
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            if let Some(conv_id) = read_guard.get(&chat_id) {
                return conv_id.clone();
            }
        }

        // Slow path: acquire write lock and create mapping
        let mut write_guard = self
            .chat_to_conversation
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Double-check after acquiring write lock
        if let Some(conv_id) = write_guard.get(&chat_id) {
            return conv_id.clone();
        }

        // Create new ConversationId (UUIDv7-based TypeID)
        let conv_id = ConversationId::new();

        // Store bidirectional mapping
        write_guard.insert(chat_id, conv_id.clone());
        drop(write_guard);

        let mut reverse_guard = self
            .conversation_to_chat
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reverse_guard.insert(conv_id.to_string(), chat_id);

        conv_id
    }

    /// Look up the ChatId for a ConversationId
    ///
    /// Returns None if no mapping exists (e.g., conversation from different channel).
    #[must_use]
    pub fn get_chat_id(&self, conversation_id: &ConversationId) -> Option<ChatId> {
        let read_guard = self
            .conversation_to_chat
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        read_guard.get(&conversation_id.to_string()).copied()
    }

    /// Convert a Telegram User to a Talon SenderId
    ///
    /// Creates a SenderId with the channel ID, user ID, and display name.
    #[must_use]
    pub fn user_to_sender(channel_id: &ChannelId, user: &User) -> SenderId {
        let display_name = user
            .username
            .clone()
            .unwrap_or_else(|| user.first_name.clone());

        SenderId::new(channel_id.clone(), user.id.0.to_string()).with_display_name(display_name)
    }
}

impl Default for IdMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_create_returns_same_id_for_same_chat() {
        let mapper = IdMapper::new();
        let chat_id = ChatId(12345);

        let conv1 = mapper.get_or_create_conversation(chat_id);
        let conv2 = mapper.get_or_create_conversation(chat_id);

        assert_eq!(conv1.to_string(), conv2.to_string());
    }

    #[test]
    fn get_or_create_returns_different_ids_for_different_chats() {
        let mapper = IdMapper::new();
        let chat1 = ChatId(12345);
        let chat2 = ChatId(67890);

        let conv1 = mapper.get_or_create_conversation(chat1);
        let conv2 = mapper.get_or_create_conversation(chat2);

        assert_ne!(conv1.to_string(), conv2.to_string());
    }

    #[test]
    fn conversation_id_has_correct_prefix() {
        let mapper = IdMapper::new();
        let chat_id = ChatId(12345);

        let conv_id = mapper.get_or_create_conversation(chat_id);

        assert!(conv_id.to_string().starts_with("conv_"));
    }

    #[test]
    fn get_chat_id_returns_correct_mapping() {
        let mapper = IdMapper::new();
        let chat_id = ChatId(12345);

        let conv_id = mapper.get_or_create_conversation(chat_id);
        let retrieved = mapper.get_chat_id(&conv_id);

        assert_eq!(retrieved, Some(chat_id));
    }

    #[test]
    fn get_chat_id_returns_none_for_unknown() {
        let mapper = IdMapper::new();
        let unknown_conv = ConversationId::new();

        let result = mapper.get_chat_id(&unknown_conv);

        assert!(result.is_none());
    }

    #[test]
    fn user_to_sender_uses_username_when_available() {
        let channel_id = ChannelId::new("telegram");
        let user = User {
            id: teloxide::types::UserId(123),
            is_bot: false,
            first_name: "John".to_string(),
            last_name: Some("Doe".to_string()),
            username: Some("johndoe".to_string()),
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        };

        let sender = IdMapper::user_to_sender(&channel_id, &user);

        assert_eq!(sender.user_id, "123");
        assert_eq!(sender.display_name, Some("johndoe".to_string()));
    }

    #[test]
    fn user_to_sender_falls_back_to_first_name() {
        let channel_id = ChannelId::new("telegram");
        let user = User {
            id: teloxide::types::UserId(456),
            is_bot: false,
            first_name: "Jane".to_string(),
            last_name: None,
            username: None,
            language_code: None,
            is_premium: false,
            added_to_attachment_menu: false,
        };

        let sender = IdMapper::user_to_sender(&channel_id, &user);

        assert_eq!(sender.user_id, "456");
        assert_eq!(sender.display_name, Some("Jane".to_string()));
    }
}
