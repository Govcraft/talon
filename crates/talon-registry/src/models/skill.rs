//! Skill model

use serde::{Deserialize, Serialize};

/// Registered skill in the registry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill identifier
    pub id: String,
    /// Skill name
    pub name: String,
    /// Skill description
    pub description: String,
    /// Publisher ID
    pub publisher_id: String,
    /// Required trust tier
    pub trust_tier: u8,
    /// OmniBOR artifact ID for integrity
    pub omnibor_id: String,
    /// Agent URI
    pub agent_uri: String,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
