# Talon Workspace Scaffold Plan

> Rust Implementation Plan - February 3, 2026
> Status: READY FOR IMPLEMENTATION

## Overview

This plan scaffolds the Talon workspace structure following Rust best practices:
- Cargo workspace with member crates
- TypeIDs using `mti` crate with `MagicTypeId`
- Custom error types (no anyhow/thiserror)
- Pure functions at boundaries
- Feature flags for optional functionality

## Workspace Structure

```
talon/
├── Cargo.toml                    # Workspace root
├── CLAUDE.md                     # ✓ Already exists
├── config.toml                   # Runtime configuration
├── docs/
│   ├── design/                   # ✓ Already exists
│   └── plans/                    # This file
├── crates/
│   ├── talon-core/              # Core daemon library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs         # TalonError enum
│   │       ├── types.rs         # TypeIDs (ConversationId, etc.)
│   │       ├── config.rs        # Configuration loading
│   │       ├── router/          # Router actor
│   │       │   ├── mod.rs
│   │       │   └── actor.rs
│   │       ├── conversation/    # Conversation actor
│   │       │   ├── mod.rs
│   │       │   └── actor.rs
│   │       ├── skills/          # SecureSkillRegistry
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs
│   │       │   ├── verification.rs
│   │       │   └── capabilities.rs
│   │       ├── ipc/             # IPC message types
│   │       │   ├── mod.rs
│   │       │   └── messages.rs
│   │       └── trust/           # Trust tier management
│   │           ├── mod.rs
│   │           └── tiers.rs
│   │
│   ├── talon-channels/          # Channel implementations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs         # ChannelError enum
│   │       ├── channel.rs       # Channel trait
│   │       ├── terminal/        # Terminal channel
│   │       │   ├── mod.rs
│   │       │   └── ui.rs
│   │       ├── telegram/        # Telegram channel
│   │       │   └── mod.rs
│   │       └── discord/         # Discord channel
│   │           └── mod.rs
│   │
│   ├── talon-registry/          # TalonHub HTTP registry
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── routes/
│   │       │   ├── mod.rs
│   │       │   ├── skills.rs
│   │       │   ├── publishers.rs
│   │       │   ├── discover.rs
│   │       │   └── trust_roots.rs
│   │       ├── models/
│   │       │   ├── mod.rs
│   │       │   ├── skill.rs
│   │       │   ├── publisher.rs
│   │       │   └── attestation.rs
│   │       └── handlers/
│   │           ├── mod.rs
│   │           └── skill_handler.rs
│   │
│   └── talon-cli/               # CLI binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           └── commands/
│               ├── mod.rs
│               ├── chat.rs
│               ├── skills.rs
│               └── config.rs
│
└── bin/
    ├── talon-core/              # Core daemon binary
    │   ├── Cargo.toml
    │   └── src/main.rs
    ├── talon-telegram/          # Telegram bot binary
    │   ├── Cargo.toml
    │   └── src/main.rs
    └── talon-discord/           # Discord bot binary
        ├── Cargo.toml
        └── src/main.rs
```

## Implementation Tasks

### Task 1: Workspace Root Cargo.toml

Create workspace manifest with all members.

```toml
[workspace]
resolver = "2"
members = [
    "crates/talon-core",
    "crates/talon-channels",
    "crates/talon-registry",
    "crates/talon-cli",
    "bin/talon-core",
    "bin/talon-telegram",
    "bin/talon-discord",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/Govcraft/talon"
authors = ["Roland R. Rodriguez, Jr. <rrrodzilla@proton.me>"]

[workspace.dependencies]
# Core framework
acton-ai = { version = "0.24", features = ["agent-skills"] }
acton-reactive = { version = "7.1", features = ["ipc"] }

# Identity & Security (local paths for now)
agent-uri = { path = "../agent-uri/crates/agent-uri" }
agent-uri-attestation = { path = "../agent-uri/crates/agent-uri-attestation" }
agent-uri-dht = { path = "../agent-uri/crates/agent-uri-dht" }
omnibor = "0.10"

# Async runtime
tokio = { version = "1.49", features = ["full"] }
futures = "0.3"
async-trait = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# IDs
mti = "1.1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Time
chrono = { version = "0.4", features = ["serde"] }

# Error handling (custom types, no dependencies)

# Channel-specific (optional)
teloxide = { version = "0.17", optional = true }
serenity = { version = "0.12", optional = true }
crossterm = { version = "0.29", optional = true }
ratatui = { version = "0.30", optional = true }

# Registry (acton-service)
acton-service = { version = "0.15", optional = true }
```

