//! Channel error types

use std::fmt;

/// Result type alias for channel operations
pub type ChannelResult<T> = Result<T, ChannelError>;

/// Channel error enum
#[derive(Debug)]
pub enum ChannelError {
    /// Connection error
    Connection {
        /// Error description
        message: String,
    },

    /// Send error
    Send {
        /// Error description
        message: String,
    },

    /// Receive error
    Receive {
        /// Error description
        message: String,
    },

    /// Channel not started
    NotStarted,

    /// Channel already started
    AlreadyStarted,

    /// IO error
    Io {
        /// Error description
        message: String,
    },

    /// Platform-specific error
    Platform {
        /// Platform name
        platform: String,
        /// Error description
        message: String,
    },
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection { message } => write!(f, "connection error: {message}"),
            Self::Send { message } => write!(f, "send error: {message}"),
            Self::Receive { message } => write!(f, "receive error: {message}"),
            Self::NotStarted => write!(f, "channel not started"),
            Self::AlreadyStarted => write!(f, "channel already started"),
            Self::Io { message } => write!(f, "IO error: {message}"),
            Self::Platform { platform, message } => {
                write!(f, "{platform} error: {message}")
            }
        }
    }
}

impl std::error::Error for ChannelError {}

impl From<std::io::Error> for ChannelError {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            message: e.to_string(),
        }
    }
}
