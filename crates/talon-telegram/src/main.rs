use std::sync::Arc;

use talon_channel_sdk::GatewayClient;
use teloxide::prelude::*;
use tokio::sync::Mutex;

mod bot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Config from env
    let gateway_url =
        std::env::var("TALON_GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let tenant_id = std::env::var("TALON_TENANT_ID").unwrap_or_else(|_| "default".to_string());
    let bot_token = std::env::var("TELOXIDE_TOKEN")
        .map_err(|_| anyhow::anyhow!("TELOXIDE_TOKEN environment variable is required"))?;

    tracing::info!("Connecting to gateway at {}", gateway_url);

    let gateway_client = GatewayClient::connect(&gateway_url, "telegram", &tenant_id).await?;
    let gateway = Arc::new(Mutex::new(gateway_client));

    // Create bot
    let bot = Bot::new(&bot_token);

    tracing::info!("Starting Telegram bot");

    // Run the bot
    bot::run(bot, gateway).await;

    Ok(())
}
