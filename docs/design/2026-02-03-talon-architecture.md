# Talon: Secure Multi-Channel AI Assistant

> Design Document - February 3, 2026
> Status: APPROVED

## Executive Summary

Talon is a secure multi-channel AI assistant that addresses the security vulnerabilities in OpenClaw-style assistants. Built on acton-ai (actor model, Hyperlight sandboxing, skill registry), agent-uri (cryptographic identity and attestation), and the agent-skills format (plain-text skill definitions).

**Key differentiators:**
- Skills require cryptographic attestation before execution
- OmniBOR-based integrity verification prevents tampering
- Graduated trust tiers match capability to risk level
- Defense-in-depth: even attested skills run scripts in Hyperlight sandbox
- User-controlled trust roots for enterprise/personal skills

**Licensing:**
- All crates: MIT OR Apache-2.0 (open-source)
- TalonHub hosted service: Proprietary

**Target channels (MVP):**
- Terminal (CLI)
- Telegram
- Discord

---

## 1. Architecture Overview

```
+-------------------------------------------------------------------+
|                     talon-core (daemon)                           |
|  +-------------+  +-------------+  +-------------+                |
|  |   Router    |  |    LLM      |  |  Secure     |                |
|  |   Actor     |  |  Providers  |  |  Skill Reg  |                |
|  +-------------+  +-------------+  +-------------+                |
|                         |                                         |
|               Unix Domain Socket                                  |
|              ~/.local/run/talon.sock                              |
|                         |                                         |
+-------------------------+-----------------------------------------+
                          |
        +-----------------+-----------------+
        |                 |                 |
        v                 v                 v
  +-----------+    +-----------+    +-----------+
  |  talon    |    |  talon    |    |  talon    |
  |  terminal |    |  telegram |    |  discord  |
  |  (CLI)    |    |  (daemon) |    |  (daemon) |
  +-----------+    +-----------+    +-----------+
```

### Core Principles

1. **Skills are untrusted by default** - Every skill requires attestation before execution
2. **Graduated trust** - Sandboxed-only skills need less attestation than file/network-access skills
3. **Defense-in-depth** - Even attested skills run scripts in Hyperlight sandbox
4. **User-controlled trust** - Users can add private trust roots for enterprise/personal skills

### IPC-Based Process Model

| Process | Role | Lifecycle |
|---------|------|-----------|
| talon-core | Daemon, always running | systemd/launchd managed |
| talon terminal | Interactive CLI | Started by user, connects to core |
| talon telegram | Telegram bot daemon | systemd managed |
| talon discord | Discord bot daemon | systemd managed |

**Benefits of IPC architecture:**
- Single LLM pool shared across all channels
- Hot-reload channels without affecting core
- Resource efficiency (one skill cache, one memory store)
- Process isolation (channel crash does not take down core)
- Multi-user ready (multiple CLI sessions)

### Crate Structure

| Crate | Purpose | License |
|-------|---------|---------|
| talon-core | Router, conversation actors, SecureSkillRegistry, IPC | MIT/Apache-2.0 |
| talon-channels | Terminal, Telegram, Discord adapters | MIT/Apache-2.0 |
| talon-registry | HTTP registry implementing Dht trait (TalonHub backend) | MIT/Apache-2.0 |
| talon-cli | User-facing CLI application | MIT/Apache-2.0 |

**Note:** No separate talon-skills crate needed - we extend acton-ai's existing `agent-skills` feature with attestation verification.

### Foundational Dependencies

| Crate | Version | Source | Purpose |
|-------|---------|--------|---------|
| acton-ai | 0.24.0+ | local | Actor model, LLM providers, Hyperlight sandbox, SkillRegistry |
| acton-reactive | 7.1.0 | crates.io | Actor system + IPC via UDS |
| acton-service | 0.15.0 | crates.io | TalonHub HTTP backend |
| agent-uri | latest | local | URI parsing, capability paths, trust roots |
| agent-uri-attestation | latest | local | PASETO v4.public tokens, verification |
| agent-uri-dht | latest | local | Dht trait, key derivation |
| agent-skills | 0.2.0 | crates.io | Skill format parsing (used by acton-ai) |
| omnibor | 0.10.0 | crates.io | Skill integrity (gitoid) |
| teloxide | 0.17.0 | crates.io | Telegram bot API |
| serenity | 0.12.5 | crates.io | Discord bot API |
| crossterm | 0.29.0 | crates.io | Terminal I/O |
| ratatui | 0.30.0 | crates.io | Terminal UI |

