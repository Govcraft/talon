use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::TenantId;

/// Subscription plan tier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    #[default]
    Free,
    Pro,
    Enterprise,
}

/// Tenant status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TenantStatus {
    #[default]
    Active,
    Suspended,
    Deleted,
}

/// Resource limits for a tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSettings {
    pub max_agents: u32,
    pub max_sessions: u32,
    pub max_tokens_per_month: u64,
    pub allowed_providers: Vec<String>,
}

impl Default for TenantSettings {
    fn default() -> Self {
        Self {
            max_agents: 3,
            max_sessions: 100,
            max_tokens_per_month: 1_000_000,
            allowed_providers: vec!["ollama".to_string()],
        }
    }
}

/// A tenant in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub slug: String,
    pub status: TenantStatus,
    pub plan: Plan,
    pub settings: TenantSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new tenant.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub plan: Plan,
    #[serde(default)]
    pub settings: Option<TenantSettings>,
}

/// Request to update an existing tenant.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub status: Option<TenantStatus>,
    pub plan: Option<Plan>,
    pub settings: Option<TenantSettings>,
}
