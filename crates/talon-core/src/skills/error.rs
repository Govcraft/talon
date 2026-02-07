//! Skill security error types
//!
//! Custom error types for skill verification, attestation validation,
//! and capability enforcement. Follows the project's no-thiserror policy.

use std::fmt;

/// Result type alias for skill security operations
pub type SkillSecurityResult<T> = Result<T, SkillSecurityError>;

/// Errors that can occur during skill security operations
#[derive(Debug)]
pub enum SkillSecurityError {
    /// Attestation token verification failed
    AttestationVerification {
        /// The skill name that failed verification
        skill_name: String,
        /// Description of the verification failure
        reason: String,
    },

    /// Attestation token has expired
    AttestationExpired {
        /// The skill name with expired attestation
        skill_name: String,
        /// When the attestation expired
        expired_at: chrono::DateTime<chrono::Utc>,
    },

    /// Attestation not found for skill
    AttestationNotFound {
        /// The skill name without attestation
        skill_name: String,
    },

    /// OmniBOR integrity check failed
    IntegrityMismatch {
        /// The skill name that failed integrity check
        skill_name: String,
        /// Expected OmniBOR ID
        expected: String,
        /// Actual computed OmniBOR ID
        actual: String,
    },

    /// Capability not granted by attestation
    CapabilityNotGranted {
        /// The skill name requesting capability
        skill_name: String,
        /// The capability that was requested
        requested_capability: String,
        /// Capabilities that were actually granted
        granted_capabilities: Vec<String>,
    },

    /// Trust tier violation
    TrustTierViolation {
        /// The skill name with trust tier violation
        skill_name: String,
        /// Required trust tier level
        required_tier: u8,
        /// Actual trust tier level
        actual_tier: u8,
    },

    /// Registry communication error
    RegistryError {
        /// Description of the registry error
        message: String,
    },

    /// HTTP request error
    HttpError {
        /// HTTP status code if available
        status: Option<u16>,
        /// Description of the HTTP error
        message: String,
    },

    /// Skill loading error
    SkillLoadError {
        /// The skill name that failed to load
        skill_name: String,
        /// Description of the loading failure
        reason: String,
    },

    /// Skill not found
    SkillNotFound {
        /// The skill name that was not found
        skill_name: String,
    },

    /// Invalid agent URI format
    InvalidAgentUri {
        /// The invalid URI string
        uri: String,
        /// Description of what makes it invalid
        reason: String,
    },

    /// OmniBOR computation error
    OmniborError {
        /// Description of the OmniBOR error
        message: String,
    },

    /// IO error during skill operations
    IoError {
        /// Description of the IO error
        message: String,
    },

    /// Serialization/deserialization error
    SerializationError {
        /// Description of the serialization error
        message: String,
    },

    /// Cache operation error
    CacheError {
        /// Description of the cache error
        message: String,
    },

    /// Skill archive download failed
    ArchiveDownloadFailed {
        /// The agent URI of the skill whose archive failed to download
        agent_uri: String,
        /// Description of the download failure
        reason: String,
    },

    /// Skill archive parsing failed
    ArchiveParseError {
        /// The agent URI of the skill whose archive failed to parse
        agent_uri: String,
        /// Description of the parsing failure
        reason: String,
    },

    /// Trust root key fetch failed
    TrustRootFetchFailed {
        /// The trust root domain
        domain: String,
        /// Description of the fetch failure
        reason: String,
    },
}

