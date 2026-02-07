//! IPC message handlers
//!
//! Handles incoming messages from channels, including authentication
//! and message routing. User messages are delegated to the Router actor
//! which manages per-conversation actors for LLM processing.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use acton_reactive::prelude::{ActorHandle, ActorHandleInterface};

use crate::error::{TalonError, TalonResult};
use crate::ipc::auth::{AuthToken, TokenAuthenticator, ValidatedToken};
use crate::ipc::messages::{ChannelToCore, CoreToChannel};
use crate::router::RouteMessage;
use crate::types::ChannelId;

/// Handler for IPC messages from channels
#[async_trait]
pub trait IpcMessageHandler: Send + Sync {
    /// Handle an incoming message from a channel
    ///
    /// # Arguments
    ///
    /// * `message` - The message to handle
    ///
    /// # Returns
    ///
    /// A response message to send back to the channel, or an error.
    async fn handle(&self, message: ChannelToCore) -> TalonResult<CoreToChannel>;

    /// Check if a channel is authenticated
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel to check
    fn is_authenticated(&self, channel_id: &ChannelId) -> bool;
}

/// Default IPC message handler implementation
///
/// Handles authentication and delegates user messages to the Router actor.
/// The Router spawns per-conversation actors that handle LLM interaction.
pub struct DefaultIpcHandler {
    /// Token authenticator
    authenticator: TokenAuthenticator,
    /// Map of authenticated channels to their validated tokens
    authenticated_channels: Arc<DashMap<String, ValidatedToken>>,
    /// Router actor handle for delegating user messages
    router_handle: Option<ActorHandle>,
}

