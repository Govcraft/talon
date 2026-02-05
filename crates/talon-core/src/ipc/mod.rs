//! IPC message types for channel communication
//!
//! Defines the protocol for communication between channel binaries
//! and the core daemon over Unix Domain Sockets, including:
//!
//! - Message types for channel-to-core and core-to-channel communication
//! - HMAC-SHA256 token authentication
//! - Message handlers for processing incoming requests

mod auth;
mod handlers;
mod messages;

pub use auth::{AuthToken, TokenAuthenticator, ValidatedToken};
pub use handlers::{DefaultIpcHandler, IpcMessageHandler, LoggingHandler};
pub use messages::{ChannelToCore, CoreToChannel};
