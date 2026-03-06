# talon-web Channel Service Implementation Plan

## Overview

Implement the `talon-web` crate as a standalone HTTP+WebSocket channel service
that connects to the Talon gateway via gRPC using the `talon-channel-sdk`.

## Architecture

```
Web Client (browser/curl)
    |
    v
talon-web (HTTP+WS on port 8081)
    |  POST /api/v1/chat         -> sync chat
    |  POST /api/v1/chat/stream  -> SSE streaming
    |  GET  /api/v1/ws/chat      -> WebSocket bidirectional
    |
    v (gRPC)
talon-gateway (port 8080)
```

## Files to Create/Modify

### 1. `crates/talon-web/Cargo.toml` (modify)

Dependencies:
- `talon-types` (workspace) - ChatRequest, ChatResponse types
- `talon-channel-sdk` (workspace) - GatewayClient for gRPC
- `acton-service` with features: http, websocket, sse, observability
- `tokio`, `serde`, `serde_json`, `tracing`, `futures`, `anyhow` (workspace)
- `tokio-stream` 0.1 - for ReceiverStream in SSE handler

### 2. `crates/talon-web/src/main.rs` (create)

Entry point:
- Read `TALON_GATEWAY_URL`, `TALON_TENANT_ID`, `TALON_WEB_PORT` from env
- Connect `GatewayClient` to gateway
- Build routes via `routes::build_routes()`
- Configure `acton-service` `ServiceBuilder` with port override
- Serve

### 3. `crates/talon-web/src/routes.rs` (create)

- `WebState` struct holding `Arc<Mutex<GatewayClient>>`
- `build_routes()` returning `VersionedRoutes`
- Uses `VersionedApiBuilder` with `Extension(WebState)` layer
  (because `add_version` closure receives `Router<AppState<()>>`)

### 4. `crates/talon-web/src/handlers.rs` (create)

- `chat()` handler - POST, sync request/response
- `chat_stream()` handler - POST, SSE streaming response
- Both extract `Extension<WebState>` and `Json<ChatRequest>`

### 5. `crates/talon-web/src/ws.rs` (create)

- `ws_handler()` - WebSocket upgrade handler
- `handle_socket()` - bidirectional message loop
- Local `WsInbound`/`WsOutbound` structs for WS JSON protocol

## Key Design Decisions

1. **GatewayClient is Clone** - it wraps a tonic Channel which is Clone.
   Use `Arc<Mutex<GatewayClient>>` since `send_message` takes `&mut self`.

2. **Extension layer** - The `VersionedApiBuilder::add_version` closure
   receives `Router<AppState<()>>`, so we cannot use custom state directly.
   Use axum's `Extension` layer to inject `WebState`.

3. **SSE streaming** - Spawn a tokio task that locks the gateway client,
   calls `send_message_streaming`, and forwards chunks through an mpsc channel
   wrapped in `ReceiverStream` for the `Sse` response.

4. **WsMessage::Text(Utf8Bytes)** - axum 0.8 uses `Utf8Bytes` not `String`.
   Use `.into()` for String->Utf8Bytes conversion. For receiving, use
   `.to_string()` or `.as_str()` depending on what Utf8Bytes provides.

5. **Proto StreamChunk** - The gRPC `StreamChunk` has a `chunk` oneof field.
   Map each variant to a JSON SSE event for the web client.

## Semver

This is a new crate with no existing public API. The workspace version is 0.1.0
which is appropriate for initial development. No version bump needed.

## Test Strategy

Compilation check via `cargo check -p talon-web`. Integration tests would
require a running gateway, so are out of scope for this initial implementation.