impl DefaultIpcHandler {
    /// Create a new handler with a token authenticator (no routing, for testing)
    ///
    /// # Arguments
    ///
    /// * `authenticator` - The token authenticator to use
    #[must_use]
    pub fn new(authenticator: TokenAuthenticator) -> Self {
        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            router_handle: None,
        }
    }

    /// Create a new handler with a router for delegating user messages
    ///
    /// # Arguments
    ///
    /// * `authenticator` - The token authenticator to use
    /// * `router` - Handle to the Router actor
    #[must_use]
    pub fn with_router(authenticator: TokenAuthenticator, router: ActorHandle) -> Self {
        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            router_handle: Some(router),
        }
    }

    /// Get a reference to the authenticated channels map
    #[must_use]
    pub fn authenticated_channels(&self) -> &Arc<DashMap<String, ValidatedToken>> {
        &self.authenticated_channels
    }

    /// Issue a token for a channel
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel to issue a token for
    #[must_use]
    pub fn issue_token(&self, channel_id: &ChannelId) -> AuthToken {
        self.authenticator.issue_token(channel_id)
    }

    /// Handle authentication message
    fn handle_authenticate(
        &self,
        channel_id: &ChannelId,
        token_str: &str,
    ) -> TalonResult<CoreToChannel> {
        let token = AuthToken::new(token_str);

        match self.authenticator.validate(&token) {
            Ok(validated) => {
                // Verify the token is for the right channel
                if validated.channel_id.as_str() != channel_id.as_str() {
                    warn!(
                        channel_id = %channel_id,
                        token_channel = %validated.channel_id,
                        "token channel mismatch"
                    );
                    return Ok(CoreToChannel::AuthenticationResult {
                        success: false,
                        error: Some("token channel mismatch".to_string()),
                    });
                }

                // Store the authenticated channel
                self.authenticated_channels
                    .insert(channel_id.to_string(), validated.clone());

                info!(channel_id = %channel_id, "channel authenticated successfully");

                Ok(CoreToChannel::AuthenticationResult {
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                warn!(channel_id = %channel_id, error = %e, "authentication failed");
                Ok(CoreToChannel::AuthenticationResult {
                    success: false,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    /// Handle registration message
    fn handle_register(&self, channel_id: &ChannelId) -> TalonResult<CoreToChannel> {
        // Check if already authenticated
        if !self
            .authenticated_channels
            .contains_key(channel_id.as_str())
        {
            warn!(channel_id = %channel_id, "registration attempt from unauthenticated channel");
            return Err(TalonError::Unauthenticated {
                channel_id: channel_id.to_string(),
            });
        }

        info!(channel_id = %channel_id, "channel registered");

        Ok(CoreToChannel::Registered {
            channel_id: channel_id.clone(),
        })
    }

    /// Handle disconnect message
    fn handle_disconnect(&self, channel_id: &ChannelId) -> TalonResult<CoreToChannel> {
        // Remove from authenticated channels
        self.authenticated_channels.remove(channel_id.as_str());
        info!(channel_id = %channel_id, "channel disconnected");

        // Return a registered response as acknowledgment
        // (the channel will disconnect after receiving this)
        Ok(CoreToChannel::Registered {
            channel_id: channel_id.clone(),
        })
    }

    /// Require authentication for a channel operation
    fn require_auth(&self, channel_id: &ChannelId) -> TalonResult<()> {
        if !self
            .authenticated_channels
            .contains_key(channel_id.as_str())
        {
            return Err(TalonError::Unauthenticated {
                channel_id: channel_id.to_string(),
            });
        }

        // Check if token has expired
        if let Some(entry) = self.authenticated_channels.get(channel_id.as_str()) {
            if entry.is_expired() {
                // Remove expired token
                drop(entry);
                self.authenticated_channels.remove(channel_id.as_str());
                return Err(TalonError::TokenExpired {
                    expired_at: chrono::Utc::now(),
                });
            }
        }

        Ok(())
    }
}

#[async_trait]
impl IpcMessageHandler for DefaultIpcHandler {
    async fn handle(&self, message: ChannelToCore) -> TalonResult<CoreToChannel> {
        match message {
            ChannelToCore::Authenticate { channel_id, token } => {
                self.handle_authenticate(&channel_id, &token)
            }
            ChannelToCore::Register { channel_id } => self.handle_register(&channel_id),
            ChannelToCore::Disconnect { channel_id } => self.handle_disconnect(&channel_id),
            ChannelToCore::UserMessage {
                correlation_id,
                conversation_id,
                sender,
                content,
            } => {
                // Extract channel ID from sender
                let channel_id = &sender.channel_id;

                // Require authentication
                self.require_auth(channel_id)?;

                debug!(
                    correlation_id = %correlation_id,
                    conversation_id = %conversation_id,
                    channel = %channel_id,
                    "delegating user message to router"
                );

                // Delegate to Router actor
                if let Some(router) = &self.router_handle {
                    router
                        .send(RouteMessage {
                            correlation_id: correlation_id.clone(),
                            conversation_id: *conversation_id.clone(),
                            sender: *sender,
                            content: content.clone(),
                        })
                        .await;

                    // The Router+ConversationActor will process the message
                    // and the response flows back through the actor reply chain.
                    // For now, we return a Processing indicator; the actual
                    // response will come through a separate channel mechanism.
                    //
                    // TODO: Once we have bidirectional actor reply wiring,
                    // this should await the ConversationResponse and return
                    // CoreToChannel::Complete. For now, the response is
                    // delivered asynchronously.
                    Ok(CoreToChannel::Complete {
                        correlation_id,
                        conversation_id,
                        content: "Message routed to conversation actor".to_string(),
                    })
                } else {
                    // Echo back if no router configured (for testing)
                    debug!("no router configured, echoing message");
                    Ok(CoreToChannel::Complete {
                        correlation_id,
                        conversation_id,
                        content: format!("Echo (no router): {content}"),
                    })
                }
            }
        }
    }

    fn is_authenticated(&self, channel_id: &ChannelId) -> bool {
        if let Some(entry) = self.authenticated_channels.get(channel_id.as_str()) {
            !entry.is_expired()
        } else {
            false
        }
    }
}

/// Handler that wraps another handler with additional functionality
pub struct LoggingHandler<H: IpcMessageHandler> {
    inner: H,
}

impl<H: IpcMessageHandler> LoggingHandler<H> {
    /// Create a new logging handler
    #[must_use]
    pub fn new(inner: H) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<H: IpcMessageHandler> IpcMessageHandler for LoggingHandler<H> {
    async fn handle(&self, message: ChannelToCore) -> TalonResult<CoreToChannel> {
        debug!(message = ?message, "handling IPC message");
        let result = self.inner.handle(message).await;
        match &result {
            Ok(response) => debug!(response = ?response, "IPC response"),
            Err(e) => warn!(error = %e, "IPC handler error"),
        }
        result
    }

    fn is_authenticated(&self, channel_id: &ChannelId) -> bool {
        self.inner.is_authenticated(channel_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ConversationId, CorrelationId, SenderId};

    fn test_authenticator() -> TokenAuthenticator {
        TokenAuthenticator::new(b"test-secret-key-for-testing-only")
    }

    #[tokio::test]
    async fn test_authenticate_success() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        // Issue a token
        let token = handler.issue_token(&channel_id);

        // Authenticate with the token
        let message = ChannelToCore::Authenticate {
            channel_id: channel_id.clone(),
            token: token.to_string(),
        };

        let response = handler.handle(message).await;
        assert!(response.is_ok());

        match response.expect("should be ok") {
            CoreToChannel::AuthenticationResult { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            other => panic!("expected AuthenticationResult, got {other:?}"),
        }

        // Should now be authenticated
        assert!(handler.is_authenticated(&channel_id));
    }

    #[tokio::test]
    async fn test_authenticate_invalid_token() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        let message = ChannelToCore::Authenticate {
            channel_id,
            token: "invalid-token".to_string(),
        };

        let response = handler.handle(message).await;
        assert!(response.is_ok());

        match response.expect("should be ok") {
            CoreToChannel::AuthenticationResult { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("expected AuthenticationResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_register_requires_auth() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        let message = ChannelToCore::Register { channel_id };

        let response = handler.handle(message).await;
        assert!(response.is_err());

        match response {
            Err(TalonError::Unauthenticated { .. }) => {}
            other => panic!("expected Unauthenticated error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_register_after_auth() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        // Authenticate first
        let token = handler.issue_token(&channel_id);
        let auth_msg = ChannelToCore::Authenticate {
            channel_id: channel_id.clone(),
            token: token.to_string(),
        };
        handler.handle(auth_msg).await.expect("auth should succeed");

        // Now register
        let register_msg = ChannelToCore::Register {
            channel_id: channel_id.clone(),
        };
        let response = handler.handle(register_msg).await;
        assert!(response.is_ok());

        match response.expect("should be ok") {
            CoreToChannel::Registered { channel_id: cid } => {
                assert_eq!(cid.as_str(), "terminal");
            }
            other => panic!("expected Registered, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_user_message_requires_auth() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        let message = ChannelToCore::UserMessage {
            correlation_id: CorrelationId::new(),
            conversation_id: Box::new(ConversationId::new()),
            sender: Box::new(SenderId::new(channel_id, "user123")),
            content: "Hello".to_string(),
        };

        let response = handler.handle(message).await;
        assert!(response.is_err());

        match response {
            Err(TalonError::Unauthenticated { .. }) => {}
            other => panic!("expected Unauthenticated error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_disconnect_removes_auth() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        // Authenticate first
        let token = handler.issue_token(&channel_id);
        let auth_msg = ChannelToCore::Authenticate {
            channel_id: channel_id.clone(),
            token: token.to_string(),
        };
        handler.handle(auth_msg).await.expect("auth should succeed");
        assert!(handler.is_authenticated(&channel_id));

        // Disconnect
        let disconnect_msg = ChannelToCore::Disconnect {
            channel_id: channel_id.clone(),
        };
        handler
            .handle(disconnect_msg)
            .await
            .expect("disconnect should succeed");

        // No longer authenticated
        assert!(!handler.is_authenticated(&channel_id));
    }

    #[tokio::test]
    async fn test_token_channel_mismatch() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id1 = ChannelId::new("terminal");
        let channel_id2 = ChannelId::new("telegram");

        // Issue token for channel 1
        let token = handler.issue_token(&channel_id1);

        // Try to authenticate channel 2 with channel 1's token
        let message = ChannelToCore::Authenticate {
            channel_id: channel_id2,
            token: token.to_string(),
        };

        let response = handler.handle(message).await;
        assert!(response.is_ok());

        match response.expect("should be ok") {
            CoreToChannel::AuthenticationResult { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.expect("should have error").contains("mismatch"));
            }
            other => panic!("expected AuthenticationResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_user_message_echoes_without_router() {
        let handler = DefaultIpcHandler::new(test_authenticator());
        let channel_id = ChannelId::new("terminal");

        // Authenticate first
        let token = handler.issue_token(&channel_id);
        let auth_msg = ChannelToCore::Authenticate {
            channel_id: channel_id.clone(),
            token: token.to_string(),
        };
        handler.handle(auth_msg).await.expect("auth should succeed");

        // Send user message without router configured
        let message = ChannelToCore::UserMessage {
            correlation_id: CorrelationId::new(),
            conversation_id: Box::new(ConversationId::new()),
            sender: Box::new(SenderId::new(channel_id, "user123")),
            content: "Hello".to_string(),
        };

        let response = handler.handle(message).await;
        assert!(response.is_ok());

        match response.expect("should be ok") {
            CoreToChannel::Complete { content, .. } => {
                assert!(content.contains("Echo"));
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }
}
