# Phase 4: Multi-Tenancy Implementation Plan

## Summary

Replace the in-memory session store in `talon-gateway` with SurrealDB-backed multi-tenant
persistence. Add tenant and agent CRUD APIs. Implement namespace-per-tenant isolation so
each tenant's data is stored in its own SurrealDB namespace.

## Current State

- **talon-gateway** uses a global `LazyLock<Arc<RwLock<HashMap>>>` session store
- **acton-service** (v0.16) provides SurrealDB integration via `AppState::surrealdb()`
  which returns `Option<Arc<SurrealClient>>` -- the client is managed by an
  `SurrealDbAgent` actor that connects based on `[surrealdb]` config section
- `SurrealClient` = `surrealdb::Surreal<surrealdb::engine::any::Any>` (runtime protocol)
- SurrealDB queries use `client.query("SurrealQL").bind(("key", val)).await` pattern
- Results are extracted with `result.take::<Vec<T>>(0)` (statement index)
- The gateway's `ServiceBuilder::new().build()` automatically initialises the SurrealDB
  agent when `[surrealdb]` config is present

## Isolation Strategy

```
SurrealDB
  namespace: "system"              # Global admin data
    database: "admin"
      table: tenant                # Tenant registry
      table: channel_binding       # Channel -> tenant mappings

  namespace: "tenant_{slug}"       # Isolated per tenant
    database: "main"
      table: agent                 # Agent configs
      table: session               # Conversation sessions
      table: conversation          # Message history
      table: usage                 # Token/request tracking
```

The `AppState` SurrealDB client connects to `system:admin` by default (set in config).
For tenant-scoped operations, each query explicitly switches namespace:
```rust
client.query("USE NS $ns DB main; SELECT * FROM session WHERE ...")
    .bind(("ns", tenant_id.namespace()))
    .await
```

## Implementation Steps

### Step 1: Add Types to `talon-types`

**File: `/home/rodzilla/projects/talon/crates/talon-types/src/agent.rs`** (new)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::{AgentId, TenantId};

/// An AI agent configured for a specific tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub tenant_id: TenantId,
    pub name: String,
    pub system_prompt: Option<String>,
    pub provider: String,
    pub model: String,
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub trust_tier: u8,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new agent.
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub trust_tier: u8,
    #[serde(default)]
    pub is_default: bool,
}

fn default_provider() -> String { "ollama".to_string() }
fn default_model() -> String { "llama3.2".to_string() }

/// Request to update an agent.
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub system_prompt: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tools: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub trust_tier: Option<u8>,
    pub is_default: Option<bool>,
}
```

**File: `/home/rodzilla/projects/talon/crates/talon-types/src/usage.rs`** (new)

```rust
use serde::{Deserialize, Serialize};
use crate::TenantId;

/// Usage record tracking token consumption per tenant/period/model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub tenant_id: TenantId,
    pub period: String,       // "2026-02"
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub request_count: u64,
}
```

**File: `/home/rodzilla/projects/talon/crates/talon-types/src/tenant.rs`** (modify)

Add request/response types:

```rust
/// Request to create a new tenant.
#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub plan: Plan,
    #[serde(default)]
    pub settings: Option<TenantSettings>,
}

/// Request to update a tenant.
#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub status: Option<TenantStatus>,
    pub plan: Option<Plan>,
    pub settings: Option<TenantSettings>,
}
```

**File: `/home/rodzilla/projects/talon/crates/talon-types/src/lib.rs`** (modify)

Add modules:
```rust
pub mod agent;
pub mod usage;
pub use agent::*;
pub use usage::*;
```

### Step 2: Create SurrealDB Data Layer in Gateway

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/mod.rs`** (new)

```rust
pub mod schema;
pub mod tenant_repo;
pub mod agent_repo;
pub mod session_repo;
pub mod usage_repo;

use std::sync::Arc;
use acton_service::surrealdb_backend::SurrealClient;

/// Convenience type alias for the shared SurrealDB client.
pub type DbClient = Arc<SurrealClient>;

/// Extract the SurrealDB client from AppState, returning a gateway error on failure.
pub async fn get_db(state: &acton_service::prelude::AppState) -> std::result::Result<DbClient, crate::error::GatewayError> {
    state.surrealdb().await.ok_or(crate::error::GatewayError::DatabaseUnavailable)
}
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/schema.rs`** (new)

Initialisation queries run at startup:

```rust
use super::DbClient;

/// Initialise the system namespace schema (tenant registry, channel bindings).
pub async fn init_system_schema(client: &DbClient) -> std::result::Result<(), crate::error::GatewayError> {
    client.query(r#"
        USE NS system DB admin;

        DEFINE TABLE IF NOT EXISTS tenant SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS id         ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS name       ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS slug       ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS status     ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS plan       ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS settings   ON tenant TYPE object;
        DEFINE FIELD IF NOT EXISTS created_at ON tenant TYPE string;
        DEFINE FIELD IF NOT EXISTS updated_at ON tenant TYPE string;
        DEFINE INDEX IF NOT EXISTS idx_tenant_slug ON tenant FIELDS slug UNIQUE;

        DEFINE TABLE IF NOT EXISTS channel_binding SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS channel_id ON channel_binding TYPE string;
        DEFINE FIELD IF NOT EXISTS tenant_id  ON channel_binding TYPE string;
        DEFINE FIELD IF NOT EXISTS service_url ON channel_binding TYPE string;
        DEFINE FIELD IF NOT EXISTS registered_at ON channel_binding TYPE string;
        DEFINE INDEX IF NOT EXISTS idx_channel_binding ON channel_binding FIELDS channel_id UNIQUE;
    "#).await
      .map_err(|e| crate::error::GatewayError::Database(e.to_string()))?;
    Ok(())
}

/// Initialise a tenant namespace with standard tables.
pub async fn init_tenant_schema(client: &DbClient, namespace: &str) -> std::result::Result<(), crate::error::GatewayError> {
    let query = format!(r#"
        USE NS {namespace} DB main;

        DEFINE TABLE IF NOT EXISTS agent SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS id            ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS tenant_id     ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS name          ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS system_prompt ON agent TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS provider      ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS model         ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS tools         ON agent TYPE array;
        DEFINE FIELD IF NOT EXISTS skills        ON agent TYPE array;
        DEFINE FIELD IF NOT EXISTS trust_tier    ON agent TYPE int;
        DEFINE FIELD IF NOT EXISTS is_default    ON agent TYPE bool;
        DEFINE FIELD IF NOT EXISTS created_at    ON agent TYPE string;
        DEFINE FIELD IF NOT EXISTS updated_at    ON agent TYPE string;

        DEFINE TABLE IF NOT EXISTS session SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS id            ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS tenant_id     ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS channel_id    ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS sender_id     ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS agent_id      ON session TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS status        ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS message_count ON session TYPE int;
        DEFINE FIELD IF NOT EXISTS total_tokens  ON session TYPE int;
        DEFINE FIELD IF NOT EXISTS created_at    ON session TYPE string;
        DEFINE FIELD IF NOT EXISTS updated_at    ON session TYPE string;
        DEFINE INDEX IF NOT EXISTS idx_session_key ON session FIELDS tenant_id, channel_id, sender_id UNIQUE;

        DEFINE TABLE IF NOT EXISTS conversation SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS session_id   ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS role         ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS content      ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS tool_calls   ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS token_count  ON conversation TYPE int;
        DEFINE FIELD IF NOT EXISTS provider     ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS model        ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp    ON conversation TYPE string;

        DEFINE TABLE IF NOT EXISTS usage SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS tenant_id    ON usage TYPE string;
        DEFINE FIELD IF NOT EXISTS period       ON usage TYPE string;
        DEFINE FIELD IF NOT EXISTS provider     ON usage TYPE string;
        DEFINE FIELD IF NOT EXISTS model        ON usage TYPE string;
        DEFINE FIELD IF NOT EXISTS input_tokens  ON usage TYPE int;
        DEFINE FIELD IF NOT EXISTS output_tokens ON usage TYPE int;
        DEFINE FIELD IF NOT EXISTS request_count ON usage TYPE int;
        DEFINE INDEX IF NOT EXISTS idx_usage_key ON usage FIELDS tenant_id, period, provider, model UNIQUE;
    "#);
    client.query(&query).await
        .map_err(|e| crate::error::GatewayError::Database(e.to_string()))?;
    Ok(())
}
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/tenant_repo.rs`** (new)

CRUD for tenants in `system:admin`:

