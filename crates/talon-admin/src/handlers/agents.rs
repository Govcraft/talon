//! Agent management page handlers.

use acton_service::prelude::*;
use askama::Template;
use std::sync::Arc;

use crate::api_client::GatewayApiClient;
use talon_types::{Agent, CreateAgentRequest, TrustTier};

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// Template context for the agent list page.
#[derive(Template)]
#[template(path = "agents/list.html")]
struct AgentListTemplate {
    tenant_id: String,
    tenant_name: String,
    agents: Vec<Agent>,
}

/// Template context for the "create agent" form.
#[derive(Template)]
#[template(path = "agents/create.html")]
struct AgentCreateTemplate {
    tenant_id: String,
    tenant_name: String,
}

// ---------------------------------------------------------------------------
// Form types
// ---------------------------------------------------------------------------

/// HTML form data for creating an agent.
#[derive(serde::Deserialize)]
pub struct CreateAgentForm {
    pub name: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub trust_tier: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /tenants/{id}/agents -- list agents for a tenant.
#[tracing::instrument(skip(client))]
pub async fn list(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path(id): Path<String>,
) -> std::result::Result<Html<String>, StatusCode> {
    let tenant = client
        .get_tenant(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let agents = client.list_agents(&id).await.unwrap_or_default();

    let tmpl = AgentListTemplate {
        tenant_id: id,
        tenant_name: tenant.name,
        agents,
    };
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

/// GET /tenants/{id}/agents/new -- render the create-agent form.
#[tracing::instrument(skip(client))]
pub async fn new_form(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path(id): Path<String>,
) -> std::result::Result<Html<String>, StatusCode> {
    let tenant = client
        .get_tenant(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let tmpl = AgentCreateTemplate {
        tenant_id: id,
        tenant_name: tenant.name,
    };
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

/// POST /tenants/{id}/agents -- create a new agent from form data.
#[tracing::instrument(skip(client, form))]
pub async fn create(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path(id): Path<String>,
    Form(form): Form<CreateAgentForm>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let trust_tier = form
        .trust_tier
        .as_deref()
        .and_then(|s| s.parse::<TrustTier>().ok())
        .unwrap_or_default();

    let req = CreateAgentRequest {
        name: form.name,
        system_prompt: form.system_prompt.filter(|s| !s.is_empty()),
        provider: form.provider.unwrap_or_else(|| "ollama".to_string()),
        model: form.model.unwrap_or_else(|| "llama3.2".to_string()),
        tools: Vec::new(),
        skills: Vec::new(),
        trust_tier,
        is_default: false,
    };

    client
        .create_agent(&id, &req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let location = format!("/v1/tenants/{id}/agents");
    Ok((StatusCode::SEE_OTHER, [("location", location)]))
}

/// POST /tenants/{id}/agents/{agent_id}/delete -- delete an agent.
#[tracing::instrument(skip(client))]
pub async fn delete(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path((id, agent_id)): Path<(String, String)>,
) -> std::result::Result<Html<String>, StatusCode> {
    client
        .delete_agent(&id, &agent_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return empty string so HTMX `hx-swap="outerHTML"` removes the row.
    Ok(Html(String::new()))
}
