//! IPC message handlers
//!
//! Handles incoming messages from channels, including authentication
//! and message routing.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use acton_ai::prelude::*;

use crate::error::{TalonError, TalonResult};
use crate::ipc::auth::{AuthToken, TokenAuthenticator, ValidatedToken};
use crate::ipc::messages::{ChannelToCore, CoreToChannel};
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
/// Handles authentication, LLM processing, and message routing.
pub struct DefaultIpcHandler {
    /// Token authenticator
    authenticator: TokenAuthenticator,
    /// Map of authenticated channels to their validated tokens
    authenticated_channels: Arc<DashMap<String, ValidatedToken>>,
    /// ActonAI runtime for processing messages (with built-in tools)
    acton_ai: Option<Arc<ActonAI>>,
    /// Conversation histories per conversation ID
    conversations: Arc<DashMap<String, Vec<Message>>>,
}

impl DefaultIpcHandler {
    /// Create a new handler with a token authenticator
    ///
    /// # Arguments
    ///
    /// * `authenticator` - The token authenticator to use
    #[must_use]
    pub fn new(authenticator: TokenAuthenticator) -> Self {
        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            acton_ai: None,
            conversations: Arc::new(DashMap::new()),
        }
    }

    /// Create a new handler with ActonAI runtime (with built-in tools)
    ///
    /// # Arguments
    ///
    /// * `authenticator` - The token authenticator to use
    /// * `acton_ai` - The ActonAI runtime with built-in tools enabled
    #[must_use]
    pub fn with_acton_ai(authenticator: TokenAuthenticator, acton_ai: Arc<ActonAI>) -> Self {
        Self {
            authenticator,
            authenticated_channels: Arc::new(DashMap::new()),
            acton_ai: Some(acton_ai),
            conversations: Arc::new(DashMap::new()),
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
        if !self.authenticated_channels.contains_key(channel_id.as_str()) {
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
        if !self.authenticated_channels.contains_key(channel_id.as_str()) {
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
                    "processing user message"
                );

                // Process with ActonAI (includes built-in tools)
                if let Some(ai) = &self.acton_ai {
                    // Get or create conversation history
                    let conv_key = conversation_id.to_string();
                    let mut history: Vec<Message> = self
                        .conversations
                        .get(&conv_key)
                        .map(|h| h.clone())
                        .unwrap_or_default();

                    // Add user message to history
                    history.push(Message::user(&content));

                    debug!(
                        history_len = history.len(),
                        "continuing conversation with history"
                    );

                    // Continue conversation with history
                    match ai.continue_with(history.clone()).collect().await {
                        Ok(response) => {
                            debug!(
                                response_len = response.text.len(),
                                tool_calls = response.tool_calls.len(),
                                "received LLM response"
                            );

                            // Guard against empty responses
                            let response_text = if response.text.is_empty() {
                                warn!("LLM returned empty response, using fallback");
                                "I received your message but couldn't generate a response. Please try again.".to_string()
                            } else {
                                response.text
                            };

                            // Add assistant response to history
                            history.push(Message::assistant(&response_text));
                            self.conversations.insert(conv_key, history);

                            Ok(CoreToChannel::Complete {
                                correlation_id,
                                conversation_id,
                                content: response_text,
                            })
                        }
                        Err(e) => {
                            error!(error = %e, "ActonAI error");
                            Ok(CoreToChannel::Error {
                                correlation_id,
                                message: format!("LLM error: {e}"),
                            })
                        }
                    }
                } else {
                    // Echo back if no AI configured (for testing)
                    debug!("no ActonAI configured, echoing message");
                    Ok(CoreToChannel::Complete {
                        correlation_id,
                        conversation_id,
                        content: format!("Echo (no AI): {content}"),
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
}
