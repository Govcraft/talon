# Plan: Simplify Conversation Management with acton-ai 0.25 Conversation API

## Summary

Replace the manual history management in Talon's `ConversationActor` with acton-ai 0.25's `Conversation` handle. The new `Conversation` type is `Clone + Send + 'static`, backed by its own internal actor that serializes history mutations through a mailbox. This eliminates the need for manual `Vec<Message>` tracking, `AppendAssistantMessage` internal messages, and per-turn `SaveMessage` calls.

Additionally, acton-ai 0.25 makes `ActonAI` cheaply cloneable (`Arc<Inner>`), removing the need for `Arc<ActonAI>` wrappers throughout the codebase.

## Semver Recommendation

**Minor (0.3.0)**: This is a backwards-compatible internal refactoring. The IPC protocol, public message types (`ConversationResponse`, `ConversationUserMessage`, `CorrelationId`), and runtime configuration remain unchanged. Internal types (`SetupConversation`, `ConversationActor` fields) change but they are implementation details within `talon-core`.

## Changes by File

### 1. `crates/talon-core/src/conversation/messages.rs`

**Current state**: `SetupConversation` carries `Arc<ActonAI>`, `ActorHandle` (store), `system_prompt`, `channel_id`.

**Change**: Replace `SetupConversation` with `SetupConversation` that carries a pre-built `Conversation` handle instead of raw resources. The Router will build the `Conversation` before sending setup.

```rust
// BEFORE
#[acton_message]
pub struct SetupConversation {
    pub acton_ai: Arc<ActonAI>,
    pub store: ActorHandle,
    pub system_prompt: Option<String>,
    pub channel_id: ChannelId,
}

// AFTER
use acton_ai::prelude::Conversation;

#[acton_message]
pub struct SetupConversation {
    /// Pre-built acton-ai Conversation handle (Clone + Send + 'static)
    pub conversation: Conversation,
    /// Channel this conversation belongs to
    pub channel_id: ChannelId,
}
```

Remove the `Arc<ActonAI>` import since it is no longer needed in this file.

### 2. `crates/talon-core/src/conversation/actor.rs`

This is the primary simplification target.

#### Actor State

```rust
// BEFORE: 8 fields
#[acton_actor]
pub struct ConversationActor {
    id: ConversationId,
    sender: Option<SenderId>,
    channel_id: ChannelId,
    acton_ai: Option<Arc<ActonAI>>,
    store: Option<ActorHandle>,
    history: Vec<Message>,
    system_prompt: Option<String>,
    store_conversation_id: Option<acton_ai::prelude::ConversationId>,
    turn_count: usize,
}

// AFTER: 5 fields (3 removed entirely, 3 consolidated into 1)
#[acton_actor]
pub struct ConversationActor {
    id: ConversationId,
    sender: Option<SenderId>,
    channel_id: ChannelId,
    /// acton-ai Conversation handle that manages history internally
    conversation: Option<Conversation>,
    turn_count: usize,
}
```

**Removed fields**:
- `acton_ai: Option<Arc<ActonAI>>` - replaced by `Conversation` handle
- `store: Option<ActorHandle>` - no longer needed for per-turn persistence
- `history: Vec<Message>` - managed by `Conversation` internally
- `system_prompt: Option<String>` - set during `Conversation::build()`
- `store_conversation_id: Option<acton_ai::prelude::ConversationId>` - no longer needed

**Added field**:
- `conversation: Option<Conversation>` - the acton-ai 0.25 handle

#### Accessor Methods

```rust
impl ConversationActor {
    pub fn id(&self) -> &ConversationId { &self.id }
    pub fn sender(&self) -> Option<&SenderId> { self.sender.as_ref() }
    pub fn turn_count(&self) -> usize { self.turn_count }

    // CHANGED: delegate to Conversation handle
    pub fn history(&self) -> Vec<Message> {
        self.conversation
            .as_ref()
            .map(|c| c.history())
            .unwrap_or_default()
    }
}
```

Note: `history()` now returns `Vec<Message>` (owned) instead of `&[Message]` (borrowed), since it gets a snapshot from the `Conversation` handle via its `watch::Receiver`.

#### Handler Changes

**SetupConversation handler** (simplified):
```rust
builder.mutate_on::<SetupConversation>(|actor, ctx| {
    let msg = ctx.message().clone();
    actor.model.conversation = Some(msg.conversation);
    actor.model.channel_id = msg.channel_id;

    debug!(
        conversation_id = %actor.model.id,
        "conversation actor initialized"
    );

    Reply::ready()
});
```

No more `StoreCreateConversation` envelope or `tokio::spawn` block. No more `Reply::pending`.

