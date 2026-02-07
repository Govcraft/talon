# Phase 4: IPC Connection Implementation Plan

> Rust Implementation Plan - February 5, 2026
> Status: READY FOR IMPLEMENTATION

## Overview

This plan implements the IPC (Inter-Process Communication) layer between talon-core and channel processes (talon-cli, talon-telegram, talon-discord) using acton-reactive's IPC feature over Unix Domain Sockets.

## Goals

1. **Server-side IPC**: Enable talon-core to expose Router actor via Unix Domain Socket
2. **Client-side IPC**: Enable channels to connect and communicate with talon-core
3. **Message Protocol**: Use acton-reactive's IpcEnvelope protocol with Talon message types
4. **Authentication**: Integrate existing HMAC token authentication with IPC handshake
5. **Streaming Support**: Enable streaming responses for real-time token delivery

## Architecture

```
+-------------------------------------------------------------------+
|                     talon-core (daemon)                           |
|  +-------------+  +-------------+  +-------------+                |
|  |   Router    |  |    IPC      |  |  Message    |                |
|  |   Actor     |<-|  Listener   |  |  Registry   |                |
|  +-------------+  +-------------+  +-------------+                |
|        ^               ^                                          |
|        |               |                                          |
|        +---------------+                                          |
|               Unix Domain Socket                                  |
|              ~/.local/run/talon/talon.sock                        |
+-------------------------------------------------------------------+
                          |
        +-----------------+-----------------+
        |                 |                 |
        v                 v                 v
  +-----------+    +-----------+    +-----------+
  |  IpcClient|    |  IpcClient|    |  IpcClient|
  |  (CLI)    |    |  (Telegram)|   |  (Discord)|
  +-----------+    +-----------+    +-----------+
```

## Existing Code Analysis

### Already Implemented

1. **IPC Message Types** (`ipc/messages.rs`):
   - `ChannelToCore` enum (UserMessage, Register, Disconnect, Authenticate)
   - `CoreToChannel` enum (Token, Complete, Error, Registered, AuthenticationResult)

2. **Authentication** (`ipc/auth.rs`):
   - `AuthToken` newtype with HMAC-SHA256 signing
   - `TokenAuthenticator` for issuing/validating tokens
   - `ValidatedToken` with expiration checking

3. **Handler Framework** (`ipc/handlers.rs`):
   - `IpcMessageHandler` trait
   - `DefaultIpcHandler` implementation with auth enforcement
   - `LoggingHandler` decorator

4. **Router Actor** (`router/actor.rs`):
   - `Router` actor with conversation tracking
   - Message routing and lifecycle management
   - Not yet integrated with IPC

5. **Runtime** (`runtime.rs`):
   - `TalonRuntime` with placeholder IPC support
   - `start_ipc()` method as placeholder

### Gaps to Fill

1. **acton-reactive IPC Integration**: Wire up Router to acton-reactive's IPC listener
2. **IPC Message Attribute**: Add `#[acton_message(ipc)]` for serialization
3. **Type Registration**: Register message types with IPC registry
4. **Actor Exposure**: Expose Router actor for IPC access
5. **Client Module**: Create IPC client for channel processes
6. **Connection Management**: Handle connection lifecycle and reconnection

## Implementation Tasks

### Task 1: Update IPC Message Types for acton-reactive

Convert existing messages to use `#[acton_message(ipc)]` attribute.

**File**: `crates/talon-core/src/ipc/messages.rs`

