//! OmniBOR ID newtype with validation
//!
//! Provides a validated wrapper around gitoid strings in the format
//! `gitoid:blob:sha256:<64-hex-chars>`. Supports conversion to/from
//! the `omnibor` crate's `ArtifactId<Sha256>` type.

use omnibor::hash_algorithm::Sha256;
use omnibor::ArtifactId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Expected prefix for all OmniBOR gitoid strings
const GITOID_PREFIX: &str = "gitoid:blob:sha256:";

/// Expected length of the hex hash portion
const HEX_HASH_LENGTH: usize = 64;

/// A validated OmniBOR ID in gitoid format
///
/// Format: `gitoid:blob:sha256:<64-hex-chars>`
///
/// This type guarantees that the contained string is a valid gitoid
/// that can be converted to an `ArtifactId<Sha256>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OmniborId(String);

impl OmniborId {
    /// Parse and validate an OmniBOR ID from a string
    ///
    /// # Errors
    ///
    /// Returns `InvalidOmniborId` if the string is not a valid gitoid format.
    pub fn parse(s: &str) -> Result<Self, InvalidOmniborId> {
        validate_gitoid(s)?;
        Ok(Self(s.to_string()))
    }

    /// Create an OmniborId from an ArtifactId
    ///
    /// This conversion is infallible since ArtifactId always produces valid gitoids.
    #[must_use]
    pub fn from_artifact_id(id: &ArtifactId<Sha256>) -> Self {
        Self(id.to_string())
    }

    /// Convert to an ArtifactId
    ///
    /// # Errors
    ///
    /// Returns error if parsing fails (should not happen for validated OmniborId).
    pub fn to_artifact_id(&self) -> Result<ArtifactId<Sha256>, InvalidOmniborId> {
        self.0.parse().map_err(|e| InvalidOmniborId {
            input: self.0.clone(),
            reason: InvalidOmniborIdReason::ParseError(format!("{e}")),
        })
    }

    /// Get the raw gitoid string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract just the hex hash portion
    #[must_use]
    pub fn hex_hash(&self) -> &str {
        &self.0[GITOID_PREFIX.len()..]
    }
}

impl fmt::Display for OmniborId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for OmniborId {
    type Err = InvalidOmniborId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<str> for OmniborId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ArtifactId<Sha256>> for OmniborId {
    fn from(id: ArtifactId<Sha256>) -> Self {
        Self::from_artifact_id(&id)
    }
}

impl TryFrom<&OmniborId> for ArtifactId<Sha256> {
    type Error = InvalidOmniborId;

    fn try_from(id: &OmniborId) -> Result<Self, Self::Error> {
        id.to_artifact_id()
    }
}

impl Serialize for OmniborId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OmniborId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Reason why an OmniBOR ID is invalid
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidOmniborIdReason {
    /// Missing or incorrect prefix
    InvalidPrefix {
        /// The expected prefix
        expected: &'static str,
        /// What was actually found
        found: String,
    },
    /// Hash portion has wrong length
    InvalidHashLength {
        /// Expected length
        expected: usize,
        /// Actual length
        actual: usize,
    },
    /// Hash contains non-hex characters
    InvalidHexCharacter {
        /// Position of the invalid character
        position: usize,
        /// The invalid character
        character: char,
    },
    /// Error parsing with omnibor crate
    ParseError(String),
}

impl fmt::Display for InvalidOmniborIdReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix { expected, found } => {
                write!(f, "expected prefix '{expected}', found '{found}'")
            }
            Self::InvalidHashLength { expected, actual } => {
                write!(f, "expected {expected} hex characters, found {actual}")
            }
            Self::InvalidHexCharacter {
                position,
                character,
            } => {
                write!(f, "invalid hex character '{character}' at position {position}")
            }
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

/// Error returned when an OmniBOR ID string is invalid
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidOmniborId {
    /// The input string that failed validation
    pub input: String,
    /// Why the input is invalid
    pub reason: InvalidOmniborIdReason,
}

impl fmt::Display for InvalidOmniborId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid OmniBOR ID '{}': {}", self.input, self.reason)
    }
}