---

## 2. Security Model

### Threat Model

| Threat | OpenClaw Vulnerability | Talon Mitigation |
|--------|------------------------|------------------|
| Malicious skill impersonation | None - anyone can publish | Attestation binds skill identity to trust root |
| Skill tampering | None - no integrity checks | OmniBOR ID verified before loading |
| Prompt injection in SKILL.md | Full local access | Capability-gated tool access |
| Malicious scripts | Run as user | Hyperlight sandbox isolation |
| API key exfiltration | Skills can read env vars | Sandboxed scripts cannot access env |
| Over-permissioned skills | No enforcement | allowed-tools mapped to capability attestation |

### Trust Tiers (Graduated Trust Model)

```
+-------------------------------------------------------------------+
| Tier 0: Sandboxed-only (no attestation required)                  |
|   - Pure computation, no tool access                              |
|   - Example: calculator, text formatter                           |
|   - Runs in Hyperlight, cannot escape                             |
+-------------------------------------------------------------------+
| Tier 1: Read-only tools (attestation required)                    |
|   - allowed-tools: Read, Glob, Grep                               |
|   - Can read files but not modify                                 |
|   - Capability: skill/tools/filesystem/read                       |
+-------------------------------------------------------------------+
| Tier 2: Write tools (attestation + user approval)                 |
|   - allowed-tools: Write, Edit                                    |
|   - Can modify files                                              |
|   - Capability: skill/tools/filesystem/write                      |
+-------------------------------------------------------------------+
| Tier 3: Execution tools (attestation + explicit grant)            |
|   - allowed-tools: Bash(git:*), Bash(npm:*)                       |
|   - Scoped command execution                                      |
|   - Capability: skill/tools/exec/git, skill/tools/exec/npm        |
+-------------------------------------------------------------------+
| Tier 4: Network access (highest scrutiny)                         |
|   - allowed-tools: WebFetch, HTTP                                 |
|   - Potential exfiltration vector                                 |
|   - Capability: skill/tools/network/http                          |
|   - Scripts ALWAYS sandboxed, network calls logged                |
+-------------------------------------------------------------------+
```

### Verification Flow (Every Skill Load)

1. Parse skill's agent-uri identity from SKILL.md frontmatter
2. Fetch attestation from registry (or local cache)
3. Verify PASETO signature against trust root's public key
4. Check attestation not expired
5. Compute OmniBOR ID of loaded skill files
6. Compare computed vs attested OmniBOR ID (integrity check)
7. Check skill's allowed-tools covered by attested capabilities
8. If scripts present, prepare Hyperlight sandbox

---

## 3. Skill Format and OmniBOR Integration

### Identity Model

