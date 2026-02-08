//! Session store with SurrealDB persistence and in-memory fallback.
//!
//! When the SurrealDB client is available, sessions are stored in the
//! tenant-scoped namespace. When it is not (e.g. during startup before the
//! lazy connection completes), the store falls back to a global in-memory
//! `HashMap` so that existing functionality continues to work.

use std::collections::HashMap;
use std::sync::Arc;

use acton_service::prelude::AppState;
use talon_types::{Session, SessionId, SessionKey};
use tokio::sync::RwLock;

use crate::db;

/// The default tenant namespace used when no tenant ID is specified.
const DEFAULT_TENANT_NS: &str = "tenant_default";

/// Session store backed by SurrealDB with an in-memory fallback.
pub struct SessionStore {
    db_client: Option<db::DbClient>,
    fallback: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionStore {
    /// Obtain a session store from application state.
    ///
    /// Attempts to acquire the SurrealDB client. If unavailable, falls
    /// back to the global in-memory store.
    #[tracing::instrument(skip(state))]
    pub async fn from_state(state: &AppState) -> std::result::Result<Self, anyhow::Error> {
        let db_client = state.surrealdb().await;
        Ok(Self {
            db_client,
            fallback: Self::global_fallback(),
        })
    }

    /// Return the global in-memory fallback store.
    fn global_fallback() -> Arc<RwLock<HashMap<String, Session>>> {
        use std::sync::LazyLock;
        static STORE: LazyLock<Arc<RwLock<HashMap<String, Session>>>> =
            LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));
        STORE.clone()
    }

    /// Derive the tenant namespace from the session key. Uses the default
    /// namespace when the tenant ID is "default" or empty.
    fn tenant_ns(key: &SessionKey) -> String {
        let tid = key.tenant_id.as_str();
        if tid.is_empty() || tid == "default" {
            DEFAULT_TENANT_NS.to_string()
        } else {
            key.tenant_id.namespace()
        }
    }

    /// Retrieve an existing session matching the given key, or create a new one.
    #[tracing::instrument(skip(self))]
    pub async fn get_or_create(
        &self,
        key: &SessionKey,
    ) -> std::result::Result<Session, anyhow::Error> {
        if let Some(ref client) = self.db_client {
            let ns = Self::tenant_ns(key);
            db::session_repo::get_or_create_session(client, &ns, key)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        } else {
            self.fallback_get_or_create(key).await
        }
    }

    /// Look up a session by its identifier.
    #[tracing::instrument(skip(self))]
    pub async fn get(&self, id: &str) -> std::result::Result<Option<Session>, anyhow::Error> {
        if let Some(ref client) = self.db_client {
            db::session_repo::get_session(client, DEFAULT_TENANT_NS, id)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        } else {
            let sessions = self.fallback.read().await;
            Ok(sessions.get(id).cloned())
        }
    }

    /// Return all sessions currently stored.
    #[tracing::instrument(skip(self))]
    pub async fn list(&self) -> std::result::Result<Vec<Session>, anyhow::Error> {
        if let Some(ref client) = self.db_client {
            db::session_repo::list_sessions(client, DEFAULT_TENANT_NS)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        } else {
            let sessions = self.fallback.read().await;
            Ok(sessions.values().cloned().collect())
        }
    }

    /// Record a completed exchange (user message + assistant reply) against a session.
    #[tracing::instrument(skip(self))]
    pub async fn record_exchange(
        &self,
        session_id: &SessionId,
        user_msg: &str,
        assistant_msg: &str,
        token_count: u32,
    ) -> std::result::Result<(), anyhow::Error> {
        if let Some(ref client) = self.db_client {
            db::session_repo::record_exchange(
                client,
                DEFAULT_TENANT_NS,
                session_id,
                user_msg,
                assistant_msg,
                token_count,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
        } else {
            let mut sessions = self.fallback.write().await;
            if let Some(session) = sessions.get_mut(session_id.as_str()) {
                session.message_count += 2;
                session.total_tokens += u64::from(token_count);
                session.updated_at = chrono::Utc::now();
            }
            Ok(())
        }
    }

    // -- In-memory fallback implementation --

    async fn fallback_get_or_create(
        &self,
        key: &SessionKey,
    ) -> std::result::Result<Session, anyhow::Error> {
        let lookup = key.to_lookup_key();
        {
            let sessions = self.fallback.read().await;
            if let Some(session) = sessions
                .values()
                .find(|s| s.session_key.to_lookup_key() == lookup)
            {
                return Ok(session.clone());
            }
        }
        let session = Session::new(key.clone());
        let mut sessions = self.fallback.write().await;
        sessions.insert(session.id.as_str().to_string(), session.clone());
        Ok(session)
    }
}
