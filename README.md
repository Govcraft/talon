# Talon

Multi-tenant, multi-channel AI assistant gateway written in Rust. Connect any messaging platform to any LLM backend through a centralized inference pipeline with trust tiers, rate limiting, and security controls.

## Features

- **Multi-channel** — Telegram, Discord, Slack, and Web (REST/SSE/WebSocket) out of the box
- **Multi-tenant** — isolated namespaces per tenant with per-tenant agents and sessions
- **Trust tiers** — 5-level capability hierarchy (Untrusted → Full) controlling agent tool access
- **Hook-based inference pipeline** — pluggable pre/post-inference hooks for input sanitization, PII detection, and usage tracking
- **Resilience** — circuit breaker, rate limiting (governor), and retry logic protecting the LLM backend
- **Admin dashboard** — server-rendered HTMX UI for managing tenants, agents, and sessions
- **gRPC + REST** — dual protocol support on the gateway; channel services communicate via gRPC

## Architecture

Talon is a Cargo workspace with 9 crates:

| Crate | Description |
|-------|-------------|
| `talon-gateway` | Central service — chat, sessions, tenants, agents (port 8080) |
| `talon-types` | Pure domain types — no framework dependencies |
| `talon-inference` | Hook-based inference pipeline (sanitizer, PII detector, usage tracker) |
| `talon-channel-sdk` | gRPC client SDK for channel services |
| `talon-web` | Web channel — REST, SSE, WebSocket (port 8081) |
| `talon-telegram` | Telegram bot channel (teloxide) |
| `talon-discord` | Discord bot channel (serenity) |
| `talon-slack` | Slack bot channel (slack-morphism, Socket Mode) |
| `talon-admin` | Admin dashboard — Askama + HTMX + Tailwind (port 8082) |

## Quick Start

```bash
cp .env.example .env
# Edit .env with your configuration

# Run with Docker
docker compose up -d

# Or run standalone
cargo run -p talon-gateway   # Gateway on :8080
cargo run -p talon-web       # Web channel on :8081
cargo run -p talon-admin     # Admin dashboard on :8082
```

## Prerequisites

- Rust 1.93+
- `protoc` (Protocol Buffers compiler)
- SurrealDB (optional — defaults to in-memory)

## Configuration

Gateway configuration via `config.toml` and environment variables. See `.env.example` for all options including bot tokens for Telegram, Discord, and Slack channels.

## Trust Tiers

| Tier | Level | Permissions |
|------|-------|-------------|
| Untrusted | 0 | No tool access |
| Basic | 1 | Read-only tools |
| Standard | 2 | Read + write tools |
| Elevated | 3 | System-level tools |
| Full | 4 | Unrestricted |

## Built With

[acton-service](https://github.com/Govcraft/acton-service) · [acton-ai](https://github.com/Govcraft/acton-ai) · [acton-reactive](https://github.com/Govcraft/acton-reactive) · SurrealDB · Axum · Tonic

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
