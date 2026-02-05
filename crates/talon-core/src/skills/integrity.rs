//! OmniBOR integrity verification
//!
//! Pure functions for computing and verifying OmniBOR artifact IDs
//! for skill content integrity checking.

use std::path::Path;

use omnibor::hash_algorithm::Sha256;
use omnibor::ArtifactId;
use omnibor::ArtifactIdBuilder;

use crate::skills::error::{SkillSecurityError, SkillSecurityResult};
use crate::skills::omnibor_id::OmniborId;

/// Result of an integrity verification check
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrityResult {
    /// Content integrity verified - hashes match
    Verified,

    /// Content integrity check failed - hashes do not match
    Mismatch {
        /// The expected OmniBOR ID from attestation
        expected: OmniborId,
        /// The computed OmniBOR ID from content
        computed: OmniborId,
    },

    /// Integrity check was not performed (no expected ID available)
    NotChecked,
}

impl IntegrityResult {
    /// Check if verification passed
    #[must_use]
    pub fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }

    /// Check if there was a mismatch
    #[must_use]
    pub fn is_mismatch(&self) -> bool {
        matches!(self, Self::Mismatch { .. })
    }

    /// Check if verification was skipped
    #[must_use]
    pub fn is_not_checked(&self) -> bool {
        matches!(self, Self::NotChecked)
    }
}

/// Compute an ArtifactId from raw bytes
///
/// This is the core hashing operation using SHA-256 gitoid format.
#[must_use]
pub fn compute_artifact_id(content: &[u8]) -> ArtifactId<Sha256> {
    ArtifactIdBuilder::<Sha256, _>::with_rustcrypto().identify_bytes(content)
}

/// Compute the OmniBOR ID for a skill's SKILL.md file
///
/// # Arguments
///
/// * `skill_path` - Path to the skill directory containing SKILL.md
///
/// # Errors
///
/// Returns error if the SKILL.md file cannot be read.
pub fn compute_skill_omnibor_id(skill_path: &Path) -> SkillSecurityResult<OmniborId> {
    let skill_md_path = skill_path.join("SKILL.md");

    let content = std::fs::read(&skill_md_path).map_err(|e| SkillSecurityError::IoError {
        message: format!("failed to read {}: {}", skill_md_path.display(), e),
    })?;

    let artifact_id = compute_artifact_id(&content);
    Ok(OmniborId::from_artifact_id(&artifact_id))
}

/// Verify content integrity by comparing expected and computed OmniBOR IDs
///
/// # Arguments
///
/// * `expected` - The expected OmniBOR ID from attestation (None if not available)
/// * `computed` - The computed OmniBOR ID from content
///
/// # Returns
///
/// - `IntegrityResult::Verified` if both IDs match
/// - `IntegrityResult::Mismatch` if IDs don't match (with both values)
/// - `IntegrityResult::NotChecked` if no expected ID was provided
#[must_use]
pub fn verify_integrity(expected: Option<&OmniborId>, computed: &OmniborId) -> IntegrityResult {
    match expected {
        Some(expected_id) => {
            if expected_id == computed {
                IntegrityResult::Verified
            } else {
                IntegrityResult::Mismatch {
                    expected: expected_id.clone(),
                    computed: computed.clone(),
                }
            }
        }
        None => IntegrityResult::NotChecked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn sample_omnibor_id(suffix: &str) -> OmniborId {
        // Use compute_artifact_id to generate valid IDs
        let content = format!("test content {suffix}");
        let artifact_id = compute_artifact_id(content.as_bytes());
        OmniborId::from_artifact_id(&artifact_id)
    }

    #[test]
    fn test_compute_artifact_id() {
        let content = b"hello world";
        let id = compute_artifact_id(content);

        // Should produce a valid gitoid string
        let id_str = id.to_string();
        assert!(id_str.starts_with("gitoid:blob:sha256:"));
    }

    #[test]
    fn test_compute_artifact_id_deterministic() {
        let content = b"deterministic content";
        let id1 = compute_artifact_id(content);
        let id2 = compute_artifact_id(content);

        assert_eq!(id1.to_string(), id2.to_string());
    }

    #[test]
    fn test_compute_artifact_id_different_content() {
        let id1 = compute_artifact_id(b"content one");
        let id2 = compute_artifact_id(b"content two");

        assert_ne!(id1.to_string(), id2.to_string());
    }

    #[test]
    fn test_compute_skill_omnibor_id() {
        let dir = TempDir::new().unwrap();
        let skill_md_path = dir.path().join("SKILL.md");

        let mut file = std::fs::File::create(&skill_md_path).unwrap();
        writeln!(file, "# Test Skill").unwrap();
        writeln!(file, "Description of the skill").unwrap();

        let result = compute_skill_omnibor_id(dir.path());
        assert!(result.is_ok());

        let omnibor_id = result.unwrap();
        assert!(omnibor_id.as_str().starts_with("gitoid:blob:sha256:"));
    }

    #[test]
    fn test_compute_skill_omnibor_id_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = compute_skill_omnibor_id(dir.path());

        assert!(result.is_err());
        match result.unwrap_err() {
            SkillSecurityError::IoError { message } => {
                assert!(message.contains("SKILL.md"));
            }
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn test_verify_integrity_verified() {
        let id = sample_omnibor_id("same");
        let result = verify_integrity(Some(&id), &id);

        assert!(result.is_verified());
        assert!(!result.is_mismatch());
        assert!(!result.is_not_checked());
    }

    #[test]
    fn test_verify_integrity_mismatch() {
        let expected = sample_omnibor_id("expected");
        let computed = sample_omnibor_id("computed");
        let result = verify_integrity(Some(&expected), &computed);

        assert!(!result.is_verified());
        assert!(result.is_mismatch());
        assert!(!result.is_not_checked());

        if let IntegrityResult::Mismatch {
            expected: e,
            computed: c,
        } = result
        {
            assert_eq!(e, expected);
            assert_eq!(c, computed);
        } else {
            panic!("expected Mismatch variant");
        }
    }

    #[test]
    fn test_verify_integrity_not_checked() {
        let computed = sample_omnibor_id("computed");
        let result = verify_integrity(None, &computed);

        assert!(!result.is_verified());
        assert!(!result.is_mismatch());
        assert!(result.is_not_checked());
    }

    #[test]
    fn test_integrity_result_equality() {
        assert_eq!(IntegrityResult::Verified, IntegrityResult::Verified);
        assert_eq!(IntegrityResult::NotChecked, IntegrityResult::NotChecked);

        let id1 = sample_omnibor_id("one");
        let id2 = sample_omnibor_id("two");
        assert_eq!(
            IntegrityResult::Mismatch {
                expected: id1.clone(),
                computed: id2.clone()
            },
            IntegrityResult::Mismatch {
                expected: id1,
                computed: id2
            }
        );
    }
}
