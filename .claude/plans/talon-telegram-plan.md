# talon-telegram Implementation Plan

## Overview

Implement the `talon-telegram` crate as a standalone Telegram bot binary that forwards messages to the Talon gateway via gRPC using `talon-channel-sdk::GatewayClient`.

## Design Decisions

### GatewayClient Sharing

`GatewayClient` is `Clone` (it wraps a `tonic` `Channel` which is internally reference-counted). However, teloxide's `dptree` dependency injection requires types to be `Send + Sync + 'static`. Since `GatewayClient` methods take `&mut self`, we need interior mutability. We will use `Arc<Mutex<GatewayClient>>` as the shared state passed to teloxide handlers.

**Update**: Looking more carefully at `GatewayClient`, the `&mut self` is only needed because `GatewayServiceClient` takes `&mut self` for each RPC call. However, tonic's `GatewayServiceClient<Channel>` is `Clone`, and each clone can make concurrent calls. So we can clone the `GatewayClient` for each request. But since teloxide injects a single shared state, `Arc<Mutex<GatewayClient>>` is the simplest pattern that works correctly with `dptree`.

### Message Flow

1. User sends Telegram message
2. teloxide dispatches to `handle_message`
3. Handler extracts text, constructs `sender_id` as `tg:{chat_id}`
4. Calls `gateway.send_message()` via gRPC
5. Sends response text back to Telegram chat

### Files

1. `crates/talon-telegram/Cargo.toml` - Dependencies
2. `crates/talon-telegram/src/main.rs` - Entry point with config from env
3. `crates/talon-telegram/src/bot.rs` - teloxide dispatcher and message handler

### Semver

This is a new feature in a pre-1.0 crate. Stays at `0.1.0` (workspace version).

## Checklist

- [x] Read existing codebase patterns (GatewayClient, ChannelHandler, proto types)
- [x] Plan includes file layout and purpose
- [x] Plan accounts for teloxide 0.13 dptree injection model
- [x] No custom error types needed (uses anyhow at binary level, ChannelError from SDK)
