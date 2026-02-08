use crate::error::ChannelError;
use crate::proto::common;
use crate::proto::gateway::gateway_service_client::GatewayServiceClient;
use crate::proto::gateway::{
    GetSessionRequest, GetSessionResponse, HeartbeatRequest, HeartbeatResponse,
    RegisterChannelRequest, RegisterChannelResponse, SendMessageRequest, SendMessageResponse,
    StreamChunk,
};
use tonic::transport::Channel;

/// Client for communicating with the Talon gateway over gRPC.
#[derive(Clone)]
pub struct GatewayClient {
    inner: GatewayServiceClient<Channel>,
    channel_id: String,
    tenant_id: String,
}

impl GatewayClient {
    /// Connect to the gateway at the given URL.
    #[tracing::instrument]
    pub async fn connect(
        gateway_url: &str,
        channel_id: &str,
        tenant_id: &str,
    ) -> Result<Self, ChannelError> {
        let channel = Channel::from_shared(gateway_url.to_string())
            .map_err(|e| ChannelError::ConnectionFailed(e.to_string()))?
            .connect()
            .await?;

        Ok(Self {
            inner: GatewayServiceClient::new(channel),
            channel_id: channel_id.to_string(),
            tenant_id: tenant_id.to_string(),
        })
    }

    /// Register this channel with the gateway.
    #[tracing::instrument(skip(self))]
    pub async fn register(
        &mut self,
        service_url: &str,
    ) -> Result<RegisterChannelResponse, ChannelError> {
        let req = RegisterChannelRequest {
            channel_id: self.channel_id.clone(),
            service_url: service_url.to_string(),
            tenant_id: self.tenant_id.clone(),
        };
        let response = self.inner.register_channel(req).await?;
        Ok(response.into_inner())
    }

    /// Send a message and get a complete response.
    #[tracing::instrument(skip(self))]
    pub async fn send_message(
        &mut self,
        sender_id: &str,
        text: &str,
        system_prompt: Option<&str>,
    ) -> Result<SendMessageResponse, ChannelError> {
        let req = SendMessageRequest {
            session_key: Some(common::SessionKey {
                tenant_id: self.tenant_id.clone(),
                channel_id: self.channel_id.clone(),
                sender_id: sender_id.to_string(),
            }),
            text: text.to_string(),
            attachments: vec![],
            stream: false,
            system_prompt: system_prompt.map(|s| s.to_string()),
        };
        let response = self.inner.send_message(req).await?;
        Ok(response.into_inner())
    }

    /// Send a message and stream back tokens.
    #[tracing::instrument(skip(self))]
    pub async fn send_message_streaming(
        &mut self,
        sender_id: &str,
        text: &str,
        system_prompt: Option<&str>,
    ) -> Result<tonic::Streaming<StreamChunk>, ChannelError> {
        let req = SendMessageRequest {
            session_key: Some(common::SessionKey {
                tenant_id: self.tenant_id.clone(),
                channel_id: self.channel_id.clone(),
                sender_id: sender_id.to_string(),
            }),
            text: text.to_string(),
            attachments: vec![],
            stream: true,
            system_prompt: system_prompt.map(|s| s.to_string()),
        };
        let response = self.inner.send_message_streaming(req).await?;
        Ok(response.into_inner())
    }

    /// Send a heartbeat.
    #[tracing::instrument(skip(self))]
    pub async fn heartbeat(&mut self) -> Result<HeartbeatResponse, ChannelError> {
        let req = HeartbeatRequest {
            channel_id: self.channel_id.clone(),
        };
        let response = self.inner.heartbeat(req).await?;
        Ok(response.into_inner())
    }

    /// Get session state.
    #[tracing::instrument(skip(self))]
    pub async fn get_session(
        &mut self,
        sender_id: &str,
    ) -> Result<GetSessionResponse, ChannelError> {
        let req = GetSessionRequest {
            session_key: Some(common::SessionKey {
                tenant_id: self.tenant_id.clone(),
                channel_id: self.channel_id.clone(),
                sender_id: sender_id.to_string(),
            }),
        };
        let response = self.inner.get_session(req).await?;
        Ok(response.into_inner())
    }
}
