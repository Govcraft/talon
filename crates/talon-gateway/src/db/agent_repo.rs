//! Agent repository -- CRUD operations in a tenant namespace.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::DbClient;
use crate::error::GatewayError;
use talon_types::{Agent, AgentId, CreateAgentRequest, TenantId, TrustTier, UpdateAgentRequest};

/// Internal row representation for SurrealDB serialisation.
#[derive(Debug, Serialize)]
struct AgentRecord {
    id: String,
    tenant_id: String,
    name: String,
    system_prompt: Option<String>,
    provider: String,
    model: String,
    tools: Vec<String>,
    skills: Vec<String>,
    trust_tier: String,
    is_default: bool,
    created_at: String,
    updated_at: String,
}

/// Internal row representation for SurrealDB deserialisation.
#[derive(Debug, Deserialize)]
struct AgentRow {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    tenant_id: String,
    name: String,
    #[serde(default)]
    system_prompt: Option<String>,
    provider: String,
    model: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    trust_tier: String,
    #[serde(default)]
    is_default: bool,
    created_at: String,
    updated_at: String,
}

impl AgentRow {
    fn into_agent(self) -> Agent {
        let raw_id = self.id.id.to_raw();
        let trust_tier = self.trust_tier.parse::<TrustTier>().unwrap_or_default();
        Agent {
            id: AgentId::new(raw_id),
            tenant_id: TenantId::new(self.tenant_id),
            name: self.name,
            system_prompt: self.system_prompt,
            provider: self.provider,
            model: self.model,
            tools: self.tools,
            skills: self.skills,
            trust_tier,
            is_default: self.is_default,
            created_at: self.created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: self.updated_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

/// Create a new agent in the given tenant namespace.
#[tracing::instrument(skip(client))]
pub async fn create_agent(
    client: &DbClient,
    tenant_ns: &str,
    tenant_id: &TenantId,
    req: &CreateAgentRequest,
) -> std::result::Result<Agent, GatewayError> {
    let agent_id = AgentId::generate();
    let now = Utc::now();
    let record = AgentRecord {
        id: agent_id.as_str().to_string(),
        tenant_id: tenant_id.as_str().to_string(),
        name: req.name.clone(),
        system_prompt: req.system_prompt.clone(),
        provider: req.provider.clone(),
        model: req.model.clone(),
        tools: req.tools.clone(),
        skills: req.skills.clone(),
        trust_tier: req.trust_tier.to_string(),
        is_default: req.is_default,
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    };

    let data = serde_json::to_value(&record).map_err(|e| GatewayError::Internal(e.to_string()))?;

    let query =
        format!("USE NS `{tenant_ns}` DB main; CREATE type::thing('agent', $id) CONTENT $data");
    let mut result = client
        .query(&query)
        .bind(("id", agent_id.as_str().to_string()))
        .bind(("data", data))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AgentRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    rows.into_iter()
        .next()
        .map(AgentRow::into_agent)
        .ok_or_else(|| GatewayError::Internal("failed to create agent record".into()))
}

/// List all agents in a tenant namespace.
#[tracing::instrument(skip(client))]
pub async fn list_agents(
    client: &DbClient,
    tenant_ns: &str,
) -> std::result::Result<Vec<Agent>, GatewayError> {
    let query =
        format!("USE NS `{tenant_ns}` DB main; SELECT * FROM agent ORDER BY created_at DESC");
    let mut result = client
        .query(&query)
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AgentRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(AgentRow::into_agent).collect())
}

/// Get an agent by ID.
#[tracing::instrument(skip(client))]
pub async fn get_agent(
    client: &DbClient,
    tenant_ns: &str,
    agent_id: &AgentId,
) -> std::result::Result<Option<Agent>, GatewayError> {
    let query = format!("USE NS `{tenant_ns}` DB main; SELECT * FROM type::thing('agent', $id)");
    let mut result = client
        .query(&query)
        .bind(("id", agent_id.as_str().to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AgentRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(AgentRow::into_agent))
}

/// Update an agent.
#[tracing::instrument(skip(client))]
pub async fn update_agent(
    client: &DbClient,
    tenant_ns: &str,
    agent_id: &AgentId,
    req: &UpdateAgentRequest,
) -> std::result::Result<Option<Agent>, GatewayError> {
    let mut merge = serde_json::Map::new();
    if let Some(ref name) = req.name {
        merge.insert("name".into(), serde_json::Value::String(name.clone()));
    }
    if let Some(ref system_prompt) = req.system_prompt {
        merge.insert(
            "system_prompt".into(),
            serde_json::Value::String(system_prompt.clone()),
        );
    }
    if let Some(ref provider) = req.provider {
        merge.insert(
            "provider".into(),
            serde_json::Value::String(provider.clone()),
        );
    }
    if let Some(ref model) = req.model {
        merge.insert("model".into(), serde_json::Value::String(model.clone()));
    }
    if let Some(ref tools) = req.tools {
        merge.insert(
            "tools".into(),
            serde_json::to_value(tools).map_err(|e| GatewayError::Internal(e.to_string()))?,
        );
    }
    if let Some(ref skills) = req.skills {
        merge.insert(
            "skills".into(),
            serde_json::to_value(skills).map_err(|e| GatewayError::Internal(e.to_string()))?,
        );
    }
    if let Some(trust_tier) = req.trust_tier {
        merge.insert(
            "trust_tier".into(),
            serde_json::Value::String(trust_tier.to_string()),
        );
    }
    if let Some(is_default) = req.is_default {
        merge.insert("is_default".into(), serde_json::Value::Bool(is_default));
    }
    merge.insert(
        "updated_at".into(),
        serde_json::Value::String(Utc::now().to_rfc3339()),
    );

    let merge_value = serde_json::Value::Object(merge);

    let query =
        format!("USE NS `{tenant_ns}` DB main; UPDATE type::thing('agent', $id) MERGE $data");
    let mut result = client
        .query(&query)
        .bind(("id", agent_id.as_str().to_string()))
        .bind(("data", merge_value))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AgentRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(AgentRow::into_agent))
}

/// Delete an agent by ID. Returns true if a record was deleted.
#[tracing::instrument(skip(client))]
pub async fn delete_agent(
    client: &DbClient,
    tenant_ns: &str,
    agent_id: &AgentId,
) -> std::result::Result<bool, GatewayError> {
    let query =
        format!("USE NS `{tenant_ns}` DB main; DELETE type::thing('agent', $id) RETURN BEFORE");
    let mut result = client
        .query(&query)
        .bind(("id", agent_id.as_str().to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<AgentRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(!rows.is_empty())
}