**Remove StoreConversationCreated handler entirely** - no longer needed.

**ConversationUserMessage handler** (simplified):
```rust
builder.mutate_on::<ConversationUserMessage>(|actor, ctx| {
    let msg = ctx.message().clone();
    let reply_envelope = ctx.reply_envelope();

    // Set sender on first message
    if actor.model.sender.is_none() {
        actor.model.sender = Some(msg.sender.clone());
    }

    actor.model.turn_count += 1;

    let conversation = actor.model.conversation.clone();
    let conversation_id = actor.model.id.clone();
    let correlation_id = msg.correlation_id.clone();

    let handle = tokio::spawn(async move {
        let Some(conv) = conversation else {
            warn!(
                conversation_id = %conversation_id,
                "no Conversation configured, echoing message"
            );
            let echo_content = format!("Echo (no AI): {}", msg.content);
            reply_envelope
                .send(ConversationResponse {
                    correlation_id,
                    content: echo_content,
                })
                .await;
            return;
        };

        match conv.send(&msg.content).await {
            Ok(response) => {
                debug!(
                    conversation_id = %conversation_id,
                    response_len = response.text.len(),
                    tool_calls = response.tool_calls.len(),
                    "LLM response received"
                );

                let response_text = if response.text.is_empty() {
                    warn!("LLM returned empty response, using fallback");
                    "I received your message but couldn't generate a response. Please try again.".to_string()
                } else {
                    response.text
                };

                reply_envelope
                    .send(ConversationResponse {
                        correlation_id,
                        content: response_text,
                    })
                    .await;
            }
            Err(e) => {
                error!(
                    conversation_id = %conversation_id,
                    error = %e,
                    "ActonAI error"
                );
                reply_envelope
                    .send(ConversationResponse {
                        correlation_id,
                        content: format!("LLM error: {e}"),
                    })
                    .await;
            }
        }
    });

    Reply::pending(async move {
        let _ = handle.await;
    })
});
```

**Key simplifications**:
- No manual `history.push(user_message)` - `conv.send()` does this
- No `ai.continue_with(history)` - `conv.send()` handles it
- No `AppendAssistantMessage` self-message - `conv.send()` appends assistant reply
- No `SaveMessage` to store - removed per-turn persistence
- No `system_prompt` threading - set at build time

**Remove AppendAssistantMessage handler and struct entirely**.

**EndConversation handler** (simplified):
```rust
builder.mutate_on::<EndConversation>(|actor, _ctx| {
    debug!(
        conversation_id = %actor.model.id,
        turns = actor.model.turn_count,
        "ending conversation"
    );
    if let Some(conv) = &actor.model.conversation {
        conv.clear();
    }
    actor.model.conversation = None;
    Reply::ready()
});
```

#### Imports

Remove:
```rust
use std::sync::Arc;
use acton_ai::memory::{ConversationCreated as StoreConversationCreated, CreateConversation as StoreCreateConversation, SaveMessage};
```

Add:
```rust
use acton_ai::prelude::Conversation;
```

### 3. `crates/talon-core/src/conversation/mod.rs`

No changes needed. The public exports (`ConversationActor`, `spawn_conversation`, `ConversationResponse`, `ConversationUserMessage`, `EndConversation`, `SetupConversation`) remain the same.

### 4. `crates/talon-core/src/router/actor.rs`

#### State Changes

```rust
// BEFORE
acton_ai: Option<Arc<ActonAI>>,
store_handle: Option<ActorHandle>,

// AFTER
acton_ai: Option<ActonAI>,
store_handle: Option<ActorHandle>,  // kept for future persistence use
```

Remove `Arc<ActonAI>` wrapper since `ActonAI` is now `Clone` natively.

#### SetupRouter Message

```rust
// BEFORE
#[acton_message]
pub struct SetupRouter {
    pub acton_ai: Arc<ActonAI>,
    pub store: ActorHandle,
    pub skill_registry: Arc<RwLock<SecureSkillRegistry>>,
}

// AFTER
#[acton_message]
pub struct SetupRouter {
    pub acton_ai: ActonAI,
    pub store: ActorHandle,
    pub skill_registry: Arc<RwLock<SecureSkillRegistry>>,
}
```

#### RouteMessage Handler

The handler's async block changes from sending `SetupConversation` with `Arc<ActonAI>` + `store` to building a `Conversation` handle and sending it.

```rust
// Inside the tokio::spawn block, when creating a new conversation:
let new_handle = spawn_conversation(&mut runtime, &conversation_id).await;

// Build acton-ai Conversation handle
let conv = ai.conversation()
    .system("You are a helpful AI assistant.")
    .build()
    .await;

new_handle
    .send(SetupConversation {
        conversation: conv,
        channel_id,
    })
    .await;
```

