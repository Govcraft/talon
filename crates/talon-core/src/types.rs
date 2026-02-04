//! Talon type identifiers using mti crate

use mti::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Unique conversation identifier
///
/// Uses TypeID format for human-readable, time-sortable, globally unique IDs.
/// Example: `conv_01h455vb4pex5vsknk084sn02q`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(MagicTypeId);

/// Error returned when attempting to create an invalid conversation ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidConversationId {
    /// TypeID parsing failed
    Parse(String),
    /// Wrong prefix (expected "conv")
    WrongPrefix {
        /// The expected prefix
        expected: &'static str,
        /// The actual prefix found
        actual: String,
    },
}

impl fmt::Display for InvalidConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "invalid conversation ID: {e}"),
            Self::WrongPrefix { expected, actual } => {
                write!(f, "expected prefix '{expected}', got '{actual}'")
            }
        }
    }
}

impl std::error::Error for InvalidConversationId {}

impl ConversationId {
    /// The TypeID prefix for conversation identifiers
    pub const PREFIX: &'static str = "conv";

    /// Creates a new conversation ID with a fresh UUIDv7 (time-sortable)
    #[must_use]
    pub fn new() -> Self {
        Self(Self::PREFIX.create_type_id::<V7>())
    }

    /// Parses a conversation ID from a string, validating the prefix
    ///
    /// # Errors
    ///
    /// Returns `InvalidConversationId::Parse` if the string is not a valid TypeID format.
    /// Returns `InvalidConversationId::WrongPrefix` if the TypeID has a different prefix.
    pub fn parse(s: &str) -> Result<Self, InvalidConversationId> {
        let id =
            MagicTypeId::from_str(s).map_err(|e| InvalidConversationId::Parse(e.to_string()))?;

        let prefix = id.prefix().as_str();
        if prefix != Self::PREFIX {
            return Err(InvalidConversationId::WrongPrefix {
                expected: Self::PREFIX,
                actual: prefix.to_string(),
            });
        }

        Ok(Self(id))
    }

    /// Returns a reference to the underlying MagicTypeId
    #[must_use]
    pub fn inner(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ConversationId {
    type Err = InvalidConversationId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<MagicTypeId> for ConversationId {
    fn as_ref(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Serialize for ConversationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConversationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Channel identifier (e.g., "terminal", "telegram")
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    /// Create a new channel identifier
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the channel identifier as a string slice
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Correlation ID for request/response tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(MagicTypeId);

/// Error returned when attempting to create an invalid correlation ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidCorrelationId {
    /// TypeID parsing failed
    Parse(String),
    /// Wrong prefix
    WrongPrefix {
        /// The expected prefix
        expected: &'static str,
        /// The actual prefix found
        actual: String,
    },
}

impl fmt::Display for InvalidCorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "invalid correlation ID: {e}"),
            Self::WrongPrefix { expected, actual } => {
                write!(f, "expected prefix '{expected}', got '{actual}'")
            }
        }
    }
}

impl std::error::Error for InvalidCorrelationId {}

impl CorrelationId {
    /// The TypeID prefix for correlation identifiers
    pub const PREFIX: &'static str = "corr";

    /// Creates a new correlation ID with a fresh UUIDv7 (time-sortable)
    #[must_use]
    pub fn new() -> Self {
        Self(Self::PREFIX.create_type_id::<V7>())
    }

    /// Parses a correlation ID from a string, validating the prefix
    ///
    /// # Errors
    ///
    /// Returns error if the string is not a valid TypeID or has wrong prefix
    pub fn parse(s: &str) -> Result<Self, InvalidCorrelationId> {
        let id =
            MagicTypeId::from_str(s).map_err(|e| InvalidCorrelationId::Parse(e.to_string()))?;

        let prefix = id.prefix().as_str();
        if prefix != Self::PREFIX {
            return Err(InvalidCorrelationId::WrongPrefix {
                expected: Self::PREFIX,
                actual: prefix.to_string(),
            });
        }

        Ok(Self(id))
    }

    /// Returns a reference to the underlying MagicTypeId
    #[must_use]
    pub fn inner(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CorrelationId {
    type Err = InvalidCorrelationId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<MagicTypeId> for CorrelationId {
    fn as_ref(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Serialize for CorrelationId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CorrelationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Sender identity from a channel
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SenderId {
    /// Channel the sender is on
    pub channel_id: ChannelId,
    /// Platform-specific user identifier
    pub user_id: String,
    /// Optional display name
    pub display_name: Option<String>,
}

impl SenderId {
    /// Create a new sender identity
    #[must_use]
    pub fn new(channel_id: ChannelId, user_id: impl Into<String>) -> Self {
        Self {
            channel_id,
            user_id: user_id.into(),
            display_name: None,
        }
    }

    /// Set the display name
    #[must_use]
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_conversation_id_has_correct_prefix() {
        let id = ConversationId::new();
        assert!(id.to_string().starts_with("conv_"));
    }

    #[test]
    fn new_correlation_id_has_correct_prefix() {
        let id = CorrelationId::new();
        assert!(id.to_string().starts_with("corr_"));
    }

    #[test]
    fn channel_id_display() {
        let id = ChannelId::new("terminal");
        assert_eq!(id.to_string(), "terminal");
    }
}
