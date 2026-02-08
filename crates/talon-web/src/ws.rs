//! WebSocket handler for bidirectional chat.

use acton_service::prelude::*;
use serde::{Deserialize, Serialize};

use crate::routes::WebState;

/// Inbound WebSocket message from a client.
#[derive(Debug, Deserialize)]
struct WsInbound {
    message: String,
    #[serde(default)]
    sender_id: Option<String>,
    #[serde(default)]
    system_prompt: Option<String>,
}

/// Outbound WebSocket message to a client.
#[derive(Debug, Serialize)]
struct WsOutbound {
    text: String,
    session_id: String,
    model: String,
    token_count: u32,
}

/// GET /api/v1/ws/chat -- WebSocket upgrade handler.
#[tracing::instrument(skip_all)]
pub async fn ws_handler(
    Extension(state): Extension<WebState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

#[tracing::instrument(skip_all)]
async fn handle_socket(mut socket: WebSocket, state: WebState) {
    while let Some(msg) = socket.recv().await {
        let text = match msg {
            Ok(WsMessage::Text(t)) => t.to_string(),
            Ok(WsMessage::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let inbound: WsInbound = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"error": format!("Invalid JSON: {}", e)}).to_string();
                if socket.send(WsMessage::Text(err.into())).await.is_err() {
                    break;
                }
                continue;
            }
        };

        let sender_id = inbound.sender_id.as_deref().unwrap_or("ws-anonymous");

        let mut gateway = state.gateway.lock().await;
        match gateway
            .send_message(
                sender_id,
                &inbound.message,
                inbound.system_prompt.as_deref(),
            )
            .await
        {
            Ok(response) => {
                let outbound = WsOutbound {
                    text: response.text,
                    session_id: response.session_id,
                    model: response.model,
                    token_count: response.token_count,
                };
                let json = serde_json::to_string(&outbound).unwrap_or_default();
                if socket.send(WsMessage::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                let err = serde_json::json!({"error": e.to_string()}).to_string();
                if socket.send(WsMessage::Text(err.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
