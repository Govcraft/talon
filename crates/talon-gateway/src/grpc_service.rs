use talon_types::{ChannelId, SenderId, SessionKey, TenantId};
use tonic::{Request, Response, Status};

use crate::inference::InferenceService;
use crate::session_store::SessionStore;

// Include generated proto code.
// The gateway module must be nested inside common so that `super::SessionKey`
// references resolve correctly in the generated code.
pub mod proto {
    pub mod common {
        tonic::include_proto!("talon");

        pub mod gateway {
            tonic::include_proto!("talon.gateway");
        }
    }
}

use proto::common::gateway::gateway_service_server::{GatewayService, GatewayServiceServer};
use proto::common::gateway::*;

pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("talon_gateway_descriptor");

pub struct GatewayGrpcService;

#[tonic::async_trait]
impl GatewayService for GatewayGrpcService {
    #[tracing::instrument(skip(self, request))]
    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let req = request.into_inner();
        let session_key = req
            .session_key
            .ok_or_else(|| Status::invalid_argument("session_key is required"))?;

        let key = SessionKey::new(
            TenantId::new(&session_key.tenant_id),
            ChannelId::new(&session_key.channel_id),
            SenderId::new(&session_key.sender_id),
        );

        let state = acton_service::prelude::AppState::default();
        let inference = InferenceService::from_state(&state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let session_store = SessionStore::from_state(&state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let session = session_store
            .get_or_create(&key)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = inference
            .prompt(&req.text, req.system_prompt.as_deref(), &session)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        session_store
            .record_exchange(&session.id, &req.text, &response.text, response.token_count)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(SendMessageResponse {
            text: response.text,
            session_id: session.id.as_str().to_string(),
            model: response.model,
            token_count: response.token_count,
        }))
    }

    type SendMessageStreamingStream =
        tokio_stream::wrappers::ReceiverStream<Result<StreamChunk, Status>>;

    #[tracing::instrument(skip(self, request))]
    async fn send_message_streaming(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<Self::SendMessageStreamingStream>, Status> {
        let req = request.into_inner();
        let session_key = req
            .session_key
            .ok_or_else(|| Status::invalid_argument("session_key is required"))?;

        let key = SessionKey::new(
            TenantId::new(&session_key.tenant_id),
            ChannelId::new(&session_key.channel_id),
            SenderId::new(&session_key.sender_id),
        );

        let state = acton_service::prelude::AppState::default();
        let inference = InferenceService::from_state(&state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let session_store = SessionStore::from_state(&state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let session = session_store
            .get_or_create(&key)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            match inference
                .prompt(&req.text, req.system_prompt.as_deref(), &session)
                .await
            {
                Ok(response) => {
                    let chunk = StreamChunk {
                        chunk: Some(stream_chunk::Chunk::FinalResponse(SendMessageResponse {
                            text: response.text,
                            session_id: session.id.as_str().to_string(),
                            model: response.model,
                            token_count: response.token_count,
                        })),
                    };
                    let _ = tx.send(Ok(chunk)).await;
                }
                Err(e) => {
                    let chunk = StreamChunk {
                        chunk: Some(stream_chunk::Chunk::Error(e.to_string())),
                    };
                    let _ = tx.send(Ok(chunk)).await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    #[tracing::instrument(skip(self, request))]
    async fn register_channel(
        &self,
        request: Request<RegisterChannelRequest>,
    ) -> Result<Response<RegisterChannelResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            channel_id = %req.channel_id,
            service_url = %req.service_url,
            tenant_id = %req.tenant_id,
            "Channel registered"
        );
        Ok(Response::new(RegisterChannelResponse {
            success: true,
            token: String::new(), // Phase 5 will add PASETO tokens
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!(channel_id = %req.channel_id, "Heartbeat received");
        Ok(Response::new(HeartbeatResponse { ok: true }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let req = request.into_inner();
        let session_key = req
            .session_key
            .ok_or_else(|| Status::invalid_argument("session_key is required"))?;

        let key = SessionKey::new(
            TenantId::new(&session_key.tenant_id),
            ChannelId::new(&session_key.channel_id),
            SenderId::new(&session_key.sender_id),
        );

        let state = acton_service::prelude::AppState::default();
        let session_store = SessionStore::from_state(&state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let session = session_store
            .get_or_create(&key)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetSessionResponse {
            session_id: session.id.as_str().to_string(),
            status: format!("{:?}", session.status),
            message_count: session.message_count,
            total_tokens: session.total_tokens,
        }))
    }
}

/// Create the gRPC service server.
#[tracing::instrument]
pub fn create_gateway_service() -> GatewayServiceServer<GatewayGrpcService> {
    GatewayServiceServer::new(GatewayGrpcService)
}
