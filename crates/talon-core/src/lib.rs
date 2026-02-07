//! Talon Core - Secure multi-channel AI assistant daemon
//!
//! This crate provides the core runtime including:
//! - Router actor for IPC communication
//! - Conversation management
//! - SecureSkillRegistry with attestation verification
//! - Trust tier enforcement
//! - Capability-verified tool execution
//! - Token-based channel authentication

pub mod config;
pub mod conversation;
pub mod error;
pub mod ipc;
pub mod router;
pub mod runtime;
pub mod skills;
pub mod trust;
pub mod types;

pub use config::TalonConfig;
pub use error::{TalonError, TalonResult};
pub use ipc::{ChannelToCore, CoreToChannel};
// Re-export acton-ai types for convenience
pub use acton_ai::prelude::ActonAI;
pub use runtime::{RuntimeConfig, RuntimeConfigBuilder, TalonRuntime};
pub use types::*;
