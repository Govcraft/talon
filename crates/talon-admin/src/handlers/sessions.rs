//! Session browser handler.

use acton_service::prelude::*;
use askama::Template;
use std::sync::Arc;

use crate::api_client::GatewayApiClient;
use talon_types::Session;

/// Template context for the session list page.
#[derive(Template)]
#[template(path = "sessions/list.html")]
struct SessionListTemplate {
    sessions: Vec<Session>,
}

/// GET /sessions -- list all sessions.
#[tracing::instrument(skip(client))]
pub async fn list(
    Extension(client): Extension<Arc<GatewayApiClient>>,
) -> std::result::Result<Html<String>, StatusCode> {
    let sessions = client.list_sessions().await.unwrap_or_default();

    let tmpl = SessionListTemplate { sessions };
    Ok(Html(
        tmpl.render()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}