```rust
//! IPC message definitions for acton-reactive integration

use acton_reactive::prelude::*;
use serde::{Deserialize, Serialize};

use crate::types::{ChannelId, ConversationId, CorrelationId, SenderId};

/// IPC message type names for registration
pub mod message_types {
    pub const CHANNEL_AUTHENTICATE: &str = "ChannelAuthenticate";
    pub const CHANNEL_REGISTER: &str = "ChannelRegister";
    pub const CHANNEL_DISCONNECT: &str = "ChannelDisconnect";
    pub const USER_MESSAGE: &str = "UserMessage";
    pub const STREAM_TOKEN: &str = "StreamToken";
    pub const MESSAGE_COMPLETE: &str = "MessageComplete";
    pub const ERROR_RESPONSE: &str = "ErrorResponse";
    pub const AUTH_RESULT: &str = "AuthResult";
    pub const REGISTERED: &str = "Registered";
}

/// Channel authentication request
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct ChannelAuthenticate {
    /// Channel identifier
    pub channel_id: ChannelId,
    /// Authentication token
    pub token: String,
}

/// Channel registration request (after authentication)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct ChannelRegister {
    /// Channel identifier
    pub channel_id: ChannelId,
}

/// Channel disconnection notification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct ChannelDisconnect {
    /// Channel identifier
    pub channel_id: ChannelId,
}

/// User message from channel to core
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct UserMessage {
    /// Correlation ID for tracking
    pub correlation_id: CorrelationId,
    /// Conversation ID
    pub conversation_id: ConversationId,
    /// Sender identity
    pub sender: SenderId,
    /// Message content
    pub content: String,
}

/// Streaming token response
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct StreamToken {
    /// Correlation ID for tracking
    pub correlation_id: CorrelationId,
    /// Conversation ID
    pub conversation_id: ConversationId,
    /// Token content
    pub token: String,
    /// Whether this is the final token
    pub is_final: bool,
}

/// Message completion response
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct MessageComplete {
    /// Correlation ID for tracking
    pub correlation_id: CorrelationId,
    /// Conversation ID
    pub conversation_id: ConversationId,
    /// Full response content
    pub content: String,
}

/// Error response
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct ErrorResponse {
    /// Correlation ID for tracking
    pub correlation_id: CorrelationId,
    /// Error message
    pub message: String,
    /// Error code (optional)
    pub code: Option<String>,
}

/// Authentication result
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct AuthResult {
    /// Whether authentication succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Session token for subsequent requests (if success)
    pub session_token: Option<String>,
}

/// Registration acknowledgment
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message(ipc)]
pub struct Registered {
    /// Channel identifier
    pub channel_id: ChannelId,
}
```

### Task 2: Create IPC Server Module

New module for IPC server setup and configuration.

**File**: `crates/talon-core/src/ipc/server.rs`

