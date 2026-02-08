use std::sync::Arc;

use talon_channel_sdk::GatewayClient;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

mod bot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let gateway_url =
        std::env::var("TALON_GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let tenant_id = std::env::var("TALON_TENANT_ID").unwrap_or_else(|_| "default".to_string());
    let bot_token = std::env::var("SLACK_BOT_TOKEN")
        .map_err(|_| anyhow::anyhow!("SLACK_BOT_TOKEN environment variable is required"))?;
    let app_token = std::env::var("SLACK_APP_TOKEN")
        .map_err(|_| anyhow::anyhow!("SLACK_APP_TOKEN environment variable is required"))?;

    tracing::info!("Connecting to gateway at {}", gateway_url);

    let gateway_client = GatewayClient::connect(&gateway_url, "slack", &tenant_id).await?;
    let gateway = Arc::new(Mutex::new(gateway_client));

    tracing::info!("Starting Slack bot in Socket Mode");

    bot::run(gateway, bot_token, app_token).await
}
