//! IPC client for channel communication with talon-core
//!
//! This module provides a high-level IPC client for channels to communicate
//! with the talon-core daemon over Unix Domain Sockets.
//!
//! # Architecture
//!
//! ```text
//! TelegramChannel ─┬─► IpcClient ─► UDS ─► IpcServer ─► Router
//! DiscordChannel  ─┤                                      │
//! TerminalChannel ─┘                                      ▼
//!                                                    LLM Provider
//! ```
//!
//! # Protocol
//!
//! Messages are framed with a 4-byte big-endian length prefix followed by
//! a JSON payload. The maximum message size is 16 MiB.
//!
//! # Usage
//!
//! ```ignore
//! use talon_channels::ipc::{IpcClient, IpcClientConfig};
//! use talon_core::ChannelId;
//!
//! // Create client configuration
//! let config = IpcClientConfig::new(
//!     ChannelId::new("telegram"),
//!     "auth-token-here",
//! );
//!
//! // Create and connect the client
//! let client = IpcClient::new(config);
//! client.connect().await?;
//! client.authenticate().await?;
//! client.register().await?;
//!
//! // Start the receive loop for streaming responses
//! let _handle = client.start_receive_loop();
//!
//! // Send messages
//! let correlation_id = client.send_message(
//!     conversation_id,
//!     sender,
//!     "Hello, world!".to_string(),
//! ).await?;
//! ```

mod client;
mod config;
mod connection;
mod error;

pub use client::{ClientState, CompleteCallback, ErrorCallback, IpcClient, TokenCallback};
pub use config::{IpcClientConfig, IpcClientConfigBuilder};
pub use connection::{IpcConnection, IpcReader, IpcWriter, MAX_MESSAGE_SIZE};
pub use error::{IpcClientError, IpcClientResult};