- **Primary identity**: Agent-URI (agent://talonhub.io/skill/git/skill_01jx7...)
- **Integrity verification**: OmniBOR gitoid (gitoid:blob:sha256:...)
- Agent-URI is stable across versions; OmniBOR ID changes with content

### Extended Agent-Skills Format

```yaml
# SKILL.md frontmatter
---
name: git-assistant
description: Git operations helper - commit, branch, merge, rebase workflows.
license: MIT
compatibility: Requires git CLI

# Security fields (Talon extensions)
agent-uri: agent://talonhub.io/skill/git/skill_01jx7...
allowed-tools: Bash(git:*) Read Glob
trust-tier: 3

metadata:
  author: talonhub
  version: "1.2.0"
---

[Skill instructions in markdown...]
```

### OmniBOR Manifest Generation

```
git-assistant/
├── SKILL.md              -> gitoid:blob:sha256:a1b2c3...
├── scripts/
│   ├── commit-helper.sh  -> gitoid:blob:sha256:d4e5f6...
│   └── branch-util.py    -> gitoid:blob:sha256:789abc...
└── references/
    └── git-workflow.md   -> gitoid:blob:sha256:def012...
              |
         Input Manifest   -> gitoid:blob:sha256:fedcba...
              |
    Skill OmniBOR ID = gitoid:blob:sha256:fedcba...
```

### Attestation Payload (PASETO v4.public)

```json
{
  "sub": "agent://talonhub.io/skill/git/skill_01jx7...",
  "iss": "talonhub.io",
  "iat": "2026-02-03T00:00:00Z",
  "exp": "2026-05-03T00:00:00Z",
  "capabilities": [
    "skill/tools/exec/git",
    "skill/tools/filesystem/read"
  ],
  "omnibor_id": "gitoid:blob:sha256:fedcba...",
  "version": "1.2.0"
}
```

### Capability Mapping

| allowed-tools | Capability Path |
|---------------|-----------------|
| Read | skill/tools/filesystem/read |
| Write, Edit | skill/tools/filesystem/write |
| Glob, Grep | skill/tools/filesystem/read |
| Bash(git:*) | skill/tools/exec/git |
| Bash(npm:*) | skill/tools/exec/npm |
| Bash(*) | skill/tools/exec/any (highest tier) |
| WebFetch | skill/tools/network/http |

### SecureSkillRegistry (extends acton-ai)

```rust
use acton_ai::skills::{SkillRegistry, LoadedSkill};
use agent_uri::AgentUri;
use agent_uri_attestation::{Verifier, AttestationClaims};
use omnibor::ArtifactId;

/// Extends LoadedSkill with security verification
pub struct VerifiedSkill {
    pub skill: LoadedSkill,          // From acton-ai
    pub agent_uri: AgentUri,
    pub attestation: AttestationClaims,
    pub omnibor_id: ArtifactId,
    pub capabilities: Vec<CapabilityPath>,
}

/// Wraps SkillRegistry with attestation verification
pub struct SecureSkillRegistry {
    inner: SkillRegistry,            // From acton-ai
    verifier: Verifier,              // From agent-uri-attestation
    cache: AttestationCache,
    registry_client: HttpRegistryClient,
}

impl SecureSkillRegistry {
    /// Load and verify a skill (attestation + OmniBOR)
    pub async fn load_verified(&self, name: &str) -> Result<VerifiedSkill, SkillSecurityError>;
    
    /// Check if tool invocation is allowed for this skill
    pub fn check_capability(&self, skill: &VerifiedSkill, tool: &str) -> bool;
    
    /// Compute OmniBOR ID for a skill directory
    pub fn compute_omnibor_id(&self, path: &Path) -> Result<ArtifactId, OmniborError>;
}
```

---

## 4. Channel Architecture

### Channel Trait

```rust
/// Core channel trait - implement for each platform
#[async_trait]
pub trait Channel: Send + Sync {
    /// Unique channel identifier (e.g., "telegram", "discord")
    fn id(&self) -> &str;
    
    /// Start receiving messages
    async fn start(&self, sender: mpsc::Sender<InboundMessage>) -> Result<(), ChannelError>;
    
    /// Send a message to a conversation
    async fn send(&self, message: OutboundMessage) -> Result<(), ChannelError>;
    
    /// Stop the channel
    async fn stop(&self) -> Result<(), ChannelError>;
}
```

### Channel Implementations

| Channel | Crate feature | Dependencies |
|---------|---------------|--------------|
| Terminal | channel-terminal | crossterm 0.29, ratatui 0.30 |
| Telegram | channel-telegram | teloxide 0.17 |
| Discord | channel-discord | serenity 0.12 |

### IPC Communication (acton-reactive)

Channels communicate with talon-core via Unix Domain Sockets using acton-reactive's IPC feature:

```rust
// In talon-core: expose actors for IPC
runtime.ipc_registry().register::<ChannelInbound>("ChannelInbound");
runtime.ipc_registry().register::<ChannelOutbound>("ChannelOutbound");
runtime.ipc_registry().register::<StreamToken>("StreamToken");

runtime.ipc_expose("router", router_handle);
runtime.ipc_expose("skills", skill_registry_handle);

// Start IPC listener
let config = IpcConfig::new("talon");  // ~/.local/run/talon.sock
let listener = runtime.start_ipc_listener().await?;
```

---

## 5. TalonHub Registry (using acton-service)

### Features

```toml
[dependencies]
acton-service = { version = "0.15", features = [
    "http",           # HTTP server
    "database",       # PostgreSQL for skills/attestations
    "cache",          # Redis for attestation caching
    "observability",  # Tracing
    "otel-metrics",   # Metrics
    "openapi",        # API documentation
    "governor",       # Rate limiting
    "pagination-full",# Skill listing
    "jwt",            # Publisher authentication
    "auth",           # Password hashing
] }
```

### REST API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| /api/v1/skills | GET | List/search skills with filtering |
| /api/v1/skills | POST | Register new skill (requires attestation) |
| /api/v1/skills/{uri} | GET | Get skill details + attestation |
| /api/v1/skills/{uri} | DELETE | Deregister skill |
| /api/v1/skills/{uri}/attestation | GET | Get current attestation |
| /api/v1/skills/{uri}/download | GET | Download skill archive |
| /api/v1/discover/exact | GET | Dht lookup_exact |
| /api/v1/discover/prefix | GET | Dht lookup_prefix |
| /api/v1/discover/global | GET | Dht lookup_global |
| /api/v1/trust-roots | GET | List known trust roots |
| /api/v1/trust-roots/{domain}/keys | GET | Get trust root public keys |
| /api/v1/publishers/register | POST | Register as publisher |
| /api/v1/publishers/me | GET | Get publisher profile |

### Database Schema

```sql
-- Publishers (skill authors)
CREATE TABLE publishers (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    public_key BYTEA NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    verified_at TIMESTAMPTZ
);

-- Skills
CREATE TABLE skills (
    id BIGSERIAL PRIMARY KEY,
    agent_uri TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    publisher_id BIGINT REFERENCES publishers(id),
    omnibor_id TEXT NOT NULL,
    allowed_tools TEXT[] NOT NULL,
    trust_tier INT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Attestations
CREATE TABLE attestations (
    id BIGSERIAL PRIMARY KEY,
    skill_id BIGINT REFERENCES skills(id) ON DELETE CASCADE,
    token TEXT NOT NULL,
    capabilities TEXT[] NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    omnibor_id TEXT NOT NULL,
    version TEXT NOT NULL
);

-- Skill content (binary storage)
CREATE TABLE skill_archives (
    omnibor_id TEXT PRIMARY KEY,
    archive BYTEA NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Trust Root Key Publication

```
GET https://talonhub.io/.well-known/agent-keys.json

{
  "trust_root": "talonhub.io",
  "keys": [{
    "kid": "key-2026-01",
    "algorithm": "Ed25519",
    "public_key": "<base64-encoded>",
    "not_before": "2026-01-01T00:00:00Z",
    "not_after": "2027-01-01T00:00:00Z"
  }]
}
```

---

## 6. Message Flow and Conversation Model

### End-to-End Flow

1. User sends message via channel
2. Channel adapter receives message, creates ChannelInbound
3. Router actor receives IpcEnvelope<ChannelInbound> via UDS
4. Router looks up or creates Conversation actor
5. Conversation appends to history, builds LLMRequest
6. LLM Provider streams response (tokens + tool calls)
7. If tool_use: SecureSkillRegistry verifies attestation + OmniBOR, executes in Hyperlight sandbox
8. Response flows back via IPC to channel to user

### IPC Message Types

```rust
/// Inbound from channel to core
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message]
pub struct ChannelInbound {
    pub channel_id: String,
    pub conversation_id: String,
    pub sender: SenderInfo,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
}

