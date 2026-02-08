//! Generated protobuf types and gRPC service stubs.
//!
//! The module structure mirrors the proto package hierarchy:
//! - `talon` (common types like SessionKey, Attachment)
//!   - `talon.gateway` (gateway service stubs)
//!   - `talon.channel` (channel service stubs)
//!
//! The generated gateway and channel code references common types via `super::`,
//! so they must be nested inside the common module.

/// Common types and nested service modules.
pub mod common {
    // Include common types (SessionKey, Attachment, Timestamp)
    tonic::include_proto!("talon");

    /// Gateway service (channel -> gateway communication).
    pub mod gateway {
        tonic::include_proto!("talon.gateway");
    }

    /// Channel service (gateway -> channel push notifications).
    pub mod channel {
        tonic::include_proto!("talon.channel");
    }
}

// Re-export commonly used types at a convenient level.
pub use common::channel;
pub use common::gateway;
pub use common::{Attachment, SessionKey, Timestamp};
