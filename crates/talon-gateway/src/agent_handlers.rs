//! HTTP handlers for agent CRUD operations, scoped to a tenant.

use acton_service::prelude::*;

use crate::audit;
use crate::db;
use crate::error::GatewayError;
use talon_types::{AgentId, CreateAgentRequest, TenantId, UpdateAgentRequest};

/// Resolve a tenant ID to its namespace by looking up the tenant record.
#[tracing::instrument(skip(client))]
async fn resolve_tenant_namespace(
    client: &db::DbClient,
    tenant_id_str: &str,
) -> std::result::Result<(TenantId, String), GatewayError> {
    let tenant_id = TenantId::new(tenant_id_str);
    let tenant = db::tenant_repo::get_tenant(client, &tenant_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("tenant '{tenant_id_str}' not found")))?;
    let namespace = tenant.id.namespace();
    Ok((tenant.id, namespace))
}

/// POST /api/v1/tenants/{tenant_id}/agents -- create a new agent.
#[tracing::instrument(skip(state))]
pub async fn create_agent(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> std::result::Result<(StatusCode, Json<talon_types::Agent>), GatewayError> {
    let client = db::get_db(&state).await?;
    let (tid, namespace) = resolve_tenant_namespace(&client, &tenant_id).await?;

    let agent = db::agent_repo::create_agent(&client, &namespace, &tid, &req).await?;

    audit::log_agent_event(
        state.audit_logger(),
        "created",
        tid.as_str(),
        agent.id.as_str(),
        serde_json::json!({
            "name": agent.name,
            "provider": agent.provider,
            "model": agent.model,
            "trust_tier": agent.trust_tier.to_string(),
        }),
    )
    .await;

    Ok((StatusCode::CREATED, Json(agent)))
}

/// GET /api/v1/tenants/{tenant_id}/agents -- list agents for a tenant.
#[tracing::instrument(skip(state))]
pub async fn list_agents(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> std::result::Result<Json<Vec<talon_types::Agent>>, GatewayError> {
    let client = db::get_db(&state).await?;
    let (_tid, namespace) = resolve_tenant_namespace(&client, &tenant_id).await?;

    let agents = db::agent_repo::list_agents(&client, &namespace).await?;
    Ok(Json(agents))
}

/// PUT /api/v1/tenants/{tenant_id}/agents/{agent_id} -- update an agent.
#[tracing::instrument(skip(state))]
pub async fn update_agent(
    State(state): State<AppState>,
    Path((tenant_id, agent_id)): Path<(String, String)>,
    Json(req): Json<UpdateAgentRequest>,
) -> std::result::Result<Json<talon_types::Agent>, GatewayError> {
    let client = db::get_db(&state).await?;
    let (_tid, namespace) = resolve_tenant_namespace(&client, &tenant_id).await?;

    let aid = AgentId::new(&agent_id);
    let result = db::agent_repo::update_agent(&client, &namespace, &aid, &req)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("agent '{agent_id}' not found")))?;

    audit::log_agent_event(
        state.audit_logger(),
        "updated",
        &tenant_id,
        result.id.as_str(),
        serde_json::json!({"name": result.name}),
    )
    .await;

    Ok(Json(result))
}

/// DELETE /api/v1/tenants/{tenant_id}/agents/{agent_id} -- delete an agent.
#[tracing::instrument(skip(state))]
pub async fn delete_agent(
    State(state): State<AppState>,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> std::result::Result<StatusCode, GatewayError> {
    let client = db::get_db(&state).await?;
    let (_tid, namespace) = resolve_tenant_namespace(&client, &tenant_id).await?;

    let aid = AgentId::new(&agent_id);
    let deleted = db::agent_repo::delete_agent(&client, &namespace, &aid).await?;
    if deleted {
        audit::log_agent_event(
            state.audit_logger(),
            "deleted",
            &tenant_id,
            &agent_id,
            serde_json::json!({}),
        )
        .await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GatewayError::NotFound(format!(
            "agent '{agent_id}' not found"
        )))
    }
}
