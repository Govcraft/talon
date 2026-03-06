# Talon Codebase Audit Findings

Generated: 2026-02-08

## Critical: Incomplete Implementations

### 1. gRPC streaming doesn't actually stream
**`crates/talon-gateway/src/grpc_service.rs:111-134`** - The `send_message_streaming` endpoint calls non-streaming inference and sends the entire response as a single chunk. Clients expecting token-by-token delivery get bulk response.

### 2. Session record_exchange discards messages
**`crates/talon-gateway/src/db/session_repo.rs:195-196`** - `_user_msg` and `_assistant_msg` parameters are ignored. Messages are never persisted to the conversation table. Only metadata (counts/tokens) are updated.

### 3. Channel registration is a stub
**`crates/talon-gateway/src/grpc_service.rs:142-157`** - `register_channel` just logs and returns an empty token. No state stored, no PASETO token issued, no validation.

### 4. Heartbeat is a no-op
**`crates/talon-gateway/src/grpc_service.rs:159-167`** - Returns `ok: true` without checking anything.

### 5. Agents are never used for routing
Agents can be created/listed/deleted, but:
- **`crates/talon-gateway/src/db/session_repo.rs:119`** - `agent_id` always `None` in sessions
- **`crates/talon-gateway/src/inference.rs:110-115`** - Agent's provider/model/system_prompt never applied
- Single global `ActonAI` instance used for all requests regardless of agent config

---

## Critical: Security Gaps

### 6. No authentication on any endpoint
- **`crates/talon-gateway/src/routes.rs`** - All routes unprotected (tenant CRUD, chat, sessions)
- **`crates/talon-admin/src/routes.rs`** - Admin dashboard open to anyone
- **`crates/talon-admin/src/api_client.rs:24-29`** - No auth headers on admin-to-gateway calls

### 7. No tenant isolation enforcement
- **`crates/talon-gateway/src/handlers.rs:15`** - Tenant ID comes from untrusted request body, defaults to `"default"`
- Anyone can impersonate any tenant or sender

### 8. No CSRF protection on admin forms
- **`crates/talon-admin/src/handlers/tenants.rs:93-118`** - POST forms lack CSRF tokens

---

## High: Dead Code / Never Called

### 9. Audit repo never invoked
**`crates/talon-gateway/src/db/audit_repo.rs`** - Complete BLAKE3 hash-chain implementation with tests, but `append_audit_event`, `list_audit_events`, and `verify_tenant_audit_chain` are never called from any code path.

### 10. Usage tracking never invoked
**`crates/talon-gateway/src/db/usage_repo.rs:46-86`** - `increment_usage` and `get_usage` defined but never called. Usage table created but never populated.

### 11. Conversation table unused
**`crates/talon-gateway/src/db/schema.rs:48`** - Table defined in schema. `ConversationMessage` type defined in `talon-types/src/session.rs:45-57`. Neither written to nor read.

### 12. Attachment type unused
**`crates/talon-types/src/messages.rs:31-35`** - `Attachment` struct defined, `InboundMessage.attachments` field exists, but nothing ever populates it.

---

## High: Error Information Discarded

### 13. Gateway handlers lose all error context
**`crates/talon-gateway/src/handlers.rs`** - 12 instances of `.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)` that discard the actual error. No logging before returning generic 500.

### 14. Admin silently swallows failures
- **`crates/talon-admin/src/handlers/dashboard.rs:22-23`** - `.unwrap_or_default()` returns empty lists when API calls fail
- **`crates/talon-admin/src/handlers/tenants.rs:55,84`** - Same pattern
- **`crates/talon-admin/src/handlers/agents.rs:63`** - Same pattern
- Dashboard shows "0 tenants" when gateway is down with no error indication

---

## Medium: Hardcoded Values

### 15. Inconsistent gateway URL defaults
| Service | Default URL |
|---------|-------------|
| telegram, discord, slack, web | `http://127.0.0.1:8080` |
| **admin** | **`http://localhost:3000`** |

**`crates/talon-admin/src/routes.rs:16`** - Admin defaults to wrong port.

### 16. Circuit breaker not configurable
**`crates/talon-gateway/src/inference.rs:39-43`** - Hardcoded: 5 failures, 30s reset, 2 success threshold.

### 17. Rate limiter not configurable
**`crates/talon-gateway/src/routes.rs`** - Hardcoded: 30 req/min, burst of 5.

### 18. Discord registration URL is bogus
**`crates/talon-discord/src/main.rs:26`** - Registers with `"http://localhost:0"`.

---

## Medium: Race Conditions

### 19. Session creation race condition
**`crates/talon-gateway/src/db/session_repo.rs:87-145`** - Separate SELECT then CREATE. Two concurrent requests for the same session key create duplicates.

### 20. Audit chain race condition
**`crates/talon-gateway/src/db/audit_repo.rs:94-156`** - Reads latest event then inserts. Concurrent appends could corrupt sequence numbers and hash chain.

---

## Medium: Missing Operational Features

### 21. No graceful shutdown
Gateway, admin, and web services all just `serve().await?` with no signal handling.

### 22. No inference timeout
**`crates/talon-gateway/src/handlers.rs:33-36`** - LLM calls can hang indefinitely. No per-request deadline.

### 23. No request ID propagation
No correlation ID for tracing requests across gateway -> channel -> inference.

---

## Low: Data Handling Quirks

### 24. TrustTier saturates to Full
**`crates/talon-types/src/trust.rs:87-98`** - `From<u8>` for values >4 silently maps to `TrustTier::Full` (highest access).

### 25. Invalid dates replaced with now
**`crates/talon-gateway/src/db/tenant_repo.rs:57-58`**, **`session_repo.rs:69-70`**, **`agent_repo.rs:65-66`** - Parse failures silently replaced with `Utc::now()`.

### 26. SurrealDB namespace interpolation
**`crates/talon-gateway/src/db/agent_repo.rs`** etc. - Tenant namespace is string-interpolated into queries (`format!("USE NS \`{tenant_ns}\` DB main;")`). Backtick-escaped but not validated as a safe identifier.

### 27. `#[allow(dead_code)]` on DB row structs
**`audit_repo.rs:29`**, **`session_repo.rs:28`**, **`tenant_repo.rs:28`**, **`agent_repo.rs:30`**, **`usage_repo.rs:13`** - The `id` field on SurrealDB row types is allowed dead. These are populated by deserialization but never read.
