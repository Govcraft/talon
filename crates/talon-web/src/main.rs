//! Web/API channel service for the Talon AI gateway.
//!
//! Exposes HTTP (REST + SSE) and WebSocket endpoints for web clients,
//! forwarding messages to the gateway over gRPC via the channel SDK.

use acton_service::prelude::*;
use talon_channel_sdk::GatewayClient;

mod handlers;
mod routes;
mod ws;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let gateway_url =
        std::env::var("TALON_GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let tenant_id = std::env::var("TALON_TENANT_ID").unwrap_or_else(|_| "default".into());

    tracing::info!("Connecting to gateway at {}", gateway_url);

    let gateway_client = GatewayClient::connect(&gateway_url, "web", &tenant_id).await?;

    let routes = routes::build_routes(gateway_client);

    let mut config = Config::<()>::load().unwrap_or_default();
    let port: u16 = std::env::var("TALON_WEB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);
    config.service.port = port;

    ServiceBuilder::new()
        .with_config(config)
        .with_routes(routes)
        .build()
        .serve()
        .await?;

    Ok(())
}
