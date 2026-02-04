//! Talon Core - Secure multi-channel AI assistant daemon
//!
//! This crate provides the core runtime including:
//! - Router actor for IPC communication
//! - Conversation management
//! - SecureSkillRegistry with attestation verification
//! - Trust tier enforcement

pub mod config;
pub mod conversation;
pub mod error;
pub mod ipc;
pub mod router;
pub mod skills;
pub mod trust;
pub mod types;

pub use config::TalonConfig;
pub use error::{TalonError, TalonResult};
pub use types::*;
