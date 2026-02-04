//! Talon error types

use std::fmt;

/// Result type alias for Talon operations
pub type TalonResult<T> = Result<T, TalonError>;

/// Talon error enum
#[derive(Debug)]
pub enum TalonError {
    /// Configuration error
    Config {
        /// Error description
        message: String,
    },

    /// IPC communication error
    Ipc {
        /// Error description
        message: String,
    },

    /// Skill verification failed
    SkillVerification {
        /// Skill identifier
        skill: String,
        /// Reason for failure
        reason: String,
    },

    /// Attestation error
    Attestation {
        /// Error description
        message: String,
    },

    /// OmniBOR integrity check failed
    IntegrityCheck {
        /// Expected hash
        expected: String,
        /// Actual hash
        actual: String,
    },

    /// Capability not granted
    CapabilityDenied {
        /// Skill requesting capability
        skill: String,
        /// Denied capability
        capability: String,
    },

    /// Trust tier violation
    TrustTierViolation {
        /// Required trust tier
        required: u8,
        /// Actual trust tier
        actual: u8,
    },

    /// Actor communication error
    Actor {
        /// Error description
        message: String,
    },

    /// Channel error
    Channel {
        /// Channel identifier
        channel: String,
        /// Error description
        message: String,
    },

    /// IO error
    Io {
        /// Error description
        message: String,
    },

    /// Serialization error
    Serialization {
        /// Error description
        message: String,
    },
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
        Self::Io {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for TalonError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization {
            message: e.to_string(),
        }
    }
}