The `acton_ai` clone changes from `Arc::clone` to a simple `.clone()`.

#### Import Changes

Remove `use std::sync::Arc` for ActonAI (keep for other uses like `DashMap`, `RwLock`).
Remove `use acton_ai::prelude::*` wildcard if it was pulling in unnecessary types, or keep it and just adjust usage.

#### Tests

The existing Router tests (unit tests on `Router` struct methods) remain unchanged since they test the `Router` model directly and don't involve `ActonAI` or `SetupConversation`.

### 5. `crates/talon-core/src/runtime.rs`

#### ActonAI Wrapping

```rust
// BEFORE
let acton_ai = Arc::new(acton_ai);
// ...
acton_ai: Arc::clone(&acton_ai),

// AFTER
// No Arc wrapping needed, ActonAI is Clone via internal Arc
// ...
acton_ai: acton_ai.clone(),
```

#### TalonRuntime Struct

```rust
// BEFORE
acton_ai: Arc<ActonAI>,

// AFTER
acton_ai: ActonAI,
```

#### SetupRouter Send

```rust
// BEFORE
router_handle.send(SetupRouter {
    acton_ai: Arc::clone(&acton_ai),
    store: store_handle.clone(),
    skill_registry: Arc::clone(&skill_registry),
}).await;

// AFTER
router_handle.send(SetupRouter {
    acton_ai: acton_ai.clone(),
    store: store_handle.clone(),
    skill_registry: Arc::clone(&skill_registry),
}).await;
```

#### Accessor

```rust
// BEFORE
pub fn acton_ai(&self) -> &Arc<ActonAI> { &self.acton_ai }

// AFTER
pub fn acton_ai(&self) -> &ActonAI { &self.acton_ai }
```

## What Gets Removed

| Item | File | Reason |
|------|------|--------|
| `AppendAssistantMessage` struct + handler | `conversation/actor.rs` | `Conversation::send()` auto-appends assistant messages |
| `StoreConversationCreated` handler | `conversation/actor.rs` | No more MemoryStore conversation record creation |
| `SaveMessage` sends (2x) | `conversation/actor.rs` | Per-turn persistence removed |
| `StoreCreateConversation` send | `conversation/actor.rs` | No more MemoryStore conversation setup |
| `history: Vec<Message>` field | `conversation/actor.rs` | Managed by `Conversation` handle |
| `store: Option<ActorHandle>` field | `conversation/actor.rs` | Not needed for per-turn saves |
| `system_prompt: Option<String>` field | `conversation/actor.rs` | Set at `Conversation::build()` time |
| `store_conversation_id` field | `conversation/actor.rs` | No more MemoryStore conversation tracking |
| `acton_ai: Option<Arc<ActonAI>>` field | `conversation/actor.rs` | Replaced by `Conversation` handle |
| `Arc<ActonAI>` wrapping | `runtime.rs` | `ActonAI` is now `Clone` natively |

## What Gets Kept

| Item | Reason |
|------|--------|
| `ConversationResponse` + `CorrelationId` | IPC response correlation pattern |
| Router's `pending_responses` DashMap | IPC handler waits on oneshot |
| Echo fallback for no AI | Testing and fallback behavior |
| `MemoryStore` actor + `store_handle` | Still needed for cross-restart persistence |
| All existing Router unit tests | They test `Router` model methods, unaffected |
| All existing IPC handler tests | They don't touch conversation internals |

## Test Strategy

### Existing Tests (Update)

1. **Router unit tests** (`router/actor.rs`): No changes needed - they test `Router` struct methods directly.
2. **IPC handler tests** (`ipc/handlers.rs`): No changes needed - they test auth and message routing at the IPC level.
3. **Runtime creation test** (`runtime.rs`): Verify the `acton_ai` field type change compiles and the test still passes.

### Verification

After implementation:
1. `cargo check` - Compilation verification
2. `cargo clippy -- -D warnings` - Lint compliance
3. `cargo nextest run` - All tests pass

## Implementation Order

1. **messages.rs** - Update `SetupConversation` struct (foundational change)
2. **actor.rs** - Simplify `ConversationActor` and `spawn_conversation` (biggest change)
3. **mod.rs** - Update exports if needed
4. **router/actor.rs** - Update `SetupRouter`, Router state, and `RouteMessage` handler
5. **runtime.rs** - Remove `Arc<ActonAI>` wrapping, update `TalonRuntime` struct
6. Run quality gates: `cargo check && cargo clippy -- -D warnings && cargo nextest run`
