# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

**Talon** is a secure multi-channel AI assistant built on the acton-ai framework. It addresses the security vulnerabilities found in tools like OpenClaw by adding cryptographic attestation to skills using agent-uri concepts.

### Key Security Features

- **Agent-URI Identity**: Skills identified by `agent://trust-root/capability/agent-id` URIs
- **PASETO Attestation**: Ed25519-signed tokens verify skill authenticity
- **OmniBOR Integrity**: Content-addressable gitoid hashes verify skill content
- **Graduated Trust**: 5 trust tiers (0-4) based on capability risk
- **Hyperlight Sandboxing**: Scripts execute in isolated WebAssembly environment

### Architecture

```
talon-core (library)           - Shared business logic, agent-uri integration
talon-cli (binary)             - Terminal interface using acton-ai
talon-telegram (binary)        - Telegram bot channel
talon-discord (binary)         - Discord bot channel  
talon-hub (binary)             - Skill registry HTTP service (proprietary)
```

Communication between channels and core uses acton-reactive IPC over Unix Domain Sockets.

## Documentation

- **Architecture Design**: `docs/design/2026-02-03-talon-architecture.md`
- **Implementation Plans**: `docs/plans/` (created as work progresses)

## Build & Test Commands

```bash
cargo check                    # Fast compilation check
cargo nextest run              # Run all tests
cargo clippy -- -D warnings    # Lint with zero tolerance
cargo run --bin talon-cli      # Run terminal interface
cargo run --bin talon-hub      # Run skill registry
```

## Dependencies

### Core Framework
- `acton-ai` with `agent-skills` feature - AI agent framework with skill support
- `acton-reactive` with `ipc` feature - Actor framework with IPC support
- `acton-service` - HTTP service framework (for talon-hub)

### Identity & Security
- `agent-uri` - URI parsing and types
- `agent-uri-attestation` - PASETO attestation verification
- `omnibor` - Content integrity verification

### Channels
- `teloxide` - Telegram bot framework
- `serenity` - Discord bot framework

## Code Conventions

- All IDs use newtypes from `mti` crate with `MagicTypeId`
- Custom error types (no anyhow/thiserror)
- Pure functions preferred; side effects at boundaries
- Never suppress clippy lints - fix the underlying issue
- Use Conventional Commits for all commit messages
- Sign all commits with `-S` flag

## Key Types

```rust
// Verified skill with attestation
pub struct VerifiedSkill {
    pub skill: LoadedSkill,           // From acton-ai agent-skills
    pub agent_uri: AgentUri,          // Primary identity
    pub attestation: AttestationClaims,
    pub omnibor_id: ArtifactId,       // Integrity verification
    pub capabilities: Vec<CapabilityPath>,
}

// Secure registry wrapping acton-ai SkillRegistry
pub struct SecureSkillRegistry {
    inner: SkillRegistry,             // From acton-ai
    verifier: Verifier,               // From agent-uri-attestation
    cache: AttestationCache,
    registry_client: HttpRegistryClient,
}
```

## Trust Tiers

| Tier | Risk Level | Capabilities | Verification |
|------|------------|--------------|--------------|
| 0 | None | Read-only, no network | Signed manifest |
| 1 | Low | Local file read, limited network | + Publisher attestation |
| 2 | Medium | File write, full network | + Code review attestation |
| 3 | High | Shell execution (sandboxed) | + Security audit attestation |
| 4 | Critical | System modification | + User explicit approval per-use |

## Licensing

- `talon-core`, `talon-cli`, `talon-telegram`, `talon-discord`: MIT (open source)
- `talon-hub`: Proprietary (hosted service)

## Development Approach
- Always use the /rust-planner skill to plan out implementations and write to a new plan file.- Always use the /rust-author skill to do the actual Rust implementation passing the plan file as an input.