```rust
//! IPC server for channel connections
//!
//! Manages the Unix Domain Socket listener and handles incoming
//! connections from channel processes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use acton_reactive::ipc::{IpcConfig, IpcListener};
use acton_reactive::prelude::*;
use tracing::{info, warn};

use crate::error::{TalonError, TalonResult};
use crate::ipc::messages::message_types;
use crate::ipc::{
    AuthResult, ChannelAuthenticate, ChannelDisconnect, ChannelRegister, ErrorResponse,
    MessageComplete, Registered, StreamToken, UserMessage,
};

/// Default socket directory name
const SOCKET_DIR: &str = "talon";

/// Default socket file name
const SOCKET_FILE: &str = "talon.sock";

/// IPC server configuration
#[derive(Clone, Debug)]
pub struct IpcServerConfig {
    /// Socket path
    pub socket_path: PathBuf,
    /// Maximum connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
}

impl Default for IpcServerConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            max_connections: 100,
            connection_timeout_secs: 300,
        }
    }
}

/// Get the default socket path following XDG conventions
fn default_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(SOCKET_DIR)
        .join(SOCKET_FILE)
}

/// IPC server handle
pub struct IpcServer {
    /// The underlying listener
    listener: IpcListener,
    /// Server configuration
    config: IpcServerConfig,
}

impl IpcServer {
    /// Register all Talon message types with the IPC registry
    pub fn register_message_types(runtime: &ActorRuntime) {
        let registry = runtime.ipc_registry();

        // Request types (channel -> core)
        registry.register::<ChannelAuthenticate>(message_types::CHANNEL_AUTHENTICATE);
        registry.register::<ChannelRegister>(message_types::CHANNEL_REGISTER);
        registry.register::<ChannelDisconnect>(message_types::CHANNEL_DISCONNECT);
        registry.register::<UserMessage>(message_types::USER_MESSAGE);

        // Response types (core -> channel)
        registry.register::<StreamToken>(message_types::STREAM_TOKEN);
        registry.register::<MessageComplete>(message_types::MESSAGE_COMPLETE);
        registry.register::<ErrorResponse>(message_types::ERROR_RESPONSE);
        registry.register::<AuthResult>(message_types::AUTH_RESULT);
        registry.register::<Registered>(message_types::REGISTERED);

        info!("registered IPC message types");
    }

    /// Start the IPC server
    ///
    /// # Arguments
    ///
    /// * `runtime` - The actor runtime
    /// * `config` - Server configuration
    ///
    /// # Errors
    ///
    /// Returns error if server cannot be started.
    pub async fn start(
        runtime: &mut ActorRuntime,
        config: IpcServerConfig,
    ) -> TalonResult<Self> {
        // Ensure socket directory exists
        if let Some(parent) = config.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket if present
        if config.socket_path.exists() {
            std::fs::remove_file(&config.socket_path)?;
        }

        info!(socket_path = %config.socket_path.display(), "starting IPC server");

        // Create acton-reactive IPC config
        let ipc_config = IpcConfig::new(&config.socket_path);

        // Start the listener
        let listener = runtime
            .start_ipc_listener_with_config(ipc_config)
            .await
            .map_err(|e| TalonError::Ipc {
                message: format!("failed to start IPC listener: {e}"),
            })?;

        info!(socket_path = %config.socket_path.display(), "IPC server started");

        Ok(Self { listener, config })
    }

    /// Get the socket path
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.config.socket_path
    }

    /// Get server statistics
    #[must_use]
    pub fn stats(&self) -> IpcServerStats {
        let inner_stats = self.listener.stats();
        IpcServerStats {
            active_connections: inner_stats.connections_active,
            total_connections: inner_stats.connections_total,
            messages_received: inner_stats.messages_received,
            messages_sent: inner_stats.messages_sent,
        }
    }

    /// Shutdown the server gracefully
    pub async fn shutdown(self) {
        info!("shutting down IPC server");
        self.listener.shutdown().await;

        // Clean up socket file
        if self.config.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.config.socket_path) {
                warn!(error = %e, "failed to remove socket file");
            }
        }

        info!("IPC server shutdown complete");
    }
}

/// IPC server statistics
#[derive(Clone, Debug)]
pub struct IpcServerStats {
    /// Currently active connections
    pub active_connections: usize,
    /// Total connections since start
    pub total_connections: usize,
    /// Total messages received
    pub messages_received: u64,
    /// Total messages sent
    pub messages_sent: u64,
}
```

### Task 3: Create IPC Client Module

Client for channel processes to connect to talon-core.

**File**: `crates/talon-core/src/ipc/client.rs`