impl fmt::Display for SkillSecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttestationVerification { skill_name, reason } => {
                write!(
                    f,
                    "attestation verification failed for skill '{skill_name}': {reason}"
                )
            }
            Self::AttestationExpired {
                skill_name,
                expired_at,
            } => {
                write!(
                    f,
                    "attestation for skill '{skill_name}' expired at {expired_at}"
                )
            }
            Self::AttestationNotFound { skill_name } => {
                write!(f, "no attestation found for skill '{skill_name}'")
            }
            Self::IntegrityMismatch {
                skill_name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "integrity check failed for skill '{skill_name}': expected {expected}, got {actual}"
                )
            }
            Self::CapabilityNotGranted {
                skill_name,
                requested_capability,
                granted_capabilities,
            } => {
                let granted = granted_capabilities.join(", ");
                write!(
                    f,
                    "capability '{requested_capability}' not granted for skill '{skill_name}' (granted: {granted})"
                )
            }
            Self::TrustTierViolation {
                skill_name,
                required_tier,
                actual_tier,
            } => {
                write!(
                    f,
                    "trust tier violation for skill '{skill_name}': required tier {required_tier}, actual tier {actual_tier}"
                )
            }
            Self::RegistryError { message } => {
                write!(f, "registry error: {message}")
            }
            Self::HttpError { status, message } => {
                if let Some(code) = status {
                    write!(f, "HTTP error {code}: {message}")
                } else {
                    write!(f, "HTTP error: {message}")
                }
            }
            Self::SkillLoadError { skill_name, reason } => {
                write!(f, "failed to load skill '{skill_name}': {reason}")
            }
            Self::SkillNotFound { skill_name } => {
                write!(f, "skill '{skill_name}' not found")
            }
            Self::InvalidAgentUri { uri, reason } => {
                write!(f, "invalid agent URI '{uri}': {reason}")
            }
            Self::OmniborError { message } => {
                write!(f, "OmniBOR error: {message}")
            }
            Self::IoError { message } => {
                write!(f, "IO error: {message}")
            }
            Self::SerializationError { message } => {
                write!(f, "serialization error: {message}")
            }
            Self::CacheError { message } => {
                write!(f, "cache error: {message}")
            }
            Self::ArchiveDownloadFailed { agent_uri, reason } => {
                write!(
                    f,
                    "failed to download skill archive for '{agent_uri}': {reason}"
                )
            }
            Self::ArchiveParseError { agent_uri, reason } => {
                write!(
                    f,
                    "failed to parse skill archive for '{agent_uri}': {reason}"
                )
            }
            Self::TrustRootFetchFailed { domain, reason } => {
                write!(
                    f,
                    "failed to fetch trust root keys for '{domain}': {reason}"
                )
            }
        }
    }
}

impl std::error::Error for SkillSecurityError {}

impl From<std::io::Error> for SkillSecurityError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for SkillSecurityError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError {
            message: e.to_string(),
        }
    }
}

impl From<reqwest::Error> for SkillSecurityError {
    fn from(e: reqwest::Error) -> Self {
        let status = e.status().map(|s| s.as_u16());
        Self::HttpError {
            status,
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attestation_verification_display() {
        let err = SkillSecurityError::AttestationVerification {
            skill_name: "git-helper".to_string(),
            reason: "signature mismatch".to_string(),
        };
        assert!(err.to_string().contains("git-helper"));
        assert!(err.to_string().contains("signature mismatch"));
    }

    #[test]
    fn test_capability_not_granted_display() {
        let err = SkillSecurityError::CapabilityNotGranted {
            skill_name: "git-helper".to_string(),
            requested_capability: "file/write".to_string(),
            granted_capabilities: vec!["file/read".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("git-helper"));
        assert!(msg.contains("file/write"));
        assert!(msg.contains("file/read"));
    }

    #[test]
    fn test_integrity_mismatch_display() {
        let err = SkillSecurityError::IntegrityMismatch {
            skill_name: "test-skill".to_string(),
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_error_implements_std_error() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<SkillSecurityError>();
    }

    #[test]
    fn test_archive_download_failed_display() {
        let err = SkillSecurityError::ArchiveDownloadFailed {
            agent_uri: "agent://talonhub.io/skill/test".to_string(),
            reason: "connection refused".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("agent://talonhub.io/skill/test"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_archive_parse_error_display() {
        let err = SkillSecurityError::ArchiveParseError {
            agent_uri: "agent://talonhub.io/skill/test".to_string(),
            reason: "invalid UTF-8".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("agent://talonhub.io/skill/test"));
        assert!(msg.contains("invalid UTF-8"));
    }

    #[test]
    fn test_trust_root_fetch_failed_display() {
        let err = SkillSecurityError::TrustRootFetchFailed {
            domain: "talonhub.io".to_string(),
            reason: "timeout".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("talonhub.io"));
        assert!(msg.contains("timeout"));
    }
}
