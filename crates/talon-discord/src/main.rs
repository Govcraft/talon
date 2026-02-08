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
        std::env::var("TALON_GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let tenant_id = std::env::var("TALON_TENANT_ID").unwrap_or_else(|_| "default".into());
    let discord_token = std::env::var("DISCORD_TOKEN")
        .map_err(|_| anyhow::anyhow!("DISCORD_TOKEN environment variable is required"))?;

    tracing::info!("Connecting to gateway at {}", gateway_url);

    let mut gateway_client = GatewayClient::connect(&gateway_url, "discord", &tenant_id).await?;

    // Register with the gateway so it knows about this channel.
    gateway_client.register("http://localhost:0").await?;

    let gateway = Arc::new(Mutex::new(gateway_client));

    // Discord gateway intents required for reading message content.
    let intents = serenity::model::gateway::GatewayIntents::GUILD_MESSAGES
        | serenity::model::gateway::GatewayIntents::DIRECT_MESSAGES
        | serenity::model::gateway::GatewayIntents::MESSAGE_CONTENT;

    let handler = bot::Handler::new(gateway);

    let mut client = serenity::Client::builder(&discord_token, intents)
        .event_handler(handler)
        .await?;

    tracing::info!("Starting Discord bot");
    client.start().await?;

    Ok(())
}
