# Phase 5: Authorization + Audit Plan

## Summary

Add Cedar policy-based authorization, trust tier types, and audit logging to the Talon gateway. This phase integrates acton-service's built-in Cedar middleware and audit framework to provide:

1. A `TrustTier` domain type in `talon-types`
2. Cedar policy file for route-level authorization
3. Audit logging on tenant, agent, and chat operations via `AuditLogger`
4. Tenant-scoped audit event storage in SurrealDB
5. Configuration updates for `[cedar]` and `[audit]` sections

## Existing Patterns

- `acton_service::prelude::*` shadows `Result` -- use `std::result::Result<T, E>` explicitly
- `AppState::audit_logger()` returns `Option<&AuditLogger>` -- may be `None` if audit not configured
- `AuditLogger::log_custom(name, severity, Option<Value>)` is fire-and-forget
- `CedarAuthz::from_app_config(&config)` returns `Result<Option<CedarAuthz>>`
- `ServiceBuilder::new().with_cedar(cedar)` applies Cedar as middleware
- SurrealDB queries use `USE NS ... DB ...;` prefix pattern per existing repos
- `GatewayError` implements `IntoResponse` for HTTP error handling
- All IDs use newtype wrappers in `talon-types/src/ids.rs`

## Files to Create

### 1. `talon-types/src/trust.rs` -- TrustTier enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum TrustTier {
    Untrusted = 0,
    Basic = 1,
    Standard = 2,
    Elevated = 3,
    Full = 4,
}
```

With `Display`, `FromStr`, `Default` (defaults to `Untrusted`), and `From<u8>` / `Into<u8>` conversions.

### 2. `policies/talon.cedar` -- Cedar policy file

Basic policies covering:
- Authenticated users can chat
- Admins can manage tenants
- Tenant owners can manage their agents
- Trust tier requirements for tool access

### 3. `talon-gateway/src/audit.rs` -- Audit helper module

Helper functions that wrap `AuditLogger::log_custom()` for domain-specific events:
- `log_tenant_event(logger, kind, tenant_id, metadata)`
- `log_agent_event(logger, kind, tenant_id, agent_id, metadata)`
- `log_chat_event(logger, tenant_id, sender_id, model)`

Each function takes `Option<&AuditLogger>` so it gracefully no-ops when audit is disabled.

### 4. `talon-gateway/src/db/audit_repo.rs` -- Tenant-scoped audit storage

SurrealDB repository for writing audit events into tenant namespace `audit_event` table, with BLAKE3 hash chain for tamper detection.

## Files to Modify

### 5. `talon-types/src/lib.rs` -- Export trust module

Add `pub mod trust;` and `pub use trust::*;`

### 6. `talon-types/src/agent.rs` -- Use TrustTier instead of u8

Change `trust_tier: u8` to `trust_tier: TrustTier` in `Agent`, `CreateAgentRequest`, `UpdateAgentRequest`.

### 7. `talon-gateway/src/db/schema.rs` -- Add audit_event table

Add `DEFINE TABLE IF NOT EXISTS audit_event SCHEMALESS;` and index to `init_tenant_schema`.

### 8. `talon-gateway/src/db/mod.rs` -- Register audit_repo

Add `pub mod audit_repo;`

### 9. `talon-gateway/src/db/agent_repo.rs` -- Update trust_tier field handling

Change `trust_tier: u8` to `TrustTier` in `AgentRecord` / `AgentRow` and conversion logic.

### 10. `talon-gateway/src/tenant_handlers.rs` -- Add audit logging

Log `tenant.created`, `tenant.updated`, `tenant.deleted` events.

### 11. `talon-gateway/src/agent_handlers.rs` -- Add audit logging

Log `agent.created`, `agent.updated`, `agent.deleted` events.

### 12. `talon-gateway/src/handlers.rs` -- Add chat audit logging

Log `chat.request` events.

### 13. `talon-gateway/src/lib.rs` -- Register audit module

Add `pub mod audit;`

### 14. `config.toml` -- Add cedar and audit sections

### 15. `talon-gateway/src/main.rs` -- No changes needed

ServiceBuilder auto-configures audit from `[audit]` section. Cedar starts disabled.

## Semver Recommendation

**Minor (0.2.0)** -- New functionality (trust tiers, audit logging, Cedar policies) that is backwards compatible. The `TrustTier` replacing `u8` in the Agent type is a breaking API change in `talon-types`, but since we are pre-1.0, a minor bump is appropriate per semver conventions.

## Test Strategy

- Unit tests for `TrustTier` conversions and ordering in `talon-types`
- Unit tests for audit helper function behavior with `None` logger
- Compilation validation via `cargo check --workspace` and `cargo clippy --workspace -- -D warnings`