impl std::error::Error for InvalidOmniborId {}

/// Validate a gitoid string format
fn validate_gitoid(s: &str) -> Result<(), InvalidOmniborId> {
    // Check prefix
    if !s.starts_with(GITOID_PREFIX) {
        let found = if s.len() >= GITOID_PREFIX.len() {
            s[..GITOID_PREFIX.len()].to_string()
        } else {
            s.to_string()
        };
        return Err(InvalidOmniborId {
            input: s.to_string(),
            reason: InvalidOmniborIdReason::InvalidPrefix {
                expected: GITOID_PREFIX,
                found,
            },
        });
    }

    // Check hash portion
    let hash_part = &s[GITOID_PREFIX.len()..];

    if hash_part.len() != HEX_HASH_LENGTH {
        return Err(InvalidOmniborId {
            input: s.to_string(),
            reason: InvalidOmniborIdReason::InvalidHashLength {
                expected: HEX_HASH_LENGTH,
                actual: hash_part.len(),
            },
        });
    }

    // Check all characters are valid hex
    for (i, c) in hash_part.chars().enumerate() {
        if !c.is_ascii_hexdigit() {
            return Err(InvalidOmniborId {
                input: s.to_string(),
                reason: InvalidOmniborIdReason::InvalidHexCharacter {
                    position: GITOID_PREFIX.len() + i,
                    character: c,
                },
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnibor::ArtifactIdBuilder;

    fn sample_gitoid() -> String {
        "gitoid:blob:sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
            .to_string()
    }

    #[test]
    fn test_parse_valid_gitoid() {
        let id = OmniborId::parse(&sample_gitoid());
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), sample_gitoid());
    }

    #[test]
    fn test_parse_invalid_prefix() {
        let result = OmniborId::parse("invalid:blob:sha256:a1b2c3d4");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.reason,
            InvalidOmniborIdReason::InvalidPrefix { .. }
        ));
    }

    #[test]
    fn test_parse_invalid_length() {
        let result = OmniborId::parse("gitoid:blob:sha256:tooshort");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.reason,
            InvalidOmniborIdReason::InvalidHashLength { .. }
        ));
    }

    #[test]
    fn test_parse_invalid_hex() {
        let result = OmniborId::parse(
            "gitoid:blob:sha256:gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.reason,
            InvalidOmniborIdReason::InvalidHexCharacter { .. }
        ));
    }

    #[test]
    fn test_from_artifact_id() {
        let content = b"test content";
        let artifact_id = ArtifactIdBuilder::<Sha256, _>::with_rustcrypto().identify_bytes(content);
        let omnibor_id = OmniborId::from_artifact_id(&artifact_id);

        assert!(omnibor_id.as_str().starts_with(GITOID_PREFIX));
        assert_eq!(omnibor_id.hex_hash().len(), HEX_HASH_LENGTH);
    }

    #[test]
    fn test_roundtrip_artifact_id() {
        let content = b"test content for roundtrip";
        let artifact_id = ArtifactIdBuilder::<Sha256, _>::with_rustcrypto().identify_bytes(content);
        let omnibor_id = OmniborId::from_artifact_id(&artifact_id);
        let back = omnibor_id.to_artifact_id().unwrap();

        assert_eq!(artifact_id.to_string(), back.to_string());
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = OmniborId::parse(&sample_gitoid()).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: OmniborId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_from_str() {
        let id: OmniborId = sample_gitoid().parse().unwrap();
        assert_eq!(id.as_str(), sample_gitoid());
    }

    #[test]
    fn test_display() {
        let id = OmniborId::parse(&sample_gitoid()).unwrap();
        assert_eq!(format!("{id}"), sample_gitoid());
    }

    #[test]
    fn test_hex_hash() {
        let id = OmniborId::parse(&sample_gitoid()).unwrap();
        assert_eq!(
            id.hex_hash(),
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
        );
    }
}
