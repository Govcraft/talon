use async_trait::async_trait;
use talon_types::{InboundMessage, OutboundMessage};

use crate::client::GatewayClient;
use crate::error::ChannelError;

/// Trait for implementing a channel service.
///
/// Each channel binary implements this trait and calls `run_channel_service()`
/// which handles:
/// 1. Loading config
/// 2. Connecting gRPC client to gateway
/// 3. Building health/ready HTTP routes
/// 4. Spawning the channel event loop
/// 5. Running the HTTP health service
#[async_trait]
pub trait ChannelHandler: Send + Sync + 'static {
    /// The channel identifier (e.g., "telegram", "discord", "web").
    fn channel_id(&self) -> &str;

    /// Platform-specific raw inbound message type.
    type RawInbound: Send;

    /// Convert a platform-specific message to the gateway's InboundMessage format.
    async fn inbound(&self, raw: Self::RawInbound) -> Result<InboundMessage, ChannelError>;

    /// Deliver a gateway response back to the platform.
    async fn outbound(&self, response: OutboundMessage) -> Result<(), ChannelError>;

    /// Run the channel's event loop (e.g., polling, webhooks, WebSocket listener).
    async fn run(&self, gateway_client: GatewayClient) -> Result<(), ChannelError>;
}
