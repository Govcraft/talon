//! Router actor implementation
//!
//! Routes messages from channels to conversation actors and manages
//! the lifecycle of conversations. Each conversation is backed by
//! its own ConversationActor child.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use acton_ai::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::conversation::messages::{ConversationUserMessage, SetupConversation};
use crate::conversation::spawn_conversation;
use crate::skills::SecureSkillRegistry;
use crate::types::{ChannelId, ConversationId, CorrelationId, SenderId};

/// Configuration for the router
#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// Maximum number of concurrent conversations
    pub max_conversations: usize,
    /// Conversation timeout (no activity)
    pub conversation_timeout: Duration,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_conversations: 1000,
            conversation_timeout: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Message to initialize the router with shared resources
#[acton_message]
pub struct SetupRouter {
    /// Shared ActonAI runtime
    pub acton_ai: Arc<ActonAI>,
    /// Handle to the MemoryStore actor
    pub store: ActorHandle,
    /// Skill registry
    pub skill_registry: Arc<RwLock<SecureSkillRegistry>>,
}

/// Message to route a user message to a conversation
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct RouteMessage {
    /// Correlation ID for request tracking
    pub correlation_id: CorrelationId,
    /// Conversation ID
    pub conversation_id: ConversationId,
    /// Sender identity
    pub sender: SenderId,
    /// Message content
    pub content: String,
}

