//! Attestation model

use serde::{Deserialize, Serialize};

/// Skill attestation record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attestation {
    /// Attestation ID
    pub id: String,
    /// Skill ID this attestation is for
    pub skill_id: String,
    /// Attestation type (manifest, publisher, code_review, security_audit)
    pub attestation_type: String,
    /// PASETO token
    pub token: String,
    /// Issuer public key
    pub issuer_key: String,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Expiration timestamp
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}
