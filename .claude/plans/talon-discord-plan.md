# Implementation Plan: talon-discord

## Overview

Implement the `talon-discord` crate -- a Discord channel adapter binary that bridges
Discord messages to the Talon AI gateway via gRPC using `talon-channel-sdk`.

## Codebase Context

### Existing Patterns (from talon-telegram)
- Binary crate with `main.rs` + `bot.rs` + `lib.rs`
- `GatewayClient::connect(url, channel_id, tenant_id)` returns a client
- Client wrapped in `Arc<Mutex<GatewayClient>>` for shared concurrent access
- `send_message(sender_id, text, system_prompt)` returns `SendMessageResponse` with `.text` field
- Sender ID format: `{channel_prefix}:{platform_user_id}` (e.g. `tg:123456`)

### GatewayClient API (from talon-channel-sdk/src/client.rs)
- `connect(gateway_url, channel_id, tenant_id)` -- creates and connects
- `register(service_url)` -- registers channel with gateway
- `send_message(sender_id, text, system_prompt)` -- unary request/response
- `send_message_streaming(sender_id, text, system_prompt)` -- streaming response
- `heartbeat()` -- health check
- `get_session(sender_id)` -- session state query

### Proto Module Structure (from talon-channel-sdk/src/proto.rs)
- `talon_channel_sdk::proto::common::SessionKey` -- but not used directly; GatewayClient handles this internally
- `talon_channel_sdk::proto::gateway::SendMessageResponse` -- has `.text` field

## Files to Create/Modify

### 1. `crates/talon-discord/Cargo.toml` (MODIFY)
Replace the empty stub with full dependencies.

Dependencies:
- `talon-types` (workspace) -- shared types
- `talon-channel-sdk` (workspace) -- gateway gRPC client
- `serenity = "0.12"` (local, NOT workspace) -- Discord API
- `tokio` (workspace) -- async runtime
- `tracing` (workspace) -- logging
- `tracing-subscriber` (workspace) -- log formatting
- `anyhow` (workspace) -- error handling in binary

Note: `anyhow` is acceptable in binary crates for top-level error handling.
Note: serenity features: `client`, `gateway`, `model`, `rustls_backend`.

### 2. `crates/talon-discord/src/lib.rs` (MODIFY)
Module declarations only.

### 3. `crates/talon-discord/src/main.rs` (CREATE)
Binary entry point:
1. Initialize tracing with `EnvFilter`
2. Read env vars: `TALON_GATEWAY_URL`, `TALON_TENANT_ID`, `DISCORD_TOKEN`
3. Connect `GatewayClient` to gateway
4. Register channel
5. Create serenity `Client` with handler
6. Start the Discord bot

### 4. `crates/talon-discord/src/bot.rs` (CREATE)
Serenity `EventHandler` implementation:
- `Handler` struct holding `Arc<Mutex<GatewayClient>>` and `tenant_id`
- `message()` handler: ignores bots, sends to gateway, replies to Discord
- `ready()` handler: logs connection
- Sender ID format: `dc:{discord_user_id}`

## Design Decisions

1. **No custom error type** -- This is a binary crate. `anyhow::Result` at the top level
   is the established pattern (matching talon-telegram).

2. **No ChannelHandler trait impl** -- Following the telegram pattern, the bot module
   directly uses GatewayClient rather than implementing the ChannelHandler trait.
   The trait exists for future SDK-driven channel orchestration but is not used by
   the current channel binaries.

3. **Arc<Mutex<GatewayClient>>** -- Same pattern as telegram. GatewayClient is Clone
   but the serenity EventHandler trait requires a single struct instance, and multiple
   message handlers may run concurrently, so Mutex serializes gateway access.

4. **serenity 0.12** -- Current stable release with async/await support.
   Features: `client`, `gateway`, `model`, `rustls_backend` (no openssl dependency).

## Implementation Notes

- The `GatewayClient::connect` takes 3 args: `(url, channel_id, tenant_id)`. The user's
  spec shows a separate `register()` call, but the actual SDK has `register(service_url)`.
  We will call `register` after connect, matching the registration pattern.

- serenity's `EventHandler` requires `#[async_trait]` which is re-exported by serenity itself.

- Discord intents needed: `GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT`.

## Verification

1. `cargo check -p talon-discord` -- must pass
2. `cargo clippy -p talon-discord -- -D warnings` -- must pass clean

## Semver

**Patch (0.1.0 -> 0.1.0)**: No version bump needed. This is a new binary crate being
implemented from an empty stub. The workspace version remains 0.1.0 as this is
pre-release development adding a new workspace member.