```rust
use super::DbClient;
use talon_types::{Tenant, TenantId, TenantSettings, TenantStatus, Plan, CreateTenantRequest, UpdateTenantRequest};
use crate::error::GatewayError;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Serialisable record for SurrealDB.
#[derive(Serialize)]
struct TenantRecord { /* mirrors Tenant fields as strings */ }

/// Deserialisable record from SurrealDB.
#[derive(Deserialize)]
struct TenantRow { /* mirrors Tenant fields */ }

pub async fn create_tenant(client: &DbClient, req: &CreateTenantRequest) -> std::result::Result<Tenant, GatewayError>;
pub async fn list_tenants(client: &DbClient) -> std::result::Result<Vec<Tenant>, GatewayError>;
pub async fn get_tenant(client: &DbClient, id: &TenantId) -> std::result::Result<Option<Tenant>, GatewayError>;
pub async fn update_tenant(client: &DbClient, id: &TenantId, req: &UpdateTenantRequest) -> std::result::Result<Option<Tenant>, GatewayError>;
pub async fn delete_tenant(client: &DbClient, id: &TenantId) -> std::result::Result<bool, GatewayError>;
```

All queries use:
```rust
client.query("USE NS system DB admin; <query>")
    .bind(...)
    .await
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/agent_repo.rs`** (new)

CRUD for agents in `tenant_{slug}:main`:

```rust
pub async fn create_agent(client: &DbClient, tenant_ns: &str, tenant_id: &TenantId, req: &CreateAgentRequest) -> Result<Agent, GatewayError>;
pub async fn list_agents(client: &DbClient, tenant_ns: &str) -> Result<Vec<Agent>, GatewayError>;
pub async fn get_agent(client: &DbClient, tenant_ns: &str, agent_id: &AgentId) -> Result<Option<Agent>, GatewayError>;
pub async fn update_agent(client: &DbClient, tenant_ns: &str, agent_id: &AgentId, req: &UpdateAgentRequest) -> Result<Option<Agent>, GatewayError>;
pub async fn delete_agent(client: &DbClient, tenant_ns: &str, agent_id: &AgentId) -> Result<bool, GatewayError>;
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/session_repo.rs`** (new)

Replace the in-memory session store:

```rust
pub async fn get_or_create_session(client: &DbClient, tenant_ns: &str, key: &SessionKey) -> Result<Session, GatewayError>;
pub async fn get_session(client: &DbClient, tenant_ns: &str, id: &str) -> Result<Option<Session>, GatewayError>;
pub async fn list_sessions(client: &DbClient, tenant_ns: &str) -> Result<Vec<Session>, GatewayError>;
pub async fn record_exchange(client: &DbClient, tenant_ns: &str, session_id: &SessionId, user_msg: &str, assistant_msg: &str, token_count: u32) -> Result<(), GatewayError>;
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/db/usage_repo.rs`** (new)

```rust
pub async fn increment_usage(client: &DbClient, tenant_ns: &str, tenant_id: &TenantId, provider: &str, model: &str, input_tokens: u64, output_tokens: u64) -> Result<(), GatewayError>;
pub async fn get_usage(client: &DbClient, tenant_ns: &str, tenant_id: &TenantId, period: &str) -> Result<Vec<UsageRecord>, GatewayError>;
```

### Step 3: Add Gateway Error Type

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/error.rs`** (new)

```rust
use acton_service::prelude::*;

/// Errors produced by the gateway service.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("database unavailable")]
    DatabaseUnavailable,

    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            GatewayError::DatabaseUnavailable => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            GatewayError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database error".to_string()),
            GatewayError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            GatewayError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            GatewayError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            GatewayError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}
```

### Step 4: Tenant and Agent HTTP Handlers

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/tenant_handlers.rs`** (new)

```rust
use acton_service::prelude::*;
use crate::db;
use crate::error::GatewayError;
use talon_types::*;

pub async fn create_tenant(State(state): State<AppState>, Json(req): Json<CreateTenantRequest>) -> std::result::Result<(StatusCode, Json<Tenant>), GatewayError>;
pub async fn list_tenants(State(state): State<AppState>) -> std::result::Result<Json<Vec<Tenant>>, GatewayError>;
pub async fn get_tenant(State(state): State<AppState>, Path(id): Path<String>) -> std::result::Result<Json<Tenant>, GatewayError>;
pub async fn update_tenant(State(state): State<AppState>, Path(id): Path<String>, Json(req): Json<UpdateTenantRequest>) -> std::result::Result<Json<Tenant>, GatewayError>;
pub async fn delete_tenant(State(state): State<AppState>, Path(id): Path<String>) -> std::result::Result<StatusCode, GatewayError>;
```