```rust
//! IPC client for channel processes
//!
//! Enables channels to connect to talon-core via Unix Domain Socket.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use acton_reactive::ipc::protocol::{read_response, write_envelope};
use acton_reactive::ipc::{socket_exists, socket_is_alive, IpcEnvelope, IpcResponse};
use serde::Serialize;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::{TalonError, TalonResult};
use crate::ipc::messages::message_types;
use crate::ipc::{AuthResult, ChannelAuthenticate, ChannelDisconnect, ChannelRegister, Registered};
use crate::types::ChannelId;

/// Maximum response size (1MB)
const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

/// Default request timeout
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// IPC client configuration
#[derive(Clone, Debug)]
pub struct IpcClientConfig {
    /// Socket path to connect to
    pub socket_path: PathBuf,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Retry attempts for connection
    pub retry_attempts: u32,
    /// Delay between retries in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for IpcClientConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Get the default socket path
fn default_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("talon")
        .join("talon.sock")
}

/// Connection state
enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Connected with read/write streams
    Connected {
        reader: ReadHalf<UnixStream>,
        writer: WriteHalf<UnixStream>,
    },
    /// Authenticated and ready
    Authenticated {
        reader: ReadHalf<UnixStream>,
        writer: WriteHalf<UnixStream>,
        session_token: String,
    },
}

/// IPC client for channel-to-core communication
pub struct IpcClient {
    /// Client configuration
    config: IpcClientConfig,
    /// Channel identifier
    channel_id: ChannelId,
    /// Connection state (wrapped for interior mutability)
    state: Mutex<ConnectionState>,
}

impl IpcClient {
    /// Create a new IPC client
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier
    /// * `config` - Client configuration
    #[must_use]
    pub fn new(channel_id: ChannelId, config: IpcClientConfig) -> Self {
        Self {
            config,
            channel_id,
            state: Mutex::new(ConnectionState::Disconnected),
        }
    }

    /// Create with default configuration
    #[must_use]
    pub fn with_defaults(channel_id: ChannelId) -> Self {
        Self::new(channel_id, IpcClientConfig::default())
    }

    /// Check if the server is available
    pub async fn is_server_available(&self) -> bool {
        socket_exists(&self.config.socket_path)
            && socket_is_alive(&self.config.socket_path).await
    }

    /// Connect to the server
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(&self) -> TalonResult<()> {
        let mut state = self.state.lock().await;

        // Already connected?
        if matches!(*state, ConnectionState::Connected { .. } | ConnectionState::Authenticated { .. }) {
            return Ok(());
        }

        info!(
            socket_path = %self.config.socket_path.display(),
            channel_id = %self.channel_id,
            "connecting to talon-core"
        );

        // Check socket availability
        if !socket_exists(&self.config.socket_path) {
            return Err(TalonError::Ipc {
                message: format!(
                    "socket not found: {}",
                    self.config.socket_path.display()
                ),
            });
        }

        // Attempt connection with retries
        let mut last_error = None;
        for attempt in 0..self.config.retry_attempts {
            match UnixStream::connect(&self.config.socket_path).await {
                Ok(stream) => {
                    let (reader, writer) = tokio::io::split(stream);
                    *state = ConnectionState::Connected { reader, writer };
                    info!(channel_id = %self.channel_id, "connected to talon-core");
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt + 1 < self.config.retry_attempts {
                        debug!(
                            attempt = attempt + 1,
                            max_attempts = self.config.retry_attempts,
                            "connection failed, retrying"
                        );
                        tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(TalonError::Ipc {
            message: format!(
                "failed to connect after {} attempts: {}",
                self.config.retry_attempts,
                last_error.map_or_else(|| "unknown error".to_string(), |e| e.to_string())
            ),
        })
    }

    /// Authenticate with the server
    ///
    /// # Arguments
    ///
    /// * `token` - Authentication token
    ///
    /// # Errors
    ///
    /// Returns error if authentication fails.
    pub async fn authenticate(&self, token: &str) -> TalonResult<()> {
        let mut state = self.state.lock().await;

        // Check connection state
        let (reader, writer) = match std::mem::replace(&mut *state, ConnectionState::Disconnected) {
            ConnectionState::Connected { reader, writer } => (reader, writer),
            ConnectionState::Authenticated { .. } => {
                return Ok(()); // Already authenticated
            }
            ConnectionState::Disconnected => {
                return Err(TalonError::Ipc {
                    message: "not connected".to_string(),
                });
            }
        };

        // Send authentication request
        let auth_request = ChannelAuthenticate {
            channel_id: self.channel_id.clone(),
            token: token.to_string(),
        };

        let response = send_and_receive(
            reader,
            writer,
            "router",
            message_types::CHANNEL_AUTHENTICATE,
            &auth_request,
            self.config.timeout_ms,
        )
        .await?;

        let auth_result: AuthResult = serde_json::from_value(response.payload)
            .map_err(|e| TalonError::Serialization {
                message: e.to_string(),
            })?;

        if auth_result.success {
            // Reconnect for authenticated state
            let stream = UnixStream::connect(&self.config.socket_path).await?;
            let (reader, writer) = tokio::io::split(stream);

            *state = ConnectionState::Authenticated {
                reader,
                writer,
                session_token: auth_result.session_token.unwrap_or_default(),
            };

            info!(channel_id = %self.channel_id, "authenticated successfully");
            Ok(())
        } else {
            *state = ConnectionState::Disconnected;
            Err(TalonError::AuthenticationFailed {
                reason: auth_result.error.unwrap_or_else(|| "unknown error".to_string()),
            })
        }
    }

    /// Send a request and wait for response
    ///
    /// # Type Parameters
    ///
    /// * `T` - Request type (must be Serialize)
    /// * `R` - Response type (must be DeserializeOwned)
    ///
    /// # Arguments
    ///
    /// * `target_actor` - Target actor name
    /// * `message_type` - Message type name
    /// * `request` - The request payload
    ///
    /// # Errors
    ///
    /// Returns error if request fails or response cannot be deserialized.
    pub async fn request<T, R>(
        &self,
        target_actor: &str,
        message_type: &str,
        request: &T,
    ) -> TalonResult<R>
    where
        T: Serialize,
        R: serde::de::DeserializeOwned,
    {
        let mut state = self.state.lock().await;

        let (reader, writer) = match &mut *state {
            ConnectionState::Authenticated { reader, writer, .. } => {
                // Take ownership temporarily
                todo!("implement request sending for authenticated state")
            }
            ConnectionState::Connected { .. } => {
                return Err(TalonError::Unauthenticated {
                    channel_id: self.channel_id.to_string(),
                });
            }
            ConnectionState::Disconnected => {
                return Err(TalonError::Ipc {
                    message: "not connected".to_string(),
                });
            }
        };
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) -> TalonResult<()> {
        let mut state = self.state.lock().await;

        match std::mem::replace(&mut *state, ConnectionState::Disconnected) {
            ConnectionState::Authenticated { reader, writer, .. }
            | ConnectionState::Connected { reader, writer } => {
                // Send disconnect notification
                let disconnect = ChannelDisconnect {
                    channel_id: self.channel_id.clone(),
                };

                // Best effort - ignore errors during disconnect
                let _ = send_and_receive(
                    reader,
                    writer,
                    "router",
                    message_types::CHANNEL_DISCONNECT,
                    &disconnect,
                    5000,
                )
                .await;

                info!(channel_id = %self.channel_id, "disconnected from talon-core");
            }
            ConnectionState::Disconnected => {
                // Already disconnected
            }
        }

        Ok(())
    }

    /// Get the channel ID
    #[must_use]
    pub fn channel_id(&self) -> &ChannelId {
        &self.channel_id
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        let state = self.state.lock().await;
        matches!(*state, ConnectionState::Connected { .. } | ConnectionState::Authenticated { .. })
    }

    /// Check if authenticated
    pub async fn is_authenticated(&self) -> bool {
        let state = self.state.lock().await;
        matches!(*state, ConnectionState::Authenticated { .. })
    }
}

/// Send a message and receive response
async fn send_and_receive<T: Serialize>(
    mut reader: ReadHalf<UnixStream>,
    mut writer: WriteHalf<UnixStream>,
    target_actor: &str,
    message_type: &str,
    payload: &T,
    timeout_ms: u64,
) -> TalonResult<IpcResponse> {
    let payload_value = serde_json::to_value(payload)?;

    let envelope = IpcEnvelope::new_request_with_timeout(
        target_actor,
        message_type,
        payload_value,
        timeout_ms,
    );

    write_envelope(&mut writer, &envelope)
        .await
        .map_err(|e| TalonError::Ipc {
            message: format!("failed to send message: {e}"),
        })?;

    let response = read_response(&mut reader, MAX_RESPONSE_SIZE)
        .await
        .map_err(|e| TalonError::Ipc {
            message: format!("failed to read response: {e}"),
        })?;

    if !response.success {
        return Err(TalonError::Ipc {
            message: response
                .error
                .map_or_else(|| "unknown error".to_string(), |e| e.message),
        });
    }

    Ok(response)
}
```

