use thiserror::Error;

/// Domain errors shared across Talon services.
#[derive(Debug, Error)]
pub enum TalonError {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("tenant not found: {0}")]
    TenantNotFound(String),

    #[error("agent not found: {0}")]
    AgentNotFound(String),

    #[error("inference failed: {0}")]
    InferenceFailed(String),

    #[error("channel error: {0}")]
    ChannelError(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("database error: {0}")]
    DatabaseError(String),

    #[error("authorization denied: {0}")]
    Unauthorized(String),

    #[error("rate limit exceeded")]
    RateLimitExceeded,

    #[error("internal error: {0}")]
    Internal(String),
}