The `create_tenant` handler:
1. Validates slug format (alphanumeric + hyphens)
2. Creates tenant record in `system:admin`
3. Calls `init_tenant_schema()` to create the namespace

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/agent_handlers.rs`** (new)

```rust
pub async fn create_agent(State(state): State<AppState>, Path(tenant_id): Path<String>, Json(req): Json<CreateAgentRequest>) -> std::result::Result<(StatusCode, Json<Agent>), GatewayError>;
pub async fn list_agents(State(state): State<AppState>, Path(tenant_id): Path<String>) -> std::result::Result<Json<Vec<Agent>>, GatewayError>;
pub async fn update_agent(State(state): State<AppState>, Path((tenant_id, agent_id)): Path<(String, String)>, Json(req): Json<UpdateAgentRequest>) -> std::result::Result<Json<Agent>, GatewayError>;
pub async fn delete_agent(State(state): State<AppState>, Path((tenant_id, agent_id)): Path<(String, String)>) -> std::result::Result<StatusCode, GatewayError>;
```

Each handler:
1. Looks up the tenant in `system:admin` to get its slug
2. Computes namespace = `tenant_{slug}`
3. Performs the operation in the tenant namespace

### Step 5: Update Routes

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/routes.rs`** (modify)

```rust
pub fn build_routes() -> VersionedRoutes {
    VersionedApiBuilder::new()
        .with_base_path("/api")
        .add_version(ApiVersion::V1, |router| {
            router
                // Chat
                .route("/chat", post(handlers::chat))
                .route("/chat/stream", post(handlers::chat_stream))
                // Sessions
                .route("/sessions", get(handlers::list_sessions))
                .route("/sessions/{id}", get(handlers::get_session))
                // Tenants
                .route("/tenants", post(tenant_handlers::create_tenant).get(tenant_handlers::list_tenants))
                .route("/tenants/{id}", get(tenant_handlers::get_tenant).put(tenant_handlers::update_tenant).delete(tenant_handlers::delete_tenant))
                // Agents (per-tenant)
                .route("/tenants/{tenant_id}/agents", post(agent_handlers::create_agent).get(agent_handlers::list_agents))
                .route("/tenants/{tenant_id}/agents/{agent_id}", put(agent_handlers::update_agent).delete(agent_handlers::delete_agent))
        })
        .build_routes()
}
```

### Step 6: Update Existing Handlers for Tenant-Scoped Sessions

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/handlers.rs`** (modify)

Change `SessionStore::from_state` calls to use the new `db::session_repo` functions:
- Extract tenant_id from request
- Look up tenant to get namespace
- Call `db::session_repo::get_or_create_session(client, namespace, key)`
- After inference, call `db::usage_repo::increment_usage()`

### Step 7: Update `session_store.rs`

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/session_store.rs`** (modify)

Rewrite `SessionStore` to delegate to `db::session_repo`. The struct now holds a
`DbClient` instead of an in-memory map. The `from_state` method extracts the client
from `AppState::surrealdb()`. The `get_or_create`, `get`, `list`, and `record_exchange`
methods become thin wrappers around the repo functions, passing the tenant namespace.

For backward compatibility with handlers that don't yet pass a tenant, use "default"
as the tenant namespace. The default tenant will be auto-created on startup.

### Step 8: Update `lib.rs` and `main.rs`

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/lib.rs`** (modify)

```rust
pub mod agent_handlers;
pub mod db;
pub mod error;
pub mod grpc_service;
pub mod handlers;
pub mod inference;
pub mod routes;
pub mod session_store;
pub mod tenant_handlers;
```

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/src/main.rs`** (modify)

After `ServiceBuilder::new()...build()`, but before `.serve()`, run schema init:
```rust
// Initialise SurrealDB schema after service builds (and client connects)
// We use a startup task that retries until the client is available
tokio::spawn(async move {
    // Wait for SurrealDB client
    // Then init_system_schema + create "default" tenant + init_tenant_schema
});
```

Actually, since `ServiceBuilder::build()` is synchronous and the SurrealDB agent
connects lazily, we'll add a startup initialisation step. We can use
`ActonService::serve()` which does not return until shutdown. So we spawn a task
before calling serve that waits for `state.surrealdb()` to become `Some`, then runs
the schema init.

### Step 9: Add `surrealdb` Dependency to Gateway Cargo.toml

**File: `/home/rodzilla/projects/talon/crates/talon-gateway/Cargo.toml`** (modify)

Add `surrealdb` to dependencies:
```toml
surrealdb = { version = "2.6", features = ["protocol-ws", "kv-mem"] }
```

This is needed because we reference `surrealdb::Error` and SurrealDB types directly
in the repo modules. The `acton-service` re-exports `SurrealClient` but not the
full surrealdb crate.

### Step 10: Config File

