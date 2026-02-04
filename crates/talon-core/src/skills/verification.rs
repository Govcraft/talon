//! Skill verification using agent-uri attestation

use crate::error::TalonResult;

/// Skill verifier using PASETO attestations
pub struct SkillVerifier {
    /// Trusted public keys
    trusted_keys: Vec<String>,
}

impl SkillVerifier {
    /// Create a new skill verifier
    #[must_use]
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
        }
    }

    /// Add a trusted public key
    pub fn add_trusted_key(&mut self, key: impl Into<String>) {
        self.trusted_keys.push(key.into());
    }

    /// Verify a skill's attestation
    ///
    /// # Errors
    ///
    /// Returns error if attestation is invalid
    pub fn verify(&self, _skill_path: &str) -> TalonResult<()> {
        // Stub implementation - will use agent-uri-attestation
        Ok(())
    }
}

impl Default for SkillVerifier {
    fn default() -> Self {
        Self::new()
    }
}