### Task 4: Update Router for IPC Integration

Add message handlers to Router actor for IPC messages.

**File**: `crates/talon-core/src/router/actor.rs` (modifications)

Add new handler registrations in the builder pattern:

```rust
// In the runtime, when building the router:

let mut router_builder = runtime.new_actor::<Router>();

// Handle authentication
router_builder.mutate_on::<ChannelAuthenticate>(|actor, env| {
    let channel_id = env.message().channel_id.clone();
    let token = env.message().token.clone();
    let reply = env.reply_envelope();
    let authenticator = actor.model.authenticator.clone();

    Reply::pending(async move {
        let auth_token = AuthToken::new(&token);
        match authenticator.validate(&auth_token) {
            Ok(validated) => {
                if validated.channel_id.as_str() != channel_id.as_str() {
                    reply.send(AuthResult {
                        success: false,
                        error: Some("token channel mismatch".to_string()),
                        session_token: None,
                    }).await;
                } else {
                    reply.send(AuthResult {
                        success: true,
                        error: None,
                        session_token: Some(token),
                    }).await;
                }
            }
            Err(e) => {
                reply.send(AuthResult {
                    success: false,
                    error: Some(e.to_string()),
                    session_token: None,
                }).await;
            }
        }
    })
});

// Handle registration
router_builder.mutate_on::<ChannelRegister>(|actor, env| {
    let channel_id = env.message().channel_id.clone();
    let reply = env.reply_envelope();

    actor.model.add_connection();

    Reply::pending(async move {
        reply.send(Registered { channel_id }).await;
    })
});

// Handle disconnect
router_builder.mutate_on::<ChannelDisconnect>(|actor, env| {
    let channel_id = env.message().channel_id.clone();
    let reply = env.reply_envelope();

    actor.model.remove_connection();

    Reply::pending(async move {
        reply.send(Registered { channel_id }).await;
    })
});

// Handle user messages
router_builder.mutate_on::<UserMessage>(|actor, env| {
    let msg = env.message();
    let correlation_id = msg.correlation_id.clone();
    let conversation_id = msg.conversation_id.clone();
    let content = msg.content.clone();
    let reply = env.reply_envelope();

    let route_result = actor.model.route_message(&correlation_id, &conversation_id, &content);

    Reply::pending(async move {
        match route_result {
            Ok(()) => {
                // In a full implementation, this would forward to the conversation actor
                // and stream responses back
                reply.send(MessageComplete {
                    correlation_id,
                    conversation_id,
                    content: format!("Echo: {content}"),
                }).await;
            }
            Err(e) => {
                reply.send(ErrorResponse {
                    correlation_id,
                    message: e,
                    code: Some("ROUTE_ERROR".to_string()),
                }).await;
            }
        }
    })
});
```

