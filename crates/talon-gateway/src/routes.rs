use acton_service::prelude::*;

use crate::rate_limit::ChatRateLimiter;
use crate::{agent_handlers, handlers, tenant_handlers};

/// Build the versioned HTTP routes for the gateway API.
///
/// Chat endpoints (`/chat`, `/chat/stream`) are rate-limited to 30 requests
/// per minute with a burst allowance of 5. All other endpoints (sessions,
/// tenants, agents) are not rate-limited at the route level.
pub fn build_routes() -> VersionedRoutes {
    // 30 requests per minute, burst of 5 additional requests.
    let chat_limiter = ChatRateLimiter::new(30, 5);

    VersionedApiBuilder::new()
        .with_base_path("/api")
        .add_version(ApiVersion::V1, |router| {
            // Chat routes with rate limiting applied.
            let chat_routes = Router::new()
                .route("/chat", post(handlers::chat))
                .route("/chat/stream", post(handlers::chat_stream))
                .layer(axum::middleware::from_fn_with_state(
                    chat_limiter,
                    ChatRateLimiter::middleware,
                ));

            router
                .merge(chat_routes)
                // Sessions
                .route("/sessions", get(handlers::list_sessions))
                .route("/sessions/{id}", get(handlers::get_session))
                // Tenants
                .route(
                    "/tenants",
                    post(tenant_handlers::create_tenant).get(tenant_handlers::list_tenants),
                )
                .route(
                    "/tenants/{id}",
                    get(tenant_handlers::get_tenant)
                        .put(tenant_handlers::update_tenant)
                        .delete(tenant_handlers::delete_tenant),
                )
                // Agents (per-tenant)
                .route(
                    "/tenants/{tenant_id}/agents",
                    post(agent_handlers::create_agent).get(agent_handlers::list_agents),
                )
                .route(
                    "/tenants/{tenant_id}/agents/{agent_id}",
                    put(agent_handlers::update_agent).delete(agent_handlers::delete_agent),
                )
        })
        .build_routes()
}
