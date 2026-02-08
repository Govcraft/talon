//! HTTP handlers for tenant CRUD operations.

use acton_service::prelude::*;

use crate::audit;
use crate::db;
use crate::error::GatewayError;
use talon_types::{CreateTenantRequest, TenantId, UpdateTenantRequest};

/// Validate that a slug contains only lowercase alphanumeric characters and hyphens.
#[tracing::instrument]
fn validate_slug(slug: &str) -> std::result::Result<(), GatewayError> {
    if slug.is_empty() {
        return Err(GatewayError::BadRequest("slug must not be empty".into()));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(GatewayError::BadRequest(
            "slug must contain only lowercase letters, digits, and hyphens".into(),
        ));
    }
    Ok(())
}

/// POST /api/v1/tenants -- create a new tenant.
#[tracing::instrument(skip(state))]
pub async fn create_tenant(
    State(state): State<AppState>,
    Json(req): Json<CreateTenantRequest>,
) -> std::result::Result<(StatusCode, Json<talon_types::Tenant>), GatewayError> {
    validate_slug(&req.slug)?;

    let client = db::get_db(&state).await?;

    // Check for slug uniqueness.
    if let Some(_existing) = db::tenant_repo::get_tenant_by_slug(&client, &req.slug).await? {
        return Err(GatewayError::Conflict(format!(
            "tenant with slug '{}' already exists",
            req.slug
        )));
    }

    let tenant = db::tenant_repo::create_tenant(&client, &req).await?;

    // Initialise the tenant's namespace with standard tables.
    let namespace = tenant.id.namespace();
    db::schema::init_tenant_schema(&client, &namespace).await?;

    audit::log_tenant_event(
        state.audit_logger(),
        "created",
        tenant.id.as_str(),
        serde_json::json!({
            "name": tenant.name,
            "slug": tenant.slug,
            "plan": format!("{:?}", tenant.plan),
        }),
    )
    .await;

    Ok((StatusCode::CREATED, Json(tenant)))
}

/// GET /api/v1/tenants -- list all tenants.
#[tracing::instrument(skip(state))]
pub async fn list_tenants(
    State(state): State<AppState>,
) -> std::result::Result<Json<Vec<talon_types::Tenant>>, GatewayError> {
    let client = db::get_db(&state).await?;
    let tenants = db::tenant_repo::list_tenants(&client).await?;
    Ok(Json(tenants))
}

/// GET /api/v1/tenants/{id} -- get a tenant by ID.
#[tracing::instrument(skip(state))]
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<talon_types::Tenant>, GatewayError> {
    let client = db::get_db(&state).await?;
    let tenant_id = TenantId::new(&id);
    db::tenant_repo::get_tenant(&client, &tenant_id)
        .await?
        .map(Json)
        .ok_or_else(|| GatewayError::NotFound(format!("tenant '{id}' not found")))
}

/// PUT /api/v1/tenants/{id} -- update a tenant.
#[tracing::instrument(skip(state))]
pub async fn update_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> std::result::Result<Json<talon_types::Tenant>, GatewayError> {
    let client = db::get_db(&state).await?;
    let tenant_id = TenantId::new(&id);
    let result = db::tenant_repo::update_tenant(&client, &tenant_id, &req)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("tenant '{id}' not found")))?;

    audit::log_tenant_event(
        state.audit_logger(),
        "updated",
        result.id.as_str(),
        serde_json::json!({"name": result.name}),
    )
    .await;

    Ok(Json(result))
}

/// DELETE /api/v1/tenants/{id} -- delete a tenant.
#[tracing::instrument(skip(state))]
pub async fn delete_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<StatusCode, GatewayError> {
    let client = db::get_db(&state).await?;
    let tenant_id = TenantId::new(&id);
    let deleted = db::tenant_repo::delete_tenant(&client, &tenant_id).await?;
    if deleted {
        audit::log_tenant_event(
            state.audit_logger(),
            "deleted",
            tenant_id.as_str(),
            serde_json::json!({}),
        )
        .await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GatewayError::NotFound(format!("tenant '{id}' not found")))
    }
}
