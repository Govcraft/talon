//! Dashboard overview handler.

use acton_service::prelude::*;
use askama::Template;
use std::sync::Arc;

use crate::api_client::GatewayApiClient;

/// Template context for the dashboard overview page.
#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    tenant_count: usize,
    session_count: usize,
}

/// GET / -- render the dashboard overview with summary counts.
#[tracing::instrument(skip(client))]
pub async fn index(
    Extension(client): Extension<Arc<GatewayApiClient>>,
) -> std::result::Result<Html<String>, StatusCode> {
    let tenants = client.list_tenants().await.unwrap_or_default();
    let sessions = client.list_sessions().await.unwrap_or_default();

    let tmpl = DashboardTemplate {
        tenant_count: tenants.len(),
        session_count: sessions.len(),
    };

    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}