### Task 2: talon-core Crate

**Cargo.toml:**
```toml
[package]
name = "talon-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[features]
default = []

[dependencies]
acton-ai.workspace = true
acton-reactive.workspace = true
agent-uri.workspace = true
agent-uri-attestation.workspace = true
agent-uri-dht.workspace = true
omnibor.workspace = true
tokio.workspace = true
futures.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
mti.workspace = true
tracing.workspace = true
chrono.workspace = true
```

**src/lib.rs:**
```rust
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
```

**src/types.rs (TypeIDs):**
```rust
//! Talon type identifiers using mti crate

use mti::prelude::*;
use serde::{Deserialize, Serialize};

/// Unique conversation identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, MagicTypeId)]
#[mti(prefix = "conv")]
pub struct ConversationId(MagicId);

/// Channel identifier (e.g., "terminal", "telegram")
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Correlation ID for request/response tracking
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, MagicTypeId)]
#[mti(prefix = "corr")]
pub struct CorrelationId(MagicId);

/// Sender identity from a channel
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SenderId {
    pub channel_id: ChannelId,
    pub user_id: String,
    pub display_name: Option<String>,
}
```

**src/error.rs (Custom Error Type):**
```rust
//! Talon error types

use std::fmt;

/// Result type alias for Talon operations
pub type TalonResult<T> = Result<T, TalonError>;

/// Talon error enum
#[derive(Debug)]
pub enum TalonError {
    /// Configuration error
    Config { message: String },
    
    /// IPC communication error
    Ipc { message: String },
    
    /// Skill verification failed
    SkillVerification { skill: String, reason: String },
    
    /// Attestation error
    Attestation { message: String },
    
    /// OmniBOR integrity check failed
    IntegrityCheck { expected: String, actual: String },
    
    /// Capability not granted
    CapabilityDenied { skill: String, capability: String },
    
    /// Trust tier violation
    TrustTierViolation { required: u8, actual: u8 },
    
    /// Actor communication error
    Actor { message: String },
    
    /// Channel error
    Channel { channel: String, message: String },
    
    /// IO error
    Io { message: String },
    
    /// Serialization error
    Serialization { message: String },
}

impl fmt::Display for TalonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config { message } => write!(f, "configuration error: {message}"),
            Self::Ipc { message } => write!(f, "IPC error: {message}"),
            Self::SkillVerification { skill, reason } => {
                write!(f, "skill verification failed for {skill}: {reason}")
            }
            Self::Attestation { message } => write!(f, "attestation error: {message}"),
            Self::IntegrityCheck { expected, actual } => {
                write!(f, "integrity check failed: expected {expected}, got {actual}")
            }
            Self::CapabilityDenied { skill, capability } => {
                write!(f, "capability {capability} denied for skill {skill}")
            }
            Self::TrustTierViolation { required, actual } => {
                write!(f, "trust tier violation: required {required}, actual {actual}")
            }
            Self::Actor { message } => write!(f, "actor error: {message}"),
            Self::Channel { channel, message } => {
                write!(f, "channel {channel} error: {message}")
            }
            Self::Io { message } => write!(f, "IO error: {message}"),
            Self::Serialization { message } => write!(f, "serialization error: {message}"),
        }
    }
}

impl std::error::Error for TalonError {}

impl From<std::io::Error> for TalonError {
    fn from(e: std::io::Error) -> Self {
        Self::Io { message: e.to_string() }
    }
}

impl From<serde_json::Error> for TalonError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization { message: e.to_string() }
    }
}
```

### Task 3: talon-channels Crate

**Cargo.toml:**
```toml
[package]
name = "talon-channels"
version.workspace = true
edition.workspace = true
license.workspace = true

[features]
default = ["terminal"]
terminal = ["dep:crossterm", "dep:ratatui"]
telegram = ["dep:teloxide"]
discord = ["dep:serenity"]

[dependencies]
talon-core = { path = "../talon-core" }
tokio.workspace = true
futures.workspace = true
async-trait.workspace = true
serde.workspace = true
tracing.workspace = true

# Optional channel deps
crossterm = { workspace = true, optional = true }
ratatui = { workspace = true, optional = true }
teloxide = { workspace = true, optional = true }
serenity = { workspace = true, optional = true }
```

