//! HTTP request handlers for the web channel service.

use acton_service::prelude::*;
use talon_channel_sdk::proto::gateway::stream_chunk::Chunk;
use talon_types::{ChatRequest, ChatResponse};

use crate::routes::WebState;

/// POST /api/v1/chat -- synchronous chat request.
#[tracing::instrument(skip(state))]
pub async fn chat(
    Extension(state): Extension<WebState>,
    Json(req): Json<ChatRequest>,
) -> std::result::Result<Json<ChatResponse>, StatusCode> {
    let sender_id = req.sender_id.as_deref().unwrap_or("web-anonymous");

    let mut gateway = state.gateway.lock().await;
    let response = gateway
        .send_message(sender_id, &req.message, req.system_prompt.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Gateway error: {}", e);
            StatusCode::BAD_GATEWAY
        })?;

    Ok(Json(ChatResponse {
        text: response.text,
        session_id: response.session_id,
        model: response.model,
        token_count: response.token_count,
    }))
}

/// POST /api/v1/chat/stream -- SSE streaming chat response.
#[tracing::instrument(skip(state))]
pub async fn chat_stream(
    Extension(state): Extension<WebState>,
    Json(req): Json<ChatRequest>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let sender_id = req
        .sender_id
        .as_deref()
        .unwrap_or("web-anonymous")
        .to_string();
    let message = req.message.clone();
    let system_prompt = req.system_prompt.clone();

    let (tx, rx) =
        tokio::sync::mpsc::channel::<std::result::Result<SseEvent, std::convert::Infallible>>(32);
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let gateway = state.gateway.clone();

    tokio::spawn(async move {
        let mut gateway = gateway.lock().await;
        match gateway
            .send_message_streaming(&sender_id, &message, system_prompt.as_deref())
            .await
        {
            Ok(mut grpc_stream) => {
                use futures::StreamExt;
                while let Some(chunk_result) = grpc_stream.next().await {
                    match chunk_result {
                        Ok(stream_chunk) => {
                            let data = match stream_chunk.chunk {
                                Some(Chunk::Token(token)) => {
                                    serde_json::json!({"type": "token", "data": token}).to_string()
                                }
                                Some(Chunk::ToolCall(call)) => {
                                    serde_json::json!({"type": "tool_call", "data": call})
                                        .to_string()
                                }
                                Some(Chunk::FinalResponse(resp)) => serde_json::json!({
                                    "type": "done",
                                    "data": {
                                        "text": resp.text,
                                        "session_id": resp.session_id,
                                        "model": resp.model,
                                        "token_count": resp.token_count,
                                    }
                                })
                                .to_string(),
                                Some(Chunk::Error(err)) => {
                                    serde_json::json!({"type": "error", "data": err}).to_string()
                                }
                                None => continue,
                            };
                            let event = SseEvent::default().data(data);
                            if tx.send(Ok(event)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let data = serde_json::json!({"type": "error", "data": e.to_string()})
                                .to_string();
                            let event = SseEvent::default().data(data);
                            let _ = tx.send(Ok(event)).await;
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let data = serde_json::json!({"type": "error", "data": e.to_string()}).to_string();
                let event = SseEvent::default().data(data);
                let _ = tx.send(Ok(event)).await;
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
