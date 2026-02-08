//! Route definitions for the web channel service.

use acton_service::prelude::*;
use std::sync::Arc;
use talon_channel_sdk::GatewayClient;
use tokio::sync::Mutex;

use crate::handlers;
use crate::ws;

/// Shared state injected into handlers via `Extension`.
#[derive(Clone)]
pub struct WebState {
    pub gateway: Arc<Mutex<GatewayClient>>,
}

/// Build the versioned route tree for the web channel service.
pub fn build_routes(gateway_client: GatewayClient) -> VersionedRoutes {
    let web_state = WebState {
        gateway: Arc::new(Mutex::new(gateway_client)),
    };

    VersionedApiBuilder::new()
        .with_base_path("/api")
        .add_version(ApiVersion::V1, move |router| {
            router
                .route("/chat", post(handlers::chat))
                .route("/chat/stream", post(handlers::chat_stream))
                .route("/ws/chat", get(ws::ws_handler))
                .layer(Extension(web_state.clone()))
        })
        .build_routes()
}
