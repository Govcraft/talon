//! Tenant repository -- CRUD operations against the system namespace.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::DbClient;
use crate::error::GatewayError;
use talon_types::{
    CreateTenantRequest, Plan, Tenant, TenantId, TenantSettings, TenantStatus, UpdateTenantRequest,
};

/// Internal row representation for SurrealDB serialisation.
#[derive(Debug, Serialize)]
struct TenantRecord {
    id: String,
    name: String,
    slug: String,
    status: String,
    plan: String,
    settings: TenantSettings,
    created_at: String,
    updated_at: String,
}

/// Internal row representation for SurrealDB deserialisation.
#[derive(Debug, Deserialize)]
struct TenantRow {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    name: String,
    slug: String,
    status: String,
    plan: String,
    settings: TenantSettings,
    created_at: String,
    updated_at: String,
}

impl TenantRow {
    fn into_tenant(self) -> Tenant {
        let raw_id = self.id.id.to_raw();
        Tenant {
            id: TenantId::new(raw_id),
            name: self.name,
            slug: self.slug,
            status: match self.status.as_str() {
                "suspended" => TenantStatus::Suspended,
                "deleted" => TenantStatus::Deleted,
                _ => TenantStatus::Active,
            },
            plan: match self.plan.as_str() {
                "pro" => Plan::Pro,
                "enterprise" => Plan::Enterprise,
                _ => Plan::Free,
            },
            settings: self.settings,
            created_at: self.created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: self.updated_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

fn plan_to_str(plan: &Plan) -> &'static str {
    match plan {
        Plan::Free => "free",
        Plan::Pro => "pro",
        Plan::Enterprise => "enterprise",
    }
}

fn status_to_str(status: &TenantStatus) -> &'static str {
    match status {
        TenantStatus::Active => "active",
        TenantStatus::Suspended => "suspended",
        TenantStatus::Deleted => "deleted",
    }
}

/// Create a new tenant in the system namespace.
#[tracing::instrument(skip(client))]
pub async fn create_tenant(
    client: &DbClient,
    req: &CreateTenantRequest,
) -> std::result::Result<Tenant, GatewayError> {
    let tenant_id = TenantId::generate();
    let now = Utc::now();
    let record = TenantRecord {
        id: tenant_id.as_str().to_string(),
        name: req.name.clone(),
        slug: req.slug.clone(),
        status: "active".to_string(),
        plan: plan_to_str(&req.plan).to_string(),
        settings: req.settings.clone().unwrap_or_default(),
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    };

    let data = serde_json::to_value(&record).map_err(|e| GatewayError::Internal(e.to_string()))?;

    let mut result = client
        .query("USE NS system DB admin; CREATE type::thing('tenant', $id) CONTENT $data")
        .bind(("id", tenant_id.as_str().to_string()))
        .bind(("data", data))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    // Statement index 1 is the CREATE (index 0 is USE NS)
    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    rows.into_iter()
        .next()
        .map(TenantRow::into_tenant)
        .ok_or_else(|| GatewayError::Internal("failed to create tenant record".into()))
}

/// List all tenants.
#[tracing::instrument(skip(client))]
pub async fn list_tenants(client: &DbClient) -> std::result::Result<Vec<Tenant>, GatewayError> {
    let mut result = client
        .query("USE NS system DB admin; SELECT * FROM tenant ORDER BY created_at DESC")
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(TenantRow::into_tenant).collect())
}

/// Get a tenant by ID.
#[tracing::instrument(skip(client))]
pub async fn get_tenant(
    client: &DbClient,
    id: &TenantId,
) -> std::result::Result<Option<Tenant>, GatewayError> {
    let mut result = client
        .query("USE NS system DB admin; SELECT * FROM type::thing('tenant', $id)")
        .bind(("id", id.as_str().to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(TenantRow::into_tenant))
}

/// Get a tenant by slug.
#[tracing::instrument(skip(client))]
pub async fn get_tenant_by_slug(
    client: &DbClient,
    slug: &str,
) -> std::result::Result<Option<Tenant>, GatewayError> {
    let mut result = client
        .query("USE NS system DB admin; SELECT * FROM tenant WHERE slug = $slug LIMIT 1")
        .bind(("slug", slug.to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(TenantRow::into_tenant))
}

/// Update a tenant.
#[tracing::instrument(skip(client))]
pub async fn update_tenant(
    client: &DbClient,
    id: &TenantId,
    req: &UpdateTenantRequest,
) -> std::result::Result<Option<Tenant>, GatewayError> {
    // Build a MERGE object from only the provided fields.
    let mut merge = serde_json::Map::new();
    if let Some(ref name) = req.name {
        merge.insert("name".into(), serde_json::Value::String(name.clone()));
    }
    if let Some(ref status) = req.status {
        merge.insert(
            "status".into(),
            serde_json::Value::String(status_to_str(status).into()),
        );
    }
    if let Some(ref plan) = req.plan {
        merge.insert(
            "plan".into(),
            serde_json::Value::String(plan_to_str(plan).into()),
        );
    }
    if let Some(ref settings) = req.settings {
        merge.insert(
            "settings".into(),
            serde_json::to_value(settings).map_err(|e| GatewayError::Internal(e.to_string()))?,
        );
    }
    merge.insert(
        "updated_at".into(),
        serde_json::Value::String(Utc::now().to_rfc3339()),
    );

    let merge_value = serde_json::Value::Object(merge);

    let mut result = client
        .query("USE NS system DB admin; UPDATE type::thing('tenant', $id) MERGE $data")
        .bind(("id", id.as_str().to_string()))
        .bind(("data", merge_value))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(TenantRow::into_tenant))
}

/// Delete a tenant by ID. Returns true if a record was deleted.
#[tracing::instrument(skip(client))]
pub async fn delete_tenant(
    client: &DbClient,
    id: &TenantId,
) -> std::result::Result<bool, GatewayError> {
    let mut result = client
        .query("USE NS system DB admin; DELETE type::thing('tenant', $id) RETURN BEFORE")
        .bind(("id", id.as_str().to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<TenantRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(!rows.is_empty())
}
