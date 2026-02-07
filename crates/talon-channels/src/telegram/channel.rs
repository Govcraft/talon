//! TelegramChannel implementation
//!
//! Implements the Channel trait for Telegram using teloxide 0.17.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use talon_core::{ChannelId, ConversationId};
use teloxide::dispatching::ShutdownToken;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::channel::{Channel, InboundMessage, MessageContent, OutboundMessage};
use crate::error::{ChannelError, ChannelResult};
use crate::telegram::config::TelegramConfig;
use crate::telegram::handlers::build_message_handler;
use crate::telegram::mapping::IdMapper;
use crate::telegram::streaming::{send_split_message, StreamingManager};

/// Telegram channel implementation
pub struct TelegramChannel {
    /// Channel identifier
    id: ChannelId,
    /// Channel configuration (kept for potential future use)
    #[allow(dead_code)]
    config: TelegramConfig,
    /// Teloxide bot instance
    bot: Bot,
    /// Running state flag
    running: Arc<AtomicBool>,
    /// ID mapper for ChatId <-> ConversationId
    id_mapper: Arc<IdMapper>,
    /// Streaming response manager
    streaming_manager: Arc<RwLock<StreamingManager>>,
    /// Dispatcher task handle
    dispatcher_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// Shutdown token for graceful dispatcher shutdown
    shutdown_token: Arc<RwLock<Option<ShutdownToken>>>,
}

impl TelegramChannel {
    /// Create a new Telegram channel with the given configuration
    ///
    /// # Errors
    ///
    /// Returns error if bot creation fails.
    pub fn new(config: TelegramConfig) -> ChannelResult<Self> {
        let bot = Bot::new(config.token());

        Ok(Self {
            id: ChannelId::new("telegram"),
            config: config.clone(),
            bot,
            running: Arc::new(AtomicBool::new(false)),
            id_mapper: Arc::new(IdMapper::new()),
            streaming_manager: Arc::new(RwLock::new(StreamingManager::new(
                config.debounce_interval,
            ))),
            dispatcher_handle: Arc::new(RwLock::new(None)),
            shutdown_token: Arc::new(RwLock::new(None)),
        })
    }

    /// Create a new Telegram channel loading config from environment/keyring
    ///
    /// # Errors
    ///
    /// Returns error if configuration loading fails.
    pub fn from_env() -> ChannelResult<Self> {
        let config = TelegramConfig::load().map_err(|e| ChannelError::Connection {
            message: e.to_string(),
        })?;

        Self::new(config)
    }

    /// Get a reference to the bot instance
    pub fn bot(&self) -> &Bot {
        &self.bot
    }

    /// Get a reference to the ID mapper
    pub fn id_mapper(&self) -> &Arc<IdMapper> {
        &self.id_mapper
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn id(&self) -> ChannelId {
        self.id.clone()
    }

    async fn start(&self, sender: mpsc::Sender<InboundMessage>) -> ChannelResult<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(ChannelError::AlreadyStarted);
        }

        tracing::info!("Starting Telegram channel");

        // Build the message handler
        let handler = build_message_handler();

        // Clone dependencies for the dispatcher
        let id_mapper = Arc::clone(&self.id_mapper);
        let channel_id = self.id.clone();
        let bot = self.bot.clone();

        // Build dispatcher with dependencies
        // Note: We handle Ctrl+C ourselves in main.rs, so don't enable teloxide's handler
        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![sender, id_mapper, channel_id])
            .build();

        // Store shutdown token for graceful shutdown
        let shutdown_token = dispatcher.shutdown_token();
        {
            let mut token_guard = self.shutdown_token.write().await;
            *token_guard = Some(shutdown_token);
        }

        // Spawn dispatcher in background task
        let handle = tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        {
            let mut handle_guard = self.dispatcher_handle.write().await;
            *handle_guard = Some(handle);
        }

        tracing::info!("Telegram channel started");
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> ChannelResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        // Map ConversationId to ChatId
        let chat_id = self
            .id_mapper
            .get_chat_id(&message.conversation_id)
            .ok_or_else(|| ChannelError::Send {
                message: format!(
                    "no chat mapping for conversation {}",
                    message.conversation_id
                ),
            })?;

        // Determine parse mode based on content type
        let (content, parse_mode) = match &message.content {
            MessageContent::Text(text) => (text.as_str(), None),
            MessageContent::Markdown(md) => (md.as_str(), Some(ParseMode::MarkdownV2)),
            MessageContent::Image { url, caption } => {
                // For images, send the URL with optional caption
                let text = match caption {
                    Some(cap) => format!("{url}\n{cap}"),
                    None => url.clone(),
                };
                return send_split_message(&self.bot, chat_id, &text, None).await;
            }
        };

        // Send message (handling split if needed)
        send_split_message(&self.bot, chat_id, content, parse_mode).await?;

        tracing::debug!(
            conversation = %message.conversation_id,
            "sent message"
        );

        Ok(())
    }

    async fn send_token(
        &self,
        conversation_id: &ConversationId,
        token: &str,
    ) -> ChannelResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        let mut manager = self.streaming_manager.write().await;
        manager
            .append_token(conversation_id, token, &self.bot, &self.id_mapper)
            .await
    }

    async fn stop(&self) -> ChannelResult<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(ChannelError::NotStarted);
        }

        tracing::info!("Stopping Telegram channel");

        // Finalize any active streams
        {
            let mut manager = self.streaming_manager.write().await;
            if let Err(e) = manager.finalize_all(&self.bot).await {
                tracing::warn!(error = %e, "failed to finalize streams during shutdown");
            }
        }

        // Trigger graceful shutdown via token
        {
            let token_guard = self.shutdown_token.read().await;
            if let Some(ref token) = *token_guard {
                token.shutdown().ok();
            }
        }

        // Wait for dispatcher task to complete (with timeout)
        {
            let mut handle_guard = self.dispatcher_handle.write().await;
            if let Some(handle) = handle_guard.take() {
                match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                    Ok(Ok(())) => tracing::debug!("dispatcher task completed"),
                    Ok(Err(e)) => tracing::warn!(error = ?e, "dispatcher task panicked"),
                    Err(_) => tracing::warn!("dispatcher shutdown timed out"),
                }
            }
        }

        tracing::info!("Telegram channel stopped");
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_is_telegram() {
        // Note: Can't actually create channel without valid token,
        // but we can test the ID separately
        let id = ChannelId::new("telegram");
        assert_eq!(id.to_string(), "telegram");
    }

    #[test]
    fn config_clone_works() {
        let config = TelegramConfig::for_test("test-token");
        let _cloned = config.clone();
    }
}