/// Outbound from core to channel
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message]
pub struct ChannelOutbound {
    pub conversation_id: String,
    pub content: MessageContent,
    pub reply_to: Option<String>,
}

/// Streaming token for real-time output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message]
pub struct StreamToken {
    pub conversation_id: String,
    pub token: String,
    pub is_final: bool,
}

/// Skill invocation request
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message]
pub struct SkillInvoke {
    pub skill_uri: AgentUri,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub correlation_id: CorrelationId,
}

/// Skill execution result
#[derive(Clone, Debug, Serialize, Deserialize)]
#[acton_message]
pub struct SkillResult {
    pub correlation_id: CorrelationId,
    pub result: Result<serde_json::Value, String>,
    pub execution_time_ms: u64,
}
```

### Conversation Actor State

```rust
#[acton_actor]
pub struct Conversation {
    pub id: ConversationId,
    pub channel_id: String,
    pub history: Vec<Message>,
    pub active_skills: Vec<VerifiedSkill>,
    pub trust_config: UserTrustConfig,
    pub pending_requests: HashMap<CorrelationId, PendingRequest>,
    pub stream_buffer: Option<StreamAccumulator>,
}
```

---

## 7. Implementation Plan

### Phase 1: Foundation (Weeks 1-2)

| Task | Crate | Depends on |
|------|-------|------------|
| Create workspace structure | talon (root) | - |
| Implement OmniBOR skill verification | talon-core | omnibor 0.10 |
| Extend agent-uri-attestation with omnibor_id claim | agent-uri-attestation | - |
| HTTP registry client (Dht trait impl) | talon-core | agent-uri-dht |
| SecureSkillRegistry wrapping acton-ai SkillRegistry | talon-core | acton-ai (agent-skills) |

### Phase 2: Core Runtime (Weeks 3-4)

| Task | Crate | Depends on |
|------|-------|------------|
| Router actor with IPC exposure | talon-core | acton-reactive 7.1 (ipc) |
| Conversation actor with verified skill execution | talon-core | Phase 1 |
| Capability enforcement in tool execution | talon-core | Phase 1 |
| IPC message types and serialization | talon-core | - |

### Phase 3: Channels (Weeks 5-6)

| Task | Crate | Depends on |
|------|-------|------------|
| Channel trait definition | talon-channels | - |
| Terminal channel (ratatui) | talon-channels | crossterm 0.29, ratatui 0.30 |
| Telegram channel | talon-channels | teloxide 0.17 |
| Discord channel | talon-channels | serenity 0.12 |
| CLI with subcommands | talon-cli | talon-channels, talon-core |

### Phase 4: TalonHub Registry (Weeks 7-8)

| Task | Crate | Depends on |
|------|-------|------------|
| REST API with acton-service | talon-registry | acton-service 0.15 |
| Publisher registration/auth | talon-registry | - |
| Skill upload with OmniBOR verification | talon-registry | omnibor 0.10 |
| Attestation issuance | talon-registry | agent-uri-attestation |
| Trust root key publication (.well-known) | talon-registry | - |

### Phase 5: Integration and Hardening (Weeks 9-10)

| Task | Crate | Depends on |
|------|-------|------------|
| End-to-end integration tests | all | all |
| Hyperlight sandbox integration for skill scripts | talon-core | acton-ai (hyperlight) |
| Rate limiting and abuse prevention | talon-registry | - |
| Documentation and examples | all | - |
| Example skills with attestations (3+) | - | Phase 4 |

### MVP Deliverables

1. talon-core daemon with IPC
2. talon terminal CLI for local use
3. talon telegram bot daemon
4. talon discord bot daemon
5. TalonHub registry (self-hostable)
6. At least 3 example skills with attestations

---

## Appendix A: OpenClaw Feature Comparison

| OpenClaw Feature | Talon Equivalent | Security Improvement |
|------------------|------------------|---------------------|
| Multi-channel (7+) | Terminal, Telegram, Discord (MVP) | Same |
| Pi-embedded agent | acton-ai actors | Actor isolation |
| Skill plugins | agent-skills + attestation | Cryptographic verification |
| ClawHub registry | TalonHub + Dht trait | Federated trust roots |
| TOML config | TOML config | Same |
| Session files (JSONL) | Turso/libSQL | Same |
| Bash tool execution | Hyperlight sandbox | Hardware isolation |
| Prompt injection | Capability-gated tools | Reduced attack surface |

## Appendix B: Future Enhancements

- Additional channels: Slack, WhatsApp, Signal
- libp2p Kademlia DHT backend for true decentralization
- Voice integration
- Canvas/interactive UI rendering
- Enterprise SSO integration
- Skill certification program
