//! High-level IPC client with state machine
//!
//! Provides a stateful client for communicating with the talon-core daemon
//! over Unix Domain Sockets.

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use talon_core::{ChannelToCore, ConversationId, CoreToChannel, CorrelationId, SenderId};

use super::config::IpcClientConfig;
use super::connection::{IpcConnection, IpcReader, IpcWriter};
use super::error::{IpcClientError, IpcClientResult};

/// Client connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientState {
    /// Not connected to the server
    Disconnected,
    /// Connected but not authenticated
    Connected,
    /// Connected and authenticated
    Authenticated,
    /// Connected, authenticated, and registered
    Registered,
}

impl std::fmt::Display for ClientState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connected => write!(f, "connected"),
            Self::Authenticated => write!(f, "authenticated"),
            Self::Registered => write!(f, "registered"),
        }
    }
}

/// Callback type for streaming tokens
pub type TokenCallback = Arc<dyn Fn(ConversationId, String) + Send + Sync>;

/// Callback type for stream completion
pub type CompleteCallback = Arc<dyn Fn(ConversationId, String) + Send + Sync>;

/// Callback type for errors
pub type ErrorCallback = Arc<dyn Fn(CorrelationId, String) + Send + Sync>;

/// High-level IPC client
///
/// Manages the connection lifecycle and provides methods for
/// sending messages to the core daemon.
///
/// Uses separate locks for reading and writing to allow concurrent
/// receive loop and message sending.
pub struct IpcClient {
    config: IpcClientConfig,
    /// Reader half - used by receive loop
    reader: Arc<Mutex<Option<IpcReader>>>,
    /// Writer half - used by send operations
    writer: Arc<Mutex<Option<IpcWriter>>>,
    state: Arc<RwLock<ClientState>>,
    token_callback: Arc<RwLock<Option<TokenCallback>>>,
    complete_callback: Arc<RwLock<Option<CompleteCallback>>>,
    error_callback: Arc<RwLock<Option<ErrorCallback>>>,
}

