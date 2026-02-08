//! Conversation actor implementation
//!
//! Each conversation is managed by its own actor instance, holding
//! an acton-ai `Conversation` handle that manages history internally.
//! The `Conversation` is `Clone + Send + 'static` with zero-mutex
//! design via actor mailbox serialization.
//!
//! The `ConversationActor` owns both the `ActonAI` reference and the
//! `Conversation` handle, ensuring the internal actor lifecycle is
//! tied to this actor rather than an ephemeral spawn block.

use acton_ai::prelude::*;
use tracing::{debug, error, warn};

use crate::conversation::messages::{
    ConversationResponse, ConversationUserMessage, EndConversation, SetupConversation,
};
use crate::types::{ChannelId, ConversationId, SenderId};

/// Conversation actor state
///
/// Each conversation is managed by its own actor instance.
/// State includes the acton-ai `ActonAI` reference (kept alive to
/// anchor the runtime) and a `Conversation` handle which manages
/// history, system prompt, and LLM interaction internally.
#[acton_actor]
pub struct ConversationActor {
    /// Unique conversation identifier
    id: ConversationId,
    /// Sender identity
    sender: Option<SenderId>,
    /// Channel this conversation belongs to
    channel_id: ChannelId,
    /// ActonAI runtime reference — held to keep the internal runtime alive
    acton_ai: Option<ActonAI>,
    /// acton-ai Conversation handle that manages history internally
    conversation: Option<Conversation>,
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

    /// Get message history snapshot from the Conversation handle
    #[must_use]
    pub fn history(&self) -> Vec<Message> {
        self.conversation
            .as_ref()
            .map(|c| c.history())
            .unwrap_or_default()
    }
}

/// Internal message to store a built Conversation on actor state
#[acton_message]
struct ConversationBuilt {
    conversation: Conversation,
}

/// Spawn and configure a ConversationActor
///
/// Creates a new actor with message handlers registered and starts it.
/// The caller must send a `SetupConversation` message to initialize
/// the actor with an `ActonAI` reference. The actor builds its own
/// `Conversation` handle during setup, ensuring the lifecycle is
/// properly owned.
pub async fn spawn_conversation(runtime: &mut ActorRuntime, id: &ConversationId) -> ActorHandle {
    let mut builder =
        runtime.new_actor_with_name::<ConversationActor>(format!("conversation_{id}"));

    // SetupConversation: store ActonAI and build the Conversation handle
    builder.mutate_on::<SetupConversation>(|actor, ctx| {
        let msg = ctx.message().clone();
        actor.model.acton_ai = Some(msg.acton_ai.clone());
        actor.model.channel_id = msg.channel_id;

        let conversation_id = actor.model.id.clone();
        let self_handle = actor.handle().clone();

        Reply::pending(async move {
            let mut conv_builder = msg.acton_ai.conversation();
            if let Some(ref system) = msg.system_prompt {
                conv_builder = conv_builder.system(system.as_str());
            }
            let conv = conv_builder.build().await;

            debug!(
                conversation_id = %conversation_id,
                "conversation built, storing on actor"
            );

            self_handle
                .send(ConversationBuilt { conversation: conv })
                .await;
        })
    });

    // ConversationBuilt: store the built Conversation on actor state
    builder.mutate_on::<ConversationBuilt>(|actor, ctx| {
        actor.model.conversation = Some(ctx.message().conversation.clone());
        debug!(
            conversation_id = %actor.model.id,
            "conversation actor initialized"
        );
        Reply::ready()
    });

    // ConversationUserMessage: process user input through the Conversation handle
    builder.mutate_on::<ConversationUserMessage>(|actor, ctx| {
        let msg = ctx.message().clone();
        let reply_envelope = ctx.reply_envelope();

        // Set sender on first message
        if actor.model.sender.is_none() {
            actor.model.sender = Some(msg.sender.clone());
        }

        actor.model.turn_count += 1;

        let conversation = actor.model.conversation.clone();
        let conversation_id = actor.model.id.clone();
        let correlation_id = msg.correlation_id.clone();

        Reply::pending(async move {
            let Some(conv) = conversation else {
                warn!(
                    conversation_id = %conversation_id,
                    "no Conversation configured, echoing message"
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

            match conv.send(&msg.content).await {
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
        })
    });

    // EndConversation: cleanup
    builder.mutate_on::<EndConversation>(|actor, _ctx| {
        debug!(
            conversation_id = %actor.model.id,
            turns = actor.model.turn_count,
            "ending conversation"
        );
        if let Some(conv) = &actor.model.conversation {
            conv.clear();
        }
        actor.model.conversation = None;
        actor.model.acton_ai = None;
        Reply::ready()
    });

    builder.start().await
}
