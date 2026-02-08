use acton_service::prelude::*;
use talon_types::{ChannelId, ChatRequest, ChatResponse, SenderId, SessionKey, TenantId};

use crate::audit;
use crate::inference::InferenceService;
use crate::session_store::SessionStore;

/// POST /api/v1/chat - synchronous request/response chat.
#[tracing::instrument(skip(state))]
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> std::result::Result<Json<ChatResponse>, StatusCode> {
    let session_key = SessionKey::new(
        TenantId::new(req.tenant_id.as_deref().unwrap_or("default")),
        ChannelId::new("http"),
        SenderId::new(req.sender_id.as_deref().unwrap_or("anonymous")),
    );

    let inference = InferenceService::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session_store = SessionStore::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = session_store
        .get_or_create(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = inference
        .prompt(&req.message, req.system_prompt.as_deref(), &session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    session_store
        .record_exchange(
            &session.id,
            &req.message,
            &response.text,
            response.token_count,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    audit::log_chat_event(
        state.audit_logger(),
        session_key.tenant_id.as_str(),
        session_key.sender_id.as_str(),
        &response.model,
    )
    .await;

    Ok(Json(response))
}

/// POST /api/v1/chat/stream - SSE streaming chat.
#[tracing::instrument(skip(state))]
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    let session_key = SessionKey::new(
        TenantId::new(req.tenant_id.as_deref().unwrap_or("default")),
        ChannelId::new("http"),
        SenderId::new(req.sender_id.as_deref().unwrap_or("anonymous")),
    );

    let inference = InferenceService::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session_store = SessionStore::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = session_store
        .get_or_create(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let msg = req.message.clone();
    let sys = req.system_prompt.clone();

    tokio::spawn(async move {
        let result = inference
            .prompt_streaming(&msg, sys.as_deref(), &session, tx.clone())
            .await;
        if let Err(e) = result {
            let error_data =
                serde_json::json!({"type": "error", "data": e.to_string()}).to_string();
            let event = SseEvent::default().data(error_data);
            let _ = tx.send(Ok(event)).await;
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// GET /api/v1/sessions - list all sessions.
#[tracing::instrument(skip(state))]
pub async fn list_sessions(
    State(state): State<AppState>,
) -> std::result::Result<Json<Vec<talon_types::Session>>, StatusCode> {
    let session_store = SessionStore::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let sessions = session_store
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(sessions))
}

/// GET /api/v1/sessions/{id} - get a session by identifier.
#[tracing::instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<talon_types::Session>, StatusCode> {
    let session_store = SessionStore::from_state(&state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    session_store
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}
