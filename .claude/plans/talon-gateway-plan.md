# Talon Gateway - Phase 1 Implementation Plan

## Overview

Implement the `talon-gateway` crate as the central AI inference gateway service for Talon.
This is the HTTP-facing service that accepts chat requests, manages sessions in memory,
and delegates inference to acton-ai's `ActonAI` facade.

## Architecture

```
HTTP Client --> acton-service routes --> handlers --> InferenceService (ActonAI)
                                    --> SessionStore (in-memory HashMap)
```

## Files to Create/Modify

### 1. `crates/talon-gateway/Cargo.toml` (modify)
- Add dependencies: talon-types, acton-service, acton-ai, tokio, serde, serde_json,
  tracing, chrono, ulid, thiserror, futures, anyhow, tokio-stream

### 2. `crates/talon-gateway/src/main.rs` (create)
- Binary entry point using `ServiceBuilder::new()` pattern
- Builds versioned routes, constructs ActonService, calls `.serve()`

### 3. `crates/talon-gateway/src/lib.rs` (modify)
- Module declarations: routes, handlers, session_store, inference

### 4. `crates/talon-gateway/src/routes.rs` (create)
- `build_routes()` function returning `VersionedRoutes`
- Uses `VersionedApiBuilder::new()` with base path `/api`
- V1 routes: POST /chat, POST /chat/stream, GET /sessions, GET /sessions/{id}

### 5. `crates/talon-gateway/src/handlers.rs` (create)
- `chat()` - synchronous chat handler
- `chat_stream()` - SSE streaming chat handler
- `list_sessions()` - list all sessions
- `get_session()` - get session by ID
- All handlers use `State<AppState>` from acton-service

### 6. `crates/talon-gateway/src/session_store.rs` (create)
- In-memory session store using `LazyLock<Arc<RwLock<HashMap>>>`
- Methods: get_or_create, get, list, record_exchange

### 7. `crates/talon-gateway/src/inference.rs` (create)
- Wraps `ActonAI` in a global singleton pattern
- Methods: prompt (synchronous collect), prompt_streaming (SSE)
- Uses `ActonAI::builder().from_config()?.with_builtins().launch()`

## Key Design Decisions

1. **Global singletons for Phase 1**: Both SessionStore and InferenceService use
   `LazyLock` global instances. Phase 4 will move these into AppState extensions.

2. **acton-service SSE types**: Use re-exported `Sse`, `SseEvent`, `KeepAlive` from
   `acton_service::prelude::*` for streaming responses.

3. **Handler signatures**: Use `State<AppState>` (no generic parameter needed for Phase 1
   since we use the default `T = ()`).

4. **Error handling**: Map all errors to `StatusCode` for now. Phase 2 will introduce
   proper error responses.

## Semver

This is a new crate at 0.1.0 (workspace version). No bump needed - initial implementation.

## Test Strategy

For Phase 1, the goal is compilation verification via `cargo check`. Integration tests
requiring a running LLM provider will be added in a later phase.
