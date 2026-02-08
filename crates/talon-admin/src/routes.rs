//! Route definitions for the admin dashboard.

use acton_service::prelude::*;
use std::sync::Arc;

use crate::api_client::GatewayApiClient;
use crate::handlers;

/// Build the versioned route tree for the admin dashboard.
///
/// Routes are served under `/v1/` via `VersionedApiBuilder`. The
/// `GatewayApiClient` is shared across all handlers through an
/// `Extension` layer.
pub fn build_routes() -> VersionedRoutes {
    let gateway_url =
        std::env::var("TALON_GATEWAY_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let client = Arc::new(GatewayApiClient::new(&gateway_url));

    VersionedApiBuilder::new()
        .with_base_path("")
        .add_version(ApiVersion::V1, move |router| {
            router
                // Dashboard
                .route("/", get(handlers::dashboard::index))
                // Tenants
                .route(
                    "/tenants",
                    get(handlers::tenants::list).post(handlers::tenants::create),
                )
                .route("/tenants/new", get(handlers::tenants::new_form))
                .route("/tenants/{id}", get(handlers::tenants::detail))
                .route("/tenants/{id}/delete", post(handlers::tenants::delete))
                // Agents (scoped to tenant)
                .route(
                    "/tenants/{id}/agents",
                    get(handlers::agents::list).post(handlers::agents::create),
                )
                .route("/tenants/{id}/agents/new", get(handlers::agents::new_form))
                .route(
                    "/tenants/{id}/agents/{agent_id}/delete",
                    post(handlers::agents::delete),
                )
                // Sessions
                .route("/sessions", get(handlers::sessions::list))
                .layer(Extension(client))
        })
        .build_routes()
}