### Task 5: Update Runtime to Use IPC Server

**File**: `crates/talon-core/src/runtime.rs` (modifications)

```rust
// Add to TalonRuntime:

use crate::ipc::server::{IpcServer, IpcServerConfig};

pub struct TalonRuntime {
    // ... existing fields ...
    /// IPC server handle
    ipc_server: Option<IpcServer>,
}

impl TalonRuntime {
    pub async fn new(config: RuntimeConfig) -> TalonResult<Self> {
        // ... existing initialization ...

        // Register IPC message types
        IpcServer::register_message_types(&runtime);

        // Expose router for IPC
        runtime.ipc_expose("router", router_handle.clone());

        // ... rest of initialization ...
    }

    /// Start the IPC server
    pub async fn start_ipc(&mut self) -> TalonResult<()> {
        let ipc_config = IpcServerConfig {
            socket_path: self.config.ipc_socket_path.clone(),
            max_connections: 100,
            connection_timeout_secs: 300,
        };

        let server = IpcServer::start(&mut self.runtime, ipc_config).await?;
        self.ipc_server = Some(server);
        Ok(())
    }

    /// Get IPC server statistics
    pub fn ipc_stats(&self) -> Option<IpcServerStats> {
        self.ipc_server.as_ref().map(|s| s.stats())
    }
}
```

