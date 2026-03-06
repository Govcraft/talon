# talon-slack Implementation Plan

## Overview

Implement the `talon-slack` crate -- a standalone binary that bridges Slack with the Talon AI gateway. The crate uses slack-morphism's Socket Mode to receive Slack events via WebSocket, and forwards user messages to the Talon gateway via gRPC using `talon-channel-sdk::GatewayClient`.

## Architecture

```
Slack (WebSocket/Socket Mode)
    |
    v
talon-slack binary
    |  - Receives push events (message events)
    |  - Extracts text, sender, channel
    |  - Sends to gateway via gRPC
    |  - Sends gateway response back to Slack
    v
Talon Gateway (gRPC)
```

## Reference Pattern

Follows the same pattern as `talon-telegram`:
- `main.rs` reads env vars, creates clients, delegates to `bot::run()`
- `bot.rs` contains the event loop and message handling logic
- No lib.rs needed for binary crate (uses `mod bot;` in main.rs)

## Files

### 1. `crates/talon-slack/Cargo.toml`
### 2. `crates/talon-slack/src/main.rs`
### 3. `crates/talon-slack/src/bot.rs`

## Semver

Patch: 0.1.0 (initial implementation, workspace version unchanged)

## Verification

1. `cargo check -p talon-slack`
2. `cargo clippy -p talon-slack -- -D warnings`
