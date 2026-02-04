//! Publisher model

use serde::{Deserialize, Serialize};

/// Verified skill publisher
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Publisher {
    /// Unique publisher identifier
    pub id: String,
    /// Publisher name
    pub name: String,
    /// Public key for attestation verification
    pub public_key: String,
    /// Publisher trust level
    pub trust_level: u8,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}
