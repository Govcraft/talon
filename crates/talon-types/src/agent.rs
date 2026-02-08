//! Agent domain types for the Talon AI gateway.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AgentId, TenantId, TrustTier};

/// An AI agent configured for a specific tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub tenant_id: TenantId,
    pub name: String,
    pub system_prompt: Option<String>,
    pub provider: String,
    pub model: String,
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub trust_tier: TrustTier,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new agent.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub trust_tier: TrustTier,
    #[serde(default)]
    pub is_default: bool,
}

fn default_provider() -> String {
    "ollama".to_string()
}
fn default_model() -> String {
    "llama3.2".to_string()
}

/// Request to update an existing agent.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub system_prompt: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub trust_tier: Option<TrustTier>,
    pub is_default: Option<bool>,
}
