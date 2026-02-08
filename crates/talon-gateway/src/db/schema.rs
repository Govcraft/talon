//! Database schema initialisation for the Talon gateway.
//!
//! Provides functions to create the required tables and indexes
//! in both the system namespace and per-tenant namespaces.

use super::DbClient;
use crate::error::GatewayError;

/// Initialise the system namespace schema (tenant registry, channel bindings).
///
/// This should be called once at startup after the SurrealDB client
/// becomes available.
pub async fn init_system_schema(client: &DbClient) -> std::result::Result<(), GatewayError> {
    client
        .query(
            r"
        USE NS system DB admin;

        DEFINE TABLE IF NOT EXISTS tenant SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_tenant_slug ON tenant FIELDS slug UNIQUE;

        DEFINE TABLE IF NOT EXISTS channel_binding SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_channel_binding ON channel_binding FIELDS channel_id UNIQUE;
    ",
        )
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;
    Ok(())
}

/// Initialise a tenant namespace with the standard set of tables.
///
/// Called when a new tenant is created, or on startup for the default tenant.
pub async fn init_tenant_schema(
    client: &DbClient,
    namespace: &str,
) -> std::result::Result<(), GatewayError> {
    let query = format!(
        r"
        USE NS `{namespace}` DB main;

        DEFINE TABLE IF NOT EXISTS agent SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_agent_name ON agent FIELDS name;

        DEFINE TABLE IF NOT EXISTS session SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_session_key ON session FIELDS tenant_id, channel_id, sender_id UNIQUE;

        DEFINE TABLE IF NOT EXISTS conversation SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_conv_session ON conversation FIELDS session_id;

        DEFINE TABLE IF NOT EXISTS usage SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_usage_key ON usage FIELDS tenant_id, period, provider, model UNIQUE;

        DEFINE TABLE IF NOT EXISTS audit_event SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS idx_audit_seq ON audit_event FIELDS sequence;
    "
    );
    client
        .query(&query)
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;
    Ok(())
}