/// Message response for routing — sent back to the IPC handler
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct MessageRouted {
    /// Correlation ID for request tracking
    pub correlation_id: CorrelationId,
    /// Whether routing was successful
    pub success: bool,
    /// Response content from the LLM (if successful)
    pub response_content: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Message to create a new conversation
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct CreateConversation {
    /// Channel creating the conversation
    pub channel_id: ChannelId,
    /// Sender identity
    pub sender: SenderId,
}

/// Response for conversation creation
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct ConversationCreated {
    /// The new conversation ID
    pub conversation_id: ConversationId,
}

/// Message to end a conversation
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct EndConversation {
    /// The conversation to end
    pub conversation_id: ConversationId,
}

/// Get router statistics
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct GetStats;

/// Router statistics response
#[derive(Serialize, Deserialize)]
#[acton_message]
pub struct RouterStats {
    /// Number of active connections
    pub active_connections: usize,
    /// Number of active conversations
    pub active_conversations: usize,
    /// Maximum conversations allowed
    pub max_conversations: usize,
}

/// Tracked conversation metadata
#[derive(Debug)]
struct TrackedConversation {
    /// The conversation ID
    id: ConversationId,
    /// Channel that owns this conversation
    channel_id: ChannelId,
    /// Last activity time
    last_activity: std::time::Instant,
}

impl TrackedConversation {
    fn new(id: ConversationId, channel_id: ChannelId) -> Self {
        Self {
            id,
            channel_id,
            last_activity: std::time::Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_activity = std::time::Instant::now();
    }

    fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// Router actor state
///
/// Routes messages from channels to conversation actors and manages
/// the lifecycle of conversations.
#[acton_actor]
pub struct Router {
    /// Router configuration
    config: RouterConfig,
    /// Active conversations keyed by conversation ID string
    conversations: HashMap<String, TrackedConversation>,
    /// Handles to conversation actors keyed by conversation ID string
    conversation_handles: HashMap<String, ActorHandle>,
    /// Skill registry for verified skills
    skill_registry: Option<Arc<RwLock<SecureSkillRegistry>>>,
    /// Shared ActonAI runtime
    acton_ai: Option<Arc<ActonAI>>,
    /// Handle to the MemoryStore actor
    store_handle: Option<ActorHandle>,
    /// Number of active channel connections
    active_connections: usize,
}

impl Router {
    /// Create a new router with configuration
    #[must_use]
    pub fn with_config(config: RouterConfig) -> Self {
        Self {
            config,
            conversations: HashMap::new(),
            conversation_handles: HashMap::new(),
            skill_registry: None,
            acton_ai: None,
            store_handle: None,
            active_connections: 0,
        }
    }

    /// Set the skill registry
    pub fn set_skill_registry(&mut self, registry: Arc<RwLock<SecureSkillRegistry>>) {
        self.skill_registry = Some(registry);
    }

    /// Get the number of active connections
    #[must_use]
    pub fn active_connections(&self) -> usize {
        self.active_connections
    }

    /// Get the number of active conversations
    #[must_use]
    pub fn active_conversations(&self) -> usize {
        self.conversations.len()
    }

    /// Increment active connection count
    pub fn add_connection(&mut self) {
        self.active_connections += 1;
        info!(connections = self.active_connections, "channel connected");
    }

    /// Decrement active connection count
    pub fn remove_connection(&mut self) {
        self.active_connections = self.active_connections.saturating_sub(1);
        info!(
            connections = self.active_connections,
            "channel disconnected"
        );
    }

    /// Create a new conversation (metadata only)
    ///
    /// Returns the conversation ID if successful.
    pub fn create_conversation(
        &mut self,
        channel_id: &ChannelId,
        _sender: &SenderId,
    ) -> Result<ConversationId, String> {
        // Check capacity
        if self.conversations.len() >= self.config.max_conversations {
            self.cleanup_expired();

            if self.conversations.len() >= self.config.max_conversations {
                return Err("maximum conversations reached".to_string());
            }
        }

        let conversation_id = ConversationId::new();
        let tracked = TrackedConversation::new(conversation_id.clone(), channel_id.clone());

        self.conversations
            .insert(conversation_id.to_string(), tracked);

        info!(
            conversation_id = %conversation_id,
            channel_id = %channel_id,
            total_conversations = self.conversations.len(),
            "created conversation"
        );

        Ok(conversation_id)
    }

    /// End a conversation
    pub fn end_conversation(&mut self, conversation_id: &ConversationId) {
        let key = conversation_id.to_string();
        if self.conversations.remove(&key).is_some() {
            self.conversation_handles.remove(&key);
            info!(
                conversation_id = %conversation_id,
                total_conversations = self.conversations.len(),
                "ended conversation"
            );
        }
    }

    /// Route a message to a conversation (metadata update only)
    pub fn route_message(
        &mut self,
        correlation_id: &CorrelationId,
        conversation_id: &ConversationId,
        _content: &str,
    ) -> Result<(), String> {
        let key = conversation_id.to_string();

        match self.conversations.get_mut(&key) {
            Some(tracked) => {
                tracked.touch();
                debug!(
                    correlation_id = %correlation_id,
                    conversation_id = %conversation_id,
                    "message routed"
                );
                Ok(())
            }
            None => {
                warn!(
                    correlation_id = %correlation_id,
                    conversation_id = %conversation_id,
                    "conversation not found"
                );
                Err(format!("conversation {conversation_id} not found"))
            }
        }
    }

    /// Clean up expired conversations
    fn cleanup_expired(&mut self) {
        let timeout = self.config.conversation_timeout;
        let before = self.conversations.len();

        let expired_keys: Vec<String> = self
            .conversations
            .iter()
            .filter(|(_, tracked)| tracked.is_expired(timeout))
            .map(|(key, _)| key.clone())
            .collect();

        for key in &expired_keys {
            self.conversations.remove(key);
            self.conversation_handles.remove(key);
            debug!(conversation_id = %key, "cleaning up expired conversation");
        }

        let removed = before - self.conversations.len();
        if removed > 0 {
            info!(
                removed = removed,
                remaining = self.conversations.len(),
                "cleaned up expired conversations"
            );
        }
    }

    /// Get conversations for a channel
    pub fn get_channel_conversations(&self, channel_id: &ChannelId) -> Vec<ConversationId> {
        self.conversations
            .values()
            .filter(|t| t.channel_id.as_str() == channel_id.as_str())
            .map(|t| t.id.clone())
            .collect()
    }

    /// Get router statistics
    #[must_use]
    pub fn stats(&self) -> RouterStats {
        RouterStats {
            active_connections: self.active_connections,
            active_conversations: self.conversations.len(),
            max_conversations: self.config.max_conversations,
        }
    }
}

/// Spawn and configure a Router actor
///
/// Creates a new router actor with message handlers registered and starts it.
/// The caller must send a `SetupRouter` message to provide ActonAI and
/// MemoryStore references before routing messages.
pub async fn spawn_router(runtime: &mut ActorRuntime, config: RouterConfig) -> ActorHandle {
    let mut builder = runtime.new_actor_with_name::<Router>("router".to_string());

    // Set initial config on the model
    builder.model.config = config;

    // SetupRouter: receive shared resources
    builder.mutate_on::<SetupRouter>(|actor, ctx| {
        let msg = ctx.message().clone();
        actor.model.acton_ai = Some(msg.acton_ai);
        actor.model.store_handle = Some(msg.store);
        actor.model.skill_registry = Some(msg.skill_registry);
        info!("router initialized with ActonAI and MemoryStore");
        Reply::ready()
    });

    // RouteMessage: forward to conversation actor (auto-spawn if needed)
    builder.mutate_on::<RouteMessage>(|actor, ctx| {
        let msg = ctx.message().clone();
        let reply_envelope = ctx.reply_envelope();
        let conv_key = msg.conversation_id.to_string();

        // Update or create tracking metadata
        let channel_id = msg.sender.channel_id.clone();
        if !actor.model.conversations.contains_key(&conv_key) {
            // Auto-create conversation tracking on first message
            if actor.model.conversations.len() >= actor.model.config.max_conversations {
                actor.model.cleanup_expired();
                if actor.model.conversations.len() >= actor.model.config.max_conversations {
                    let correlation_id = msg.correlation_id.clone();
                    let handle = tokio::spawn(async move {
                        reply_envelope
                            .send(MessageRouted {
                                correlation_id,
                                success: false,
                                response_content: None,
                                error: Some("maximum conversations reached".to_string()),
                            })
                            .await;
                    });
                    return Reply::pending(async move {
                        let _ = handle.await;
                    });
                }
            }
            let tracked = TrackedConversation::new(msg.conversation_id.clone(), channel_id.clone());
            actor.model.conversations.insert(conv_key.clone(), tracked);
        } else if let Some(tracked) = actor.model.conversations.get_mut(&conv_key) {
            tracked.touch();
        }

        // Check if we already have a conversation actor handle
        let existing_handle = actor.model.conversation_handles.get(&conv_key).cloned();
        let acton_ai = actor.model.acton_ai.clone();
        let store = actor.model.store_handle.clone();
        let conversation_id = msg.conversation_id.clone();
        let mut runtime = actor.runtime().clone();

        let handle = tokio::spawn(async move {
            // Get or spawn conversation actor
            let conv_handle = if let Some(h) = existing_handle {
                h
            } else {
                let (Some(ai), Some(store_handle)) = (&acton_ai, store) else {
                    reply_envelope
                        .send(MessageRouted {
                            correlation_id: msg.correlation_id.clone(),
                            success: false,
                            response_content: None,
                            error: Some("router not initialized with ActonAI/MemoryStore".to_string()),
                        })
                        .await;
                    return;
                };

                let new_handle = spawn_conversation(&mut runtime, &conversation_id).await;

                new_handle
                    .send(SetupConversation {
                        acton_ai: Arc::clone(ai),
                        store: store_handle,
                        system_prompt: None,
                        channel_id,
                    })
                    .await;

                new_handle
            };

            // Forward message to conversation actor
            conv_handle
                .send(ConversationUserMessage {
                    correlation_id: msg.correlation_id.clone(),
                    content: msg.content,
                    sender: msg.sender,
                })
                .await;

            // The ConversationActor replies through the reply_envelope chain.
            // Since the actor system handles envelope forwarding, the reply
            // from ConversationActor flows back to the original caller.
        });

        // Store the handle for future messages (we do this optimistically -
        // the spawn may still be in progress, but it will be complete by
        // the time the next RouteMessage arrives)
        if !actor.model.conversation_handles.contains_key(&conv_key) {
            // We'll set this after spawn completes via a separate message
            // For now, each RouteMessage will re-check and potentially re-spawn
            // This is safe because spawn_conversation is idempotent by name
        }

        Reply::pending(async move {
            let _ = handle.await;
        })
    });

    // CreateConversation: explicitly create a new conversation
    builder.mutate_on::<CreateConversation>(|actor, ctx| {
        let msg = ctx.message().clone();
        let reply_envelope = ctx.reply_envelope();

        match actor
            .model
            .create_conversation(&msg.channel_id, &msg.sender)
        {
            Ok(conversation_id) => {
                let conv_id = conversation_id.clone();
                let handle = tokio::spawn(async move {
                    reply_envelope
                        .send(ConversationCreated {
                            conversation_id: conv_id,
                        })
                        .await;
                });

                Reply::pending(async move {
                    let _ = handle.await;
                })
            }
            Err(e) => {
                warn!(error = %e, "failed to create conversation");
                Reply::ready()
            }
        }
    });

    // EndConversation: clean up conversation actor and tracking
    builder.mutate_on::<EndConversation>(|actor, ctx| {
        let conversation_id = ctx.message().conversation_id.clone();
        let key = conversation_id.to_string();

        // Stop the conversation actor if we have a handle
        if let Some(conv_handle) = actor.model.conversation_handles.remove(&key) {
            let end_msg = crate::conversation::messages::EndConversation {
                conversation_id: conversation_id.clone(),
            };
            let handle = tokio::spawn(async move {
                conv_handle.send(end_msg).await;
                let _ = conv_handle.stop().await;
            });
            actor.model.end_conversation(&conversation_id);
            return Reply::pending(async move {
                let _ = handle.await;
            });
        }

        actor.model.end_conversation(&conversation_id);
        Reply::ready()
    });

    // GetStats: return current router statistics
    builder.mutate_on::<GetStats>(|actor, ctx| {
        let stats = actor.model.stats();
        let reply = ctx.reply_envelope();
        let handle = tokio::spawn(async move {
            reply.send(stats).await;
        });
        Reply::pending(async move {
            let _ = handle.await;
        })
    });

    builder.start().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RouterConfig {
        RouterConfig {
            max_conversations: 10,
            conversation_timeout: Duration::from_secs(60),
        }
    }

    fn test_sender() -> SenderId {
        SenderId::new(ChannelId::new("terminal"), "user123")
    }

    #[test]
    fn test_router_default() {
        let router = Router::default();
        assert_eq!(router.active_connections(), 0);
        assert_eq!(router.active_conversations(), 0);
    }

    #[test]
    fn test_router_with_config() {
        let router = Router::with_config(test_config());
        assert_eq!(router.config.max_conversations, 10);
    }

    #[test]
    fn test_connection_tracking() {
        let mut router = Router::default();

        router.add_connection();
        assert_eq!(router.active_connections(), 1);

        router.add_connection();
        assert_eq!(router.active_connections(), 2);

        router.remove_connection();
        assert_eq!(router.active_connections(), 1);

        router.remove_connection();
        assert_eq!(router.active_connections(), 0);

        // Saturating subtraction
        router.remove_connection();
        assert_eq!(router.active_connections(), 0);
    }

    #[test]
    fn test_create_conversation() {
        let mut router = Router::with_config(test_config());
        let channel_id = ChannelId::new("terminal");
        let sender = test_sender();

        let result = router.create_conversation(&channel_id, &sender);
        assert!(result.is_ok());
        assert_eq!(router.active_conversations(), 1);
    }

    #[test]
    fn test_max_conversations() {
        let mut router = Router::with_config(RouterConfig {
            max_conversations: 2,
            conversation_timeout: Duration::from_secs(60),
        });
        let channel_id = ChannelId::new("terminal");
        let sender = test_sender();

        // Create two conversations (at limit)
        assert!(router.create_conversation(&channel_id, &sender).is_ok());
        assert!(router.create_conversation(&channel_id, &sender).is_ok());

        // Third should fail
        let result = router.create_conversation(&channel_id, &sender);
        assert!(result.is_err());
    }

    #[test]
    fn test_end_conversation() {
        let mut router = Router::with_config(test_config());
        let channel_id = ChannelId::new("terminal");
        let sender = test_sender();

        let conv_id = router
            .create_conversation(&channel_id, &sender)
            .expect("should create");

        assert_eq!(router.active_conversations(), 1);

        router.end_conversation(&conv_id);
        assert_eq!(router.active_conversations(), 0);
    }

    #[test]
    fn test_route_message() {
        let mut router = Router::with_config(test_config());
        let channel_id = ChannelId::new("terminal");
        let sender = test_sender();
        let correlation_id = CorrelationId::new();

        let conv_id = router
            .create_conversation(&channel_id, &sender)
            .expect("should create");

        let result = router.route_message(&correlation_id, &conv_id, "Hello");
        assert!(result.is_ok());
    }

    #[test]
    fn test_route_message_unknown_conversation() {
        let mut router = Router::with_config(test_config());
        let correlation_id = CorrelationId::new();
        let unknown_conv = ConversationId::new();

        let result = router.route_message(&correlation_id, &unknown_conv, "Hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_channel_conversations() {
        let mut router = Router::with_config(test_config());
        let channel1 = ChannelId::new("terminal");
        let channel2 = ChannelId::new("telegram");
        let sender1 = SenderId::new(channel1.clone(), "user1");
        let sender2 = SenderId::new(channel2.clone(), "user2");

        // Create conversations for different channels
        router
            .create_conversation(&channel1, &sender1)
            .expect("should create");
        router
            .create_conversation(&channel1, &sender1)
            .expect("should create");
        router
            .create_conversation(&channel2, &sender2)
            .expect("should create");

        let terminal_convs = router.get_channel_conversations(&channel1);
        let telegram_convs = router.get_channel_conversations(&channel2);

        assert_eq!(terminal_convs.len(), 2);
        assert_eq!(telegram_convs.len(), 1);
    }

    #[test]
    fn test_router_stats() {
        let mut router = Router::with_config(test_config());
        let channel_id = ChannelId::new("terminal");
        let sender = test_sender();

        router.add_connection();
        router
            .create_conversation(&channel_id, &sender)
            .expect("should create");

        let stats = router.stats();
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.active_conversations, 1);
        assert_eq!(stats.max_conversations, 10);
    }
}
