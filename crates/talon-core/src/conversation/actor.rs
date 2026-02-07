//! Conversation actor implementation
//!
//! Each conversation is managed by its own actor instance, holding
//! message history and delegating to ActonAI for LLM processing.
//! Messages are persisted to the MemoryStore actor for durability.

use std::sync::Arc;

use acton_ai::memory::SaveMessage;
use acton_ai::prelude::*;
use tracing::{debug, error, warn};

use crate::conversation::messages::{
    ConversationResponse, ConversationUserMessage, EndConversation, SetupConversation,
};
use crate::types::{ChannelId, ConversationId, SenderId};

/// Conversation actor state
///
/// Each conversation is managed by its own actor instance.
/// State includes the LLM runtime reference, persistent store handle,
/// and in-memory message history.
#[acton_actor]
pub struct ConversationActor {
    /// Unique conversation identifier
    id: ConversationId,
    /// Sender identity
    sender: Option<SenderId>,
    /// Channel this conversation belongs to
    channel_id: ChannelId,
    /// Shared ActonAI runtime for LLM interaction
    acton_ai: Option<Arc<ActonAI>>,
    /// Handle to the MemoryStore actor for persistence
    store: Option<ActorHandle>,
    /// In-memory message history for this conversation
    history: Vec<Message>,
    /// Optional system prompt
    system_prompt: Option<String>,
    /// Conversation turn count
    turn_count: usize,
}

impl ConversationActor {
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

    /// Get the turn count
    #[must_use]
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// Get message history
    #[must_use]
    pub fn history(&self) -> &[Message] {
        &self.history
    }
}

/// Spawn and configure a ConversationActor
///
/// Creates a new actor with message handlers registered and starts it.
/// The caller must send a `SetupConversation` message to initialize
/// the actor with ActonAI and MemoryStore references.
pub async fn spawn_conversation(runtime: &mut ActorRuntime, id: &ConversationId) -> ActorHandle {
    let mut builder =
        runtime.new_actor_with_name::<ConversationActor>(format!("conversation_{id}"));

    // SetupConversation: initialize shared resources
    builder.mutate_on::<SetupConversation>(|actor, ctx| {
        let msg = ctx.message().clone();
        actor.model.acton_ai = Some(msg.acton_ai);
        actor.model.store = Some(msg.store);
        actor.model.system_prompt = msg.system_prompt;
        actor.model.channel_id = msg.channel_id;

        debug!(
            conversation_id = %actor.model.id,
            "conversation actor initialized"
        );

        Reply::ready()
    });

    // ConversationUserMessage: process user input through LLM
    builder.mutate_on::<ConversationUserMessage>(|actor, ctx| {
        let msg = ctx.message().clone();
        let reply_envelope = ctx.reply_envelope();

        // Capture state we need for the async block
        let ai = actor.model.acton_ai.clone();
        let store = actor.model.store.clone();
        let conversation_id = actor.model.id.clone();
        let system_prompt = actor.model.system_prompt.clone();

        // Set sender on first message
        if actor.model.sender.is_none() {
            actor.model.sender = Some(msg.sender.clone());
        }

        // Append user message to history
        let user_message = Message::user(&msg.content);
        actor.model.history.push(user_message.clone());
        actor.model.turn_count += 1;

        // Clone history for the async LLM call
        let history = actor.model.history.clone();
        let correlation_id = msg.correlation_id.clone();

        // We need a handle to send the assistant message back to ourselves
        let self_handle = actor.handle().clone();

        let handle = tokio::spawn(async move {
            // Persist user message to MemoryStore
            if let Some(ref store) = store {
                let store_conv_id = acton_ai::prelude::ConversationId::parse(
                    &conversation_id.to_string(),
                )
                .unwrap_or_default();
                store
                    .send(SaveMessage {
                        conversation_id: store_conv_id,
                        message: user_message,
                    })
                    .await;
            }

            // Process through ActonAI
            let Some(ai) = ai else {
                warn!(
                    conversation_id = %conversation_id,
                    "no ActonAI configured, echoing message"
                );
                let echo_content = format!("Echo (no AI): {}", msg.content);
                reply_envelope
                    .send(ConversationResponse {
                        correlation_id,
                        content: echo_content,
                    })
                    .await;
                return;
            };

            let mut prompt_builder = ai.continue_with(history);
            if let Some(ref system) = system_prompt {
                prompt_builder = prompt_builder.system(system.as_str());
            }

            match prompt_builder.collect().await {
                Ok(response) => {
                    debug!(
                        conversation_id = %conversation_id,
                        response_len = response.text.len(),
                        tool_calls = response.tool_calls.len(),
                        "LLM response received"
                    );

                    let response_text = if response.text.is_empty() {
                        warn!("LLM returned empty response, using fallback");
                        "I received your message but couldn't generate a response. Please try again.".to_string()
                    } else {
                        response.text
                    };

                    let assistant_message = Message::assistant(&response_text);

                    // Persist assistant message to MemoryStore
                    if let Some(ref store) = store {
                        let store_conv_id = acton_ai::prelude::ConversationId::parse(
                            &conversation_id.to_string(),
                        )
                        .unwrap_or_default();
                        store
                            .send(SaveMessage {
                                conversation_id: store_conv_id,
                                message: assistant_message.clone(),
                            })
                            .await;
                    }

                    // Send assistant message back to actor to append to history
                    self_handle
                        .send(AppendAssistantMessage {
                            message: assistant_message,
                        })
                        .await;

                    // Reply with the response
                    reply_envelope
                        .send(ConversationResponse {
                            correlation_id,
                            content: response_text,
                        })
                        .await;
                }
                Err(e) => {
                    error!(
                        conversation_id = %conversation_id,
                        error = %e,
                        "ActonAI error"
                    );
                    reply_envelope
                        .send(ConversationResponse {
                            correlation_id,
                            content: format!("LLM error: {e}"),
                        })
                        .await;
                }
            }
        });

        Reply::pending(async move {
            let _ = handle.await;
        })
    });

    // Internal message to append assistant response to history
    builder.mutate_on::<AppendAssistantMessage>(|actor, ctx| {
        actor.model.history.push(ctx.message().message.clone());
        Reply::ready()
    });

    // EndConversation: cleanup
    builder.mutate_on::<EndConversation>(|actor, _ctx| {
        debug!(
            conversation_id = %actor.model.id,
            turns = actor.model.turn_count,
            "ending conversation"
        );
        actor.model.history.clear();
        actor.model.acton_ai = None;
        actor.model.store = None;
        Reply::ready()
    });

    builder.start().await
}

/// Internal message to append an assistant message to history
///
/// This is used to send the LLM response back to the actor from the
/// async processing task, ensuring history is updated on the actor thread.
#[acton_message]
struct AppendAssistantMessage {
    message: Message,
}