impl IpcClient {
    /// Create a new IPC client with the given configuration
    #[must_use]
    pub fn new(config: IpcClientConfig) -> Self {
        Self {
            config,
            reader: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            token_callback: Arc::new(RwLock::new(None)),
            complete_callback: Arc::new(RwLock::new(None)),
            error_callback: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the current connection state
    pub async fn state(&self) -> ClientState {
        *self.state.read().await
    }

    /// Check if the client is connected (any connected state)
    pub async fn is_connected(&self) -> bool {
        !matches!(self.state().await, ClientState::Disconnected)
    }

    /// Check if the client is ready to send messages
    pub async fn is_ready(&self) -> bool {
        matches!(self.state().await, ClientState::Registered)
    }

    /// Connect to the IPC server
    ///
    /// # Errors
    ///
    /// Returns error if already connected or connection fails.
    pub async fn connect(&self) -> IpcClientResult<()> {
        // Check current state
        let current_state = self.state().await;
        if current_state != ClientState::Disconnected {
            return Err(IpcClientError::AlreadyConnected);
        }

        info!(socket_path = %self.config.socket_path.display(), "connecting to IPC server");

        // Attempt connection with timeout
        let conn = timeout(
            self.config.connect_timeout,
            IpcConnection::connect(&self.config.socket_path),
        )
        .await
        .map_err(|_| IpcClientError::Timeout {
            operation: "connect".to_string(),
            duration: self.config.connect_timeout,
        })??;

        // Split connection into reader and writer
        let (reader, writer) = conn.split();

        // Store both halves
        *self.reader.lock().await = Some(reader);
        *self.writer.lock().await = Some(writer);
        *self.state.write().await = ClientState::Connected;

        info!("connected to IPC server");
        Ok(())
    }

    /// Authenticate with the server
    ///
    /// # Errors
    ///
    /// Returns error if not connected or authentication fails.
    pub async fn authenticate(&self) -> IpcClientResult<()> {
        // Check current state
        let current_state = self.state().await;
        if current_state == ClientState::Disconnected {
            return Err(IpcClientError::NotConnected);
        }
        if current_state != ClientState::Connected {
            return Err(IpcClientError::InvalidState {
                current: current_state.to_string(),
                expected: "connected".to_string(),
            });
        }

        debug!(channel_id = %self.config.channel_id, "authenticating");

        // Send authentication request
        let auth_msg = ChannelToCore::Authenticate {
            channel_id: self.config.channel_id.clone(),
            token: self.config.auth_token.clone(),
        };

        self.send_raw(&auth_msg).await?;

        // Receive response
        let response: CoreToChannel = self.receive_raw().await?;

        match response {
            CoreToChannel::AuthenticationResult { success, error } => {
                if success {
                    *self.state.write().await = ClientState::Authenticated;
                    info!(channel_id = %self.config.channel_id, "authenticated successfully");
                    Ok(())
                } else {
                    let reason = error.unwrap_or_else(|| "unknown error".to_string());
                    error!(channel_id = %self.config.channel_id, reason = %reason, "authentication failed");
                    Err(IpcClientError::AuthenticationFailed { reason })
                }
            }
            other => {
                error!(?other, "unexpected response to authentication");
                Err(IpcClientError::InvalidFrame {
                    message: format!("expected AuthenticationResult, got {other:?}"),
                })
            }
        }
    }

    /// Register with the server
    ///
    /// # Errors
    ///
    /// Returns error if not authenticated or registration fails.
    pub async fn register(&self) -> IpcClientResult<()> {
        // Check current state
        let current_state = self.state().await;
        if current_state == ClientState::Disconnected {
            return Err(IpcClientError::NotConnected);
        }
        if current_state != ClientState::Authenticated {
            return Err(IpcClientError::InvalidState {
                current: current_state.to_string(),
                expected: "authenticated".to_string(),
            });
        }

        debug!(channel_id = %self.config.channel_id, "registering");

        // Send registration request
        let register_msg = ChannelToCore::Register {
            channel_id: self.config.channel_id.clone(),
        };

        self.send_raw(&register_msg).await?;

        // Receive response
        let response: CoreToChannel = self.receive_raw().await?;

        match response {
            CoreToChannel::Registered { channel_id } => {
                *self.state.write().await = ClientState::Registered;
                info!(channel_id = %channel_id, "registered successfully");
                Ok(())
            }
            CoreToChannel::Error {
                correlation_id: _,
                message,
            } => {
                error!(message = %message, "registration failed");
                Err(IpcClientError::AuthenticationFailed { reason: message })
            }
            other => {
                error!(?other, "unexpected response to registration");
                Err(IpcClientError::InvalidFrame {
                    message: format!("expected Registered, got {other:?}"),
                })
            }
        }
    }

    /// Send a user message to the core
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - The conversation ID
    /// * `sender` - The sender identity
    /// * `content` - The message content
    ///
    /// # Returns
    ///
    /// The correlation ID for tracking the request.
    ///
    /// # Errors
    ///
    /// Returns error if not registered or send fails.
    pub async fn send_message(
        &self,
        conversation_id: ConversationId,
        sender: SenderId,
        content: String,
    ) -> IpcClientResult<CorrelationId> {
        // Check current state
        let current_state = self.state().await;
        if current_state != ClientState::Registered {
            return Err(IpcClientError::NotRegistered);
        }

        let correlation_id = CorrelationId::new();

        debug!(
            correlation_id = %correlation_id,
            conversation_id = %conversation_id,
            "sending user message"
        );

        let msg = ChannelToCore::UserMessage {
            correlation_id: correlation_id.clone(),
            conversation_id: Box::new(conversation_id),
            sender: Box::new(sender),
            content,
        };

        self.send_raw(&msg).await?;

        debug!(correlation_id = %correlation_id, "user message sent");

        Ok(correlation_id)
    }

    /// Disconnect from the server
    ///
    /// # Errors
    ///
    /// Returns error if not connected.
    pub async fn disconnect(&self) -> IpcClientResult<()> {
        let current_state = self.state().await;
        if current_state == ClientState::Disconnected {
            return Ok(()); // Already disconnected
        }

        debug!(channel_id = %self.config.channel_id, "disconnecting");

        // Send disconnect message if we're registered
        if current_state == ClientState::Registered || current_state == ClientState::Authenticated {
            let disconnect_msg = ChannelToCore::Disconnect {
                channel_id: self.config.channel_id.clone(),
            };
            // Ignore errors - we're disconnecting anyway
            let _ = self.send_raw(&disconnect_msg).await;
        }

        // Shutdown writer
        if let Some(mut writer) = self.writer.lock().await.take() {
            let _ = writer.shutdown().await;
        }

        // Drop reader
        let _ = self.reader.lock().await.take();

        *self.state.write().await = ClientState::Disconnected;
        info!(channel_id = %self.config.channel_id, "disconnected");

        Ok(())
    }

    /// Set callback for streaming tokens
    pub async fn set_token_callback(&self, callback: TokenCallback) {
        *self.token_callback.write().await = Some(callback);
    }

    /// Set callback for stream completion
    pub async fn set_complete_callback(&self, callback: CompleteCallback) {
        *self.complete_callback.write().await = Some(callback);
    }

    /// Set callback for errors
    pub async fn set_error_callback(&self, callback: ErrorCallback) {
        *self.error_callback.write().await = Some(callback);
    }

    /// Start the receive loop
    ///
    /// This spawns a background task that receives messages from the server
    /// and dispatches them to the appropriate callbacks.
    ///
    /// # Returns
    ///
    /// A handle to the background task.
    pub fn start_receive_loop(&self) -> tokio::task::JoinHandle<()> {
        let reader = Arc::clone(&self.reader);
        let state = Arc::clone(&self.state);
        let token_callback = Arc::clone(&self.token_callback);
        let complete_callback = Arc::clone(&self.complete_callback);
        let error_callback = Arc::clone(&self.error_callback);
        let channel_id = self.config.channel_id.clone();

        tokio::spawn(async move {
            loop {
                // Check if still connected
                let current_state = *state.read().await;
                if current_state == ClientState::Disconnected {
                    debug!("receive loop exiting: disconnected");
                    break;
                }

                // Try to receive a message (uses only reader lock)
                let result = {
                    let mut reader_guard = reader.lock().await;
                    if let Some(r) = reader_guard.as_mut() {
                        r.receive::<CoreToChannel>().await
                    } else {
                        break;
                    }
                };

                match result {
                    Ok(message) => {
                        match message {
                            CoreToChannel::Token {
                                correlation_id: _,
                                conversation_id,
                                token,
                            } => {
                                debug!(conversation_id = %conversation_id, "received token");
                                if let Some(callback) = token_callback.read().await.as_ref() {
                                    callback(*conversation_id, token);
                                }
                            }
                            CoreToChannel::Complete {
                                correlation_id: _,
                                conversation_id,
                                content,
                            } => {
                                debug!(conversation_id = %conversation_id, "received complete");
                                if let Some(callback) = complete_callback.read().await.as_ref() {
                                    callback(*conversation_id, content);
                                }
                            }
                            CoreToChannel::Error {
                                correlation_id,
                                message,
                            } => {
                                debug!(correlation_id = %correlation_id, "received error");
                                if let Some(callback) = error_callback.read().await.as_ref() {
                                    callback(correlation_id, message);
                                }
                            }
                            CoreToChannel::Registered { .. }
                            | CoreToChannel::AuthenticationResult { .. } => {
                                // These are handled synchronously during connect flow
                                debug!(?message, "received protocol message in receive loop");
                            }
                        }
                    }
                    Err(IpcClientError::ConnectionClosed) => {
                        warn!(channel_id = %channel_id, "connection closed by server");
                        *state.write().await = ClientState::Disconnected;
                        break;
                    }
                    Err(e) => {
                        error!(channel_id = %channel_id, error = %e, "receive error");
                        // Continue trying for recoverable errors
                    }
                }
            }
        })
    }

    /// Send a raw message (uses writer lock only)
    async fn send_raw<M: serde::Serialize>(&self, message: &M) -> IpcClientResult<()> {
        let mut writer_guard = self.writer.lock().await;
        let writer = writer_guard.as_mut().ok_or(IpcClientError::NotConnected)?;

        timeout(self.config.response_timeout, writer.send(message))
            .await
            .map_err(|_| IpcClientError::Timeout {
                operation: "send".to_string(),
                duration: self.config.response_timeout,
            })?
    }

    /// Receive a raw message (uses reader lock only)
    async fn receive_raw<M: serde::de::DeserializeOwned>(&self) -> IpcClientResult<M> {
        let mut reader_guard = self.reader.lock().await;
        let reader = reader_guard.as_mut().ok_or(IpcClientError::NotConnected)?;

        timeout(self.config.response_timeout, reader.receive::<M>())
            .await
            .map_err(|_| IpcClientError::Timeout {
                operation: "receive".to_string(),
                duration: self.config.response_timeout,
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talon_core::ChannelId;

    #[test]
    fn test_client_state_display() {
        assert_eq!(ClientState::Disconnected.to_string(), "disconnected");
        assert_eq!(ClientState::Connected.to_string(), "connected");
        assert_eq!(ClientState::Authenticated.to_string(), "authenticated");
        assert_eq!(ClientState::Registered.to_string(), "registered");
    }

    #[tokio::test]
    async fn test_client_initial_state() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        assert_eq!(client.state().await, ClientState::Disconnected);
        assert!(!client.is_connected().await);
        assert!(!client.is_ready().await);
    }

    #[tokio::test]
    async fn test_connect_already_connected() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        // Manually set state to connected
        *client.state.write().await = ClientState::Connected;

        let result = client.connect().await;
        assert!(matches!(result, Err(IpcClientError::AlreadyConnected)));
    }

    #[tokio::test]
    async fn test_authenticate_not_connected() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        let result = client.authenticate().await;
        assert!(matches!(result, Err(IpcClientError::NotConnected)));
    }

    #[tokio::test]
    async fn test_register_not_authenticated() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        // Set to connected (not authenticated)
        *client.state.write().await = ClientState::Connected;

        let result = client.register().await;
        assert!(matches!(result, Err(IpcClientError::InvalidState { .. })));
    }

    #[tokio::test]
    async fn test_send_message_not_registered() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        let result = client
            .send_message(
                ConversationId::new(),
                SenderId::new(ChannelId::new("test"), "user1"),
                "hello".to_string(),
            )
            .await;

        assert!(matches!(result, Err(IpcClientError::NotRegistered)));
    }

    #[tokio::test]
    async fn test_disconnect_when_disconnected() {
        let config = IpcClientConfig::new(ChannelId::new("test"), "token");
        let client = IpcClient::new(config);

        // Should not error when already disconnected
        let result = client.disconnect().await;
        assert!(result.is_ok());
    }
}
