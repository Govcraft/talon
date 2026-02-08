//! Session repository -- database-backed session storage per tenant namespace.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::DbClient;
use crate::error::GatewayError;
use talon_types::{AgentId, Session, SessionId, SessionKey, SessionStatus};

/// Internal row for SurrealDB serialisation.
#[derive(Debug, Serialize)]
struct SessionRecord {
    id: String,
    tenant_id: String,
    channel_id: String,
    sender_id: String,
    agent_id: Option<String>,
    status: String,
    message_count: u32,
    total_tokens: u64,
    created_at: String,
    updated_at: String,
}

/// Internal row for SurrealDB deserialisation.
#[derive(Debug, Deserialize)]
struct SessionRow {
    #[allow(dead_code)]
    id: surrealdb::sql::Thing,
    tenant_id: String,
    channel_id: String,
    sender_id: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default = "default_status")]
    status: String,
    #[serde(default)]
    message_count: u32,
    #[serde(default)]
    total_tokens: u64,
    created_at: String,
    updated_at: String,
}

fn default_status() -> String {
    "active".to_string()
}

impl SessionRow {
    fn into_session(self) -> Session {
        use talon_types::{ChannelId, SenderId, TenantId};

        let raw_id = self.id.id.to_raw();
        Session {
            id: SessionId::new(raw_id),
            session_key: SessionKey::new(
                TenantId::new(self.tenant_id),
                ChannelId::new(self.channel_id),
                SenderId::new(self.sender_id),
            ),
            agent_id: self.agent_id.map(AgentId::new),
            status: match self.status.as_str() {
                "idle" => SessionStatus::Idle,
                "closed" => SessionStatus::Closed,
                _ => SessionStatus::Active,
            },
            message_count: self.message_count,
            total_tokens: self.total_tokens,
            created_at: self.created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: self.updated_at.parse().unwrap_or_else(|_| Utc::now()),
        }
    }
}

/// Get an existing session matching the key, or create a new one.
#[tracing::instrument(skip(client))]
pub async fn get_or_create_session(
    client: &DbClient,
    tenant_ns: &str,
    key: &SessionKey,
) -> std::result::Result<Session, GatewayError> {
    let tenant_id = key.tenant_id.as_str().to_string();
    let channel_id = key.channel_id.as_str().to_string();
    let sender_id = key.sender_id.as_str().to_string();

    // Try to find existing session.
    let select_query = format!(
        "USE NS `{tenant_ns}` DB main; \
         SELECT * FROM session \
         WHERE tenant_id = $tenant_id \
           AND channel_id = $channel_id \
           AND sender_id = $sender_id \
         LIMIT 1"
    );
    let mut result = client
        .query(&select_query)
        .bind(("tenant_id", tenant_id.clone()))
        .bind(("channel_id", channel_id.clone()))
        .bind(("sender_id", sender_id.clone()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<SessionRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    if let Some(row) = rows.into_iter().next() {
        return Ok(row.into_session());
    }

    // No existing session -- create a new one.
    let session_id = SessionId::generate();
    let now = Utc::now();
    let record = SessionRecord {
        id: session_id.as_str().to_string(),
        tenant_id,
        channel_id,
        sender_id,
        agent_id: None,
        status: "active".to_string(),
        message_count: 0,
        total_tokens: 0,
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
    };

    let data = serde_json::to_value(&record).map_err(|e| GatewayError::Internal(e.to_string()))?;

    let create_query =
        format!("USE NS `{tenant_ns}` DB main; CREATE type::thing('session', $id) CONTENT $data");
    let mut result = client
        .query(&create_query)
        .bind(("id", session_id.as_str().to_string()))
        .bind(("data", data))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<SessionRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    rows.into_iter()
        .next()
        .map(SessionRow::into_session)
        .ok_or_else(|| GatewayError::Internal("failed to create session record".into()))
}

/// Get a session by ID.
#[tracing::instrument(skip(client))]
pub async fn get_session(
    client: &DbClient,
    tenant_ns: &str,
    id: &str,
) -> std::result::Result<Option<Session>, GatewayError> {
    let query = format!("USE NS `{tenant_ns}` DB main; SELECT * FROM type::thing('session', $id)");
    let mut result = client
        .query(&query)
        .bind(("id", id.to_string()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<SessionRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().next().map(SessionRow::into_session))
}

/// List all sessions in a tenant namespace.
#[tracing::instrument(skip(client))]
pub async fn list_sessions(
    client: &DbClient,
    tenant_ns: &str,
) -> std::result::Result<Vec<Session>, GatewayError> {
    let query =
        format!("USE NS `{tenant_ns}` DB main; SELECT * FROM session ORDER BY updated_at DESC");
    let mut result = client
        .query(&query)
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    let rows: Vec<SessionRow> = result
        .take(1)
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(rows.into_iter().map(SessionRow::into_session).collect())
}

/// Record a completed exchange (user message + assistant reply) against a session.
#[tracing::instrument(skip(client))]
pub async fn record_exchange(
    client: &DbClient,
    tenant_ns: &str,
    session_id: &SessionId,
    _user_msg: &str,
    _assistant_msg: &str,
    token_count: u32,
) -> std::result::Result<(), GatewayError> {
    let query = format!(
        "USE NS `{tenant_ns}` DB main; \
         UPDATE type::thing('session', $id) SET \
           message_count += 2, \
           total_tokens += $tokens, \
           updated_at = $now"
    );
    client
        .query(&query)
        .bind(("id", session_id.as_str().to_string()))
        .bind(("tokens", u64::from(token_count)))
        .bind(("now", Utc::now().to_rfc3339()))
        .await
        .map_err(|e| GatewayError::Database(e.to_string()))?;

    Ok(())
}
