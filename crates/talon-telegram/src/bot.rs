use std::sync::Arc;

use talon_channel_sdk::GatewayClient;
use teloxide::prelude::*;
use tokio::sync::Mutex;

/// Run the Telegram bot event loop.
pub async fn run(bot: Bot, gateway: Arc<Mutex<GatewayClient>>) {
    let handler = Update::filter_message().endpoint(handle_message);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![gateway])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

/// Handle an incoming Telegram message.
async fn handle_message(
    bot: Bot,
    msg: Message,
    gateway: Arc<Mutex<GatewayClient>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let text = match msg.text() {
        Some(text) => text,
        None => return Ok(()), // Ignore non-text messages
    };

    let sender_id = format!("tg:{}", msg.chat.id);

    tracing::debug!(sender_id = %sender_id, "Received message from Telegram");

    let mut gateway = gateway.lock().await;
    match gateway.send_message(&sender_id, text, None).await {
        Ok(response) => {
            bot.send_message(msg.chat.id, &response.text).await?;
        }
        Err(e) => {
            tracing::error!("Gateway error: {}", e);
            bot.send_message(
                msg.chat.id,
                "Sorry, I encountered an error processing your message.",
            )
            .await?;
        }
    }

    Ok(())
}
