//! Registry error types

use std::fmt;

/// Result type alias for registry operations
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Registry error enum
#[derive(Debug)]
pub enum RegistryError {
    /// Skill not found
    SkillNotFound {
        /// Skill identifier
        skill_id: String,
    },

    /// Publisher not found
    PublisherNotFound {
        /// Publisher identifier
        publisher_id: String,
    },

    /// Invalid attestation
    InvalidAttestation {
        /// Error description
        message: String,
    },

    /// Unauthorized
    Unauthorized {
        /// Error description
        message: String,
    },

    /// Database error
    Database {
        /// Error description
        message: String,
    },

    /// Validation error
    Validation {
        /// Field name
        field: String,
        /// Error description
        message: String,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SkillNotFound { skill_id } => write!(f, "skill not found: {skill_id}"),
            Self::PublisherNotFound { publisher_id } => {
                write!(f, "publisher not found: {publisher_id}")
            }
            Self::InvalidAttestation { message } => write!(f, "invalid attestation: {message}"),
            Self::Unauthorized { message } => write!(f, "unauthorized: {message}"),
            Self::Database { message } => write!(f, "database error: {message}"),
            Self::Validation { field, message } => {
                write!(f, "validation error for {field}: {message}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}