**File: `/home/rodzilla/projects/talon/config.toml`** (new)

```toml
[service]
name = "talon-gateway"
port = 8080
log_level = "info"
timeout_secs = 30

[surrealdb]
url = "ws://localhost:8000"
namespace = "system"
database = "admin"
username = "root"
password = "root"
max_retries = 5
retry_delay_secs = 2
optional = false
lazy_init = true
```

## File Summary

### New Files
| File | Purpose |
|------|---------|
| `talon-types/src/agent.rs` | Agent, CreateAgentRequest, UpdateAgentRequest types |
| `talon-types/src/usage.rs` | UsageRecord type |
| `talon-gateway/src/error.rs` | GatewayError enum with IntoResponse |
| `talon-gateway/src/db/mod.rs` | DB module root, `get_db()` helper |
| `talon-gateway/src/db/schema.rs` | Schema init for system + tenant namespaces |
| `talon-gateway/src/db/tenant_repo.rs` | Tenant CRUD against system:admin |
| `talon-gateway/src/db/agent_repo.rs` | Agent CRUD against tenant namespace |
| `talon-gateway/src/db/session_repo.rs` | Session operations against tenant namespace |
| `talon-gateway/src/db/usage_repo.rs` | Usage tracking against tenant namespace |
| `talon-gateway/src/tenant_handlers.rs` | HTTP handlers for /tenants/* |
| `talon-gateway/src/agent_handlers.rs` | HTTP handlers for /tenants/{id}/agents/* |
| `config.toml` | SurrealDB configuration for the gateway |

### Modified Files
| File | Changes |
|------|---------|
| `talon-types/src/lib.rs` | Add `agent` and `usage` module exports |
| `talon-types/src/tenant.rs` | Add CreateTenantRequest, UpdateTenantRequest |
| `talon-gateway/src/lib.rs` | Add new module declarations |
| `talon-gateway/src/main.rs` | Add SurrealDB schema init on startup |
| `talon-gateway/src/routes.rs` | Add tenant and agent routes |
| `talon-gateway/src/handlers.rs` | Use DB-backed session resolution |
| `talon-gateway/src/session_store.rs` | Replace in-memory with SurrealDB delegation |
| `talon-gateway/Cargo.toml` | Add `surrealdb` dependency |

### Deleted Files
None -- `session_store.rs` is rewritten in-place, not deleted.

## Implementation Order

1. `talon-types` changes (agent.rs, usage.rs, tenant.rs request types)
2. `talon-gateway/Cargo.toml` dependency addition
3. `talon-gateway/src/error.rs`
4. `talon-gateway/src/db/` module (schema, repos)
5. `talon-gateway/src/tenant_handlers.rs`
6. `talon-gateway/src/agent_handlers.rs`
7. `talon-gateway/src/session_store.rs` rewrite
8. `talon-gateway/src/handlers.rs` update
9. `talon-gateway/src/routes.rs` update
10. `talon-gateway/src/lib.rs` and `main.rs` updates
11. `config.toml`

## Key Design Decisions

1. **Direct SurrealQL with `USE NS ... DB ...` prefix** -- SurrealDB supports inline
   namespace switching within a query string. This avoids needing separate client
   connections per tenant. The `USE NS ... DB ...` is prepended to each query.

2. **No separate SurrealDB clients per tenant** -- One shared client, namespace
   isolation via query-level `USE NS`. The SurrealDB Rust client supports this.

3. **Schema-on-demand** -- Tenant namespaces are created when a tenant is created via
   the API. The default tenant is auto-created at startup.

4. **Backward compatible** -- Existing handlers continue to work; if no tenant_id is
   specified, the "default" tenant is used.

5. **SurrealDB record IDs** -- Use `type::thing('table', $id)` pattern for
   deterministic record IDs (ULID-based), matching the existing `IdType::generate()`
   pattern in talon-types.

6. **GatewayError with IntoResponse** -- Allows handlers to return
   `Result<T, GatewayError>` and have errors automatically converted to proper HTTP
   responses with status codes and JSON bodies.

## Semver Recommendation

**Minor bump: 0.1.0 -> 0.2.0**

This adds new public types (Agent, UsageRecord, request types) and new API endpoints.
No existing public API is broken. The session store implementation changes from in-memory
to SurrealDB but the handler signatures remain the same.

## Verification

After implementation:
1. `cargo check --workspace` must pass
2. `cargo clippy --workspace -- -D warnings` must pass with zero warnings
3. Manually verify tenant CRUD endpoints with curl against a running SurrealDB
