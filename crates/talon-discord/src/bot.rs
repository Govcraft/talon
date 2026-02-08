use std::sync::Arc;

use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use talon_channel_sdk::GatewayClient;
use tokio::sync::Mutex;

/// Discord event handler that forwards messages to the Talon gateway.
pub struct Handler {
    gateway: Arc<Mutex<GatewayClient>>,
}

impl Handler {
    /// Create a new handler backed by the given gateway client.
    pub fn new(gateway: Arc<Mutex<GatewayClient>>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots to prevent feedback loops.
        if msg.author.bot {
            return;
        }

        let sender_id = format!("dc:{}", msg.author.id);

        tracing::debug!(sender_id = %sender_id, "Received message from Discord");

        let response = {
            let mut gw = self.gateway.lock().await;
            match gw.send_message(&sender_id, &msg.content, None).await {
                Ok(resp) => resp.text,
                Err(e) => {
                    tracing::error!("Gateway error: {e}");
                    "Sorry, I encountered an error processing your message.".to_string()
                }
            }
        };

        if let Err(e) = msg.channel_id.say(&ctx.http, &response).await {
            tracing::error!("Failed to send Discord message: {e}");
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!("Discord bot connected as {}", ready.user.name);
    }
}
