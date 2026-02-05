# Talon

**Secure Multi-Channel AI Assistant**

Talon is a security-first AI assistant that addresses critical vulnerabilities in skill-based AI systems like OpenClaw. Built on [acton-ai](https://github.com/Govcraft/acton-ai) with cryptographic attestation via [agent-uri](https://github.com/Govcraft/agent-uri).

## Why Talon?

Modern AI assistants with plugin/skill systems face serious security challenges: **anyone can publish a skill, and that skill runs with your permissions**. Talon solves this with cryptographically verified skills, graduated trust tiers, and defense-in-depth sandboxing.

## Security: Talon vs OpenClaw

| Threat Vector | OpenClaw | Talon | Impact |
|--------------|----------|-------|--------|
| **Skill Identity** | Anyone can publish with any name | Skills require PASETO attestation signed by trust root | Prevents impersonation attacks |
| **Skill Integrity** | No verification of skill content | OmniBOR gitoid hash verified before loading | Detects tampering/MITM attacks |
| **Prompt Injection** | Full local file/network access | Capability-gated tools based on attestation | Limits blast radius |
| **Script Execution** | Runs as user with full permissions | Hyperlight WebAssembly sandbox | Hardware-level isolation |
| **API Key Theft** | Skills can read environment variables | Sandboxed scripts cannot access env | Protects credentials |
| **Over-Permissioned Skills** | No enforcement of declared permissions | `allowed-tools` verified against attestation | Enforces least privilege |
| **Trust Model** | Implicit trust - install and hope | Explicit trust roots + graduated tiers | User controls trust decisions |
| **Registry Security** | Centralized, single point of failure | Federated trust roots via DHT trait | Decentralized verification |

## Key Security Features

### Cryptographic Attestation

Every skill must be attested by a trust root before execution:

```yaml
# SKILL.md frontmatter
---
name: git-assistant
agent-uri: agent://talonhub.io/skill/git/skill_01jx7...
allowed-tools: Bash(git:*) Read Glob
trust-tier: 3
---
```

The attestation (PASETO v4.public token) binds:
- Skill identity (agent-uri)
- Allowed capabilities
- Content hash (OmniBOR ID)
- Expiration time

### Graduated Trust Tiers

| Tier | Risk Level | Capabilities | Verification Required |
|------|------------|--------------|----------------------|
| **0** | None | Pure computation, no tools | Signed manifest only |
| **1** | Low | Read-only filesystem (Read, Glob, Grep) | Publisher attestation |
| **2** | Medium | Write filesystem (Write, Edit) | + Code review attestation |
| **3** | High | Scoped execution (Bash with allowlist) | + Security audit attestation |
| **4** | Critical | Network access, system modification | + Explicit user approval per-use |

### Defense in Depth

Even attested skills run their scripts in a Hyperlight WebAssembly sandbox:

```
User Message
     │
     ▼
┌─────────────────┐
│  Attestation    │ ◄── PASETO signature verification
│  Verification   │ ◄── OmniBOR integrity check
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Capability    │ ◄── Tool access control
│   Enforcement   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Hyperlight   │ ◄── Hardware-isolated sandbox
│     Sandbox     │ ◄── No env/filesystem access
└─────────────────┘
```

## Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                     talon-core (daemon)                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐    │
│  │   Router    │  │    LLM      │  │  SecureSkillRegistry │    │
│  │   Actor     │  │  (Ollama)   │  │  (attestation+tools) │    │
│  └─────────────┘  └─────────────┘  └─────────────────────┘    │
│                          │                                     │
│               Unix Domain Socket                               │
│              /tmp/talon/talon.sock                            │
└───────────────────────────┬───────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
  ┌───────────┐      ┌───────────┐      ┌───────────┐
  │  talon    │      │  talon    │      │  talon    │
  │  terminal │      │  telegram │      │  discord  │
  └───────────┘      └───────────┘      └───────────┘
```

## Quick Start

### Prerequisites

- Rust 1.80+
- [Ollama](https://ollama.ai) with a model (e.g., `qwen2.5:7b`)

### Build

```bash
cargo build --release
```

### Run the Daemon

```bash
# Start core daemon (uses Ollama at localhost:11434)
./target/release/talon-daemon
```

### Connect via Telegram

```bash
# Store your Telegram bot token
sudo talon channel add telegram --token <your-bot-token>

# Run the Telegram bot
./target/release/talon-telegram
```

## Configuration

The daemon uses these defaults:

| Setting | Default | Description |
|---------|---------|-------------|
| IPC Socket | `/tmp/talon/talon.sock` | Unix domain socket for channel communication |
| Ollama Host | `http://localhost:11434/v1` | LLM provider URL (OpenAI-compatible) |
| Ollama Model | `qwen2.5:7b` | Default model for conversations |
| Max Conversations | 1000 | Concurrent conversation limit |

## Built-in Tools

Talon includes acton-ai's built-in tools:

| Tool | Description | Trust Tier |
|------|-------------|------------|
| `read_file` | Read file contents | 1 |
| `write_file` | Write to files | 2 |
| `edit_file` | Edit existing files | 2 |
| `glob` | Find files by pattern | 1 |
| `grep` | Search file contents | 1 |
| `bash` | Execute shell commands | 3 |
| `calculate` | Math evaluation | 0 |
| `web_fetch` | HTTP requests | 4 |

## Development

```bash
# Run tests
cargo nextest run

# Check lints
cargo clippy -- -D warnings

# Run daemon with debug logging
RUST_LOG=debug ./target/release/talon-daemon
```

## License

- **talon-core**, **talon-cli**, **talon-telegram**, **talon-discord**: MIT
- **talon-hub** (registry service): Proprietary

## Related Projects

- [acton-ai](https://github.com/Govcraft/acton-ai) - AI agent framework with Hyperlight sandboxing
- [agent-uri](https://github.com/Govcraft/agent-uri) - Cryptographic identity for AI agents
- [agent-uri-attestation](https://github.com/Govcraft/agent-uri-attestation) - PASETO-based attestation
- [omnibor](https://github.com/omnibor/omnibor-rs) - Content-addressable integrity verification