### Task 6: Update mod.rs Exports

**File**: `crates/talon-core/src/ipc/mod.rs`

```rust
//! IPC communication for channel-to-core messaging
//!
//! This module provides:
//! - Server-side IPC listener (`server` module)
//! - Client-side IPC connector (`client` module)
//! - Message types (`messages` module)
//! - Authentication (`auth` module)
//! - Message handlers (`handlers` module)

mod auth;
mod client;
mod handlers;
mod messages;
mod server;

// Auth exports
pub use auth::{AuthToken, TokenAuthenticator, ValidatedToken};

// Handler exports
pub use handlers::{DefaultIpcHandler, IpcMessageHandler, LoggingHandler};

// Message exports
pub use messages::{
    message_types, AuthResult, ChannelAuthenticate, ChannelDisconnect, ChannelRegister,
    ErrorResponse, MessageComplete, Registered, StreamToken, UserMessage,
};

// Server exports
pub use server::{IpcServer, IpcServerConfig, IpcServerStats};

// Client exports
pub use client::{IpcClient, IpcClientConfig};
```

## Custom Error Types

No new error types needed - existing `TalonError` variants cover IPC scenarios:
- `TalonError::Ipc` - General IPC errors
- `TalonError::Unauthenticated` - Channel not authenticated
- `TalonError::AuthenticationFailed` - Token validation failed
- `TalonError::TokenExpired` - Token has expired

## Test Strategy

### Unit Tests

1. **Message Serialization**
   - Test that all IPC messages serialize/deserialize correctly
   - Test message type names match constants

2. **Server Configuration**
   - Test default socket path generation
   - Test custom configuration

3. **Client Configuration**
   - Test default configuration
   - Test retry logic

### Integration Tests

1. **Server Lifecycle**
   - Start server, verify socket created
   - Shutdown server, verify socket removed

2. **Client Connection**
   - Connect to server
   - Authenticate
   - Send messages
   - Disconnect

3. **Authentication Flow**
   - Valid token authentication
   - Invalid token rejection
   - Token expiration handling

4. **Message Routing**
   - Route message to conversation
   - Handle unknown conversation
   - Stream tokens back

## Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `crates/talon-core/src/ipc/messages.rs` | Modify | Add `#[acton_message(ipc)]` attributes |
| `crates/talon-core/src/ipc/server.rs` | Create | IPC server implementation |
| `crates/talon-core/src/ipc/client.rs` | Create | IPC client implementation |
| `crates/talon-core/src/ipc/mod.rs` | Modify | Export new modules |
| `crates/talon-core/src/router/actor.rs` | Modify | Add IPC message handlers |
| `crates/talon-core/src/runtime.rs` | Modify | Integrate IPC server |

## Dependencies

No new dependencies required. Uses existing:
- `acton-reactive` with `ipc` feature (already in workspace)
- `tokio` for async I/O
- `serde` / `serde_json` for serialization

## Semver Recommendation

**Minor version bump (0.2.0 -> 0.3.0)**

Justification:
- New public APIs (IpcServer, IpcClient, new message types)
- Backward compatible - existing code continues to work
- No breaking changes to existing public interfaces
