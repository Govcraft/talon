//! Tenant CRUD page handlers.

use acton_service::prelude::*;
use askama::Template;
use std::sync::Arc;

use crate::api_client::GatewayApiClient;
use talon_types::{CreateTenantRequest, Tenant};

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// Template context for the tenant list page.
#[derive(Template)]
#[template(path = "tenants/list.html")]
struct TenantListTemplate {
    tenants: Vec<Tenant>,
}

/// Template context for the tenant detail page.
#[derive(Template)]
#[template(path = "tenants/detail.html")]
struct TenantDetailTemplate {
    tenant: Tenant,
    agents: Vec<talon_types::Agent>,
}

/// Template context for the "create tenant" form.
#[derive(Template)]
#[template(path = "tenants/create.html")]
struct TenantCreateTemplate;

// ---------------------------------------------------------------------------
// Form types
// ---------------------------------------------------------------------------

/// HTML form data for creating a tenant.
#[derive(serde::Deserialize)]
pub struct CreateTenantForm {
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /tenants -- list all tenants.
#[tracing::instrument(skip(client))]
pub async fn list(
    Extension(client): Extension<Arc<GatewayApiClient>>,
) -> std::result::Result<Html<String>, StatusCode> {
    let tenants = client.list_tenants().await.unwrap_or_default();

    let tmpl = TenantListTemplate { tenants };
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

/// GET /tenants/new -- render the create-tenant form.
#[tracing::instrument]
pub async fn new_form() -> std::result::Result<Html<String>, StatusCode> {
    let tmpl = TenantCreateTemplate;
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

/// GET /tenants/{id} -- show tenant detail with its agents.
#[tracing::instrument(skip(client))]
pub async fn detail(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path(id): Path<String>,
) -> std::result::Result<Html<String>, StatusCode> {
    let tenant = client
        .get_tenant(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let agents = client.list_agents(&id).await.unwrap_or_default();

    let tmpl = TenantDetailTemplate { tenant, agents };
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

/// POST /tenants -- create a new tenant from form data, then redirect.
#[tracing::instrument(skip(client, form))]
pub async fn create(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Form(form): Form<CreateTenantForm>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let plan = match form.plan.as_deref() {
        Some("pro") => talon_types::Plan::Pro,
        Some("enterprise") => talon_types::Plan::Enterprise,
        _ => talon_types::Plan::Free,
    };

    let req = CreateTenantRequest {
        name: form.name,
        slug: form.slug,
        plan,
        settings: None,
    };

    client
        .create_tenant(&req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok((StatusCode::SEE_OTHER, [("location", "/v1/tenants")]))
}

/// POST /tenants/{id}/delete -- delete a tenant, return empty body for HTMX swap.
#[tracing::instrument(skip(client))]
pub async fn delete(
    Extension(client): Extension<Arc<GatewayApiClient>>,
    Path(id): Path<String>,
) -> std::result::Result<Html<String>, StatusCode> {
    client
        .delete_tenant(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return empty string so HTMX `hx-swap="outerHTML"` removes the row.
    Ok(Html(String::new()))
}
