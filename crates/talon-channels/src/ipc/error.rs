//! IPC client error types

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Result type alias for IPC client operations
pub type IpcClientResult<T> = Result<T, IpcClientError>;

/// IPC client error enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcClientError {
    /// Failed to connect to the IPC socket
    ConnectionFailed {
        /// Path to the socket
        socket_path: PathBuf,
        /// Error description
        message: String,
    },

    /// Client is not connected
    NotConnected,

    /// Client is already connected
    AlreadyConnected,

    /// Authentication failed
    AuthenticationFailed {
        /// Reason for failure
        reason: String,
    },

    /// Failed to send message
    SendFailed {
        /// Error description
        message: String,
    },

    /// Failed to receive message
    ReceiveFailed {
        /// Error description
        message: String,
    },

    /// Serialization error
    Serialization {
        /// Error description
        message: String,
    },

    /// Deserialization error
    Deserialization {
        /// Error description
        message: String,
    },

    /// Connection was closed unexpectedly
    ConnectionClosed,

    /// Operation timed out
    Timeout {
        /// Operation that timed out
        operation: String,
        /// Timeout duration
        duration: Duration,
    },

    /// Invalid frame received
    InvalidFrame {
        /// Error description
        message: String,
    },

    /// Client is not registered
    NotRegistered,

    /// Invalid client state for operation
    InvalidState {
        /// Current state
        current: String,
        /// Expected state
        expected: String,
    },

    /// Message too large
    MessageTooLarge {
        /// Message size
        size: usize,
        /// Maximum allowed size
        max_size: usize,
    },
}

impl fmt::Display for IpcClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionFailed { socket_path, message } => {
                write!(f, "failed to connect to {}: {message}", socket_path.display())
            }
            Self::NotConnected => write!(f, "client is not connected"),
            Self::AlreadyConnected => write!(f, "client is already connected"),
            Self::AuthenticationFailed { reason } => {
                write!(f, "authentication failed: {reason}")
            }
            Self::SendFailed { message } => write!(f, "send failed: {message}"),
            Self::ReceiveFailed { message } => write!(f, "receive failed: {message}"),
            Self::Serialization { message } => write!(f, "serialization error: {message}"),
            Self::Deserialization { message } => write!(f, "deserialization error: {message}"),
            Self::ConnectionClosed => write!(f, "connection closed unexpectedly"),
            Self::Timeout { operation, duration } => {
                write!(f, "{operation} timed out after {duration:?}")
            }
            Self::InvalidFrame { message } => write!(f, "invalid frame: {message}"),
            Self::NotRegistered => write!(f, "client is not registered"),
            Self::InvalidState { current, expected } => {
                write!(f, "invalid state: currently {current}, expected {expected}")
            }
            Self::MessageTooLarge { size, max_size } => {
                write!(f, "message too large: {size} bytes exceeds maximum {max_size} bytes")
            }
        }
    }
}

impl std::error::Error for IpcClientError {}

impl From<std::io::Error> for IpcClientError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::NotFound
            | std::io::ErrorKind::PermissionDenied => Self::ConnectionFailed {
                socket_path: PathBuf::new(),
                message: e.to_string(),
            },
            std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe => Self::ConnectionClosed,
            std::io::ErrorKind::TimedOut => Self::Timeout {
                operation: "IO operation".to_string(),
                duration: Duration::ZERO,
            },
            std::io::ErrorKind::UnexpectedEof => Self::ConnectionClosed,
            _ => Self::SendFailed {
                message: e.to_string(),
            },
        }
    }
}

impl From<serde_json::Error> for IpcClientError {
    fn from(e: serde_json::Error) -> Self {
        if e.is_data() || e.is_syntax() || e.is_eof() {
            Self::Deserialization {
                message: e.to_string(),
            }
        } else {
            Self::Serialization {
                message: e.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = IpcClientError::NotConnected;
        assert_eq!(err.to_string(), "client is not connected");

        let err = IpcClientError::ConnectionFailed {
            socket_path: PathBuf::from("/tmp/test.sock"),
            message: "connection refused".to_string(),
        };
        assert!(err.to_string().contains("/tmp/test.sock"));
        assert!(err.to_string().contains("connection refused"));

        let err = IpcClientError::Timeout {
            operation: "connect".to_string(),
            duration: Duration::from_secs(5),
        };
        assert!(err.to_string().contains("connect"));
        assert!(err.to_string().contains("5s"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection refused",
        );
        let err: IpcClientError = io_err.into();
        assert!(matches!(err, IpcClientError::ConnectionFailed { .. }));

        let io_err = std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken pipe",
        );
        let err: IpcClientError = io_err.into();
        assert!(matches!(err, IpcClientError::ConnectionClosed));
    }

    #[test]
    fn test_error_equality() {
        assert_eq!(IpcClientError::NotConnected, IpcClientError::NotConnected);
        assert_ne!(IpcClientError::NotConnected, IpcClientError::AlreadyConnected);
    }
}
