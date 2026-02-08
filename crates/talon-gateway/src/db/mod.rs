//! SurrealDB data access layer for the Talon gateway.
//!
//! All database operations go through this module. Each sub-module
//! provides repository functions for a specific domain entity.

pub mod agent_repo;
pub mod audit_repo;
pub mod schema;
pub mod session_repo;
pub mod tenant_repo;
pub mod usage_repo;

use std::sync::Arc;

use acton_service::surrealdb_backend::SurrealClient;

/// Convenience type alias for the shared SurrealDB client handle.
pub type DbClient = Arc<SurrealClient>;

/// Extract the SurrealDB client from `AppState`, returning a gateway error
/// when the database connection is not yet available.
pub async fn get_db(
    state: &acton_service::prelude::AppState,
) -> std::result::Result<DbClient, crate::error::GatewayError> {
    state
        .surrealdb()
        .await
        .ok_or(crate::error::GatewayError::DatabaseUnavailable)
}