**src/lib.rs:**
```rust
//! Talon Channels - Multi-platform communication adapters

pub mod channel;
pub mod error;

#[cfg(feature = "terminal")]
pub mod terminal;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

pub use channel::Channel;
pub use error::{ChannelError, ChannelResult};
```

**src/channel.rs (Channel Trait):**
```rust
//! Core Channel trait

use async_trait::async_trait;
use talon_core::{ChannelId, ConversationId, SenderId};
use tokio::sync::mpsc;

use crate::error::ChannelResult;

/// Message content variants
#[derive(Clone, Debug)]
pub enum MessageContent {
    Text(String),
    Markdown(String),
    Image { url: String, caption: Option<String> },
}

/// Inbound message from user
#[derive(Clone, Debug)]
pub struct InboundMessage {
    pub conversation_id: ConversationId,
    pub sender: SenderId,
    pub content: MessageContent,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Outbound message to user
#[derive(Clone, Debug)]
pub struct OutboundMessage {
    pub conversation_id: ConversationId,
    pub content: MessageContent,
    pub reply_to: Option<String>,
}

/// Channel trait - implement for each platform
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique channel identifier
    fn id(&self) -> ChannelId;

    /// Start receiving messages
    async fn start(&self, sender: mpsc::Sender<InboundMessage>) -> ChannelResult<()>;

    /// Send a message to a conversation
    async fn send(&self, message: OutboundMessage) -> ChannelResult<()>;

    /// Send streaming token
    async fn send_token(&self, conversation_id: &ConversationId, token: &str) -> ChannelResult<()>;

    /// Stop the channel
    async fn stop(&self) -> ChannelResult<()>;
}
```

### Task 4: talon-registry Crate

**Cargo.toml:**
```toml
[package]
name = "talon-registry"
version.workspace = true
edition.workspace = true
license = "Proprietary"

[dependencies]
talon-core = { path = "../talon-core" }
acton-service = { version = "0.15", features = [
    "http",
    "database",
    "cache",
    "observability",
    "otel-metrics",
    "openapi",
    "governor",
    "pagination-full",
    "jwt",
    "auth",
] }
agent-uri.workspace = true
agent-uri-attestation.workspace = true
omnibor.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
chrono.workspace = true
```

### Task 5: talon-cli Crate

**Cargo.toml:**
```toml
[package]
name = "talon-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
talon-core = { path = "../talon-core" }
talon-channels = { path = "../talon-channels", features = ["terminal"] }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
clap = { version = "4", features = ["derive"] }
```

### Task 6: Binary Crates

**bin/talon-core/Cargo.toml:**
```toml
[package]
name = "talon-daemon"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "talon-daemon"
path = "src/main.rs"

[dependencies]
talon-core = { path = "../../crates/talon-core" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

**bin/talon-telegram/Cargo.toml:**
```toml
[package]
name = "talon-telegram"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "talon-telegram"
path = "src/main.rs"

[dependencies]
talon-core = { path = "../../crates/talon-core" }
talon-channels = { path = "../../crates/talon-channels", features = ["telegram"] }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

**bin/talon-discord/Cargo.toml:**
```toml
[package]
name = "talon-discord"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "talon-discord"
path = "src/main.rs"

[dependencies]
talon-core = { path = "../../crates/talon-core" }
talon-channels = { path = "../../crates/talon-channels", features = ["discord"] }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

## Implementation Order

1. **Workspace root** - Cargo.toml with workspace config
2. **talon-core** - Types, errors, config (stub modules)
3. **talon-channels** - Channel trait, terminal stub
4. **talon-registry** - Stub with acton-service
5. **talon-cli** - CLI with clap
6. **Binaries** - Main entry points

## Verification

After scaffold creation:
```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

## Notes

- Local paths for agent-uri crates assume ~/projects/agent-uri exists on matilda
- All crates use workspace inheritance for version/edition/license
- Features control optional channel implementations
- talon-registry is proprietary license (hosted service)
