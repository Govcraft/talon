# Phase 6: Inference Hooks Plan

## Overview

Implement the `talon-inference` crate providing a hook-based pipeline for pre/post-inference processing. This is pure domain logic with no framework dependencies.

## Architecture

```
InboundMessage
    |
    v
[Pre-Inference Hooks] (sorted by priority, lower = first)
  1. InputSanitizer  (p=10) - strip HTML/script tags
  2. PiiDetector     (p=20) - regex-based PII detection
    |
    v
[ActonAI Prompt Execution]   <- external, not part of this crate
    |
    v
[Post-Inference Hooks] (sorted by priority, lower = first)
  1. UsageTracker    (p=90) - check/update token counters
    |
    v
OutboundMessage (safe, audited, compliant)
```

## Files to Create/Modify

### Modified Files

1. **`Cargo.toml`** (workspace root) - Add `regex` workspace dependency
2. **`crates/talon-inference/Cargo.toml`** - Add all dependencies

### New Files

3. **`crates/talon-inference/src/lib.rs`** - Module declarations and re-exports
4. **`crates/talon-inference/src/error.rs`** - `HookError` and `PipelineError` types
5. **`crates/talon-inference/src/hook.rs`** - `InferenceHook` trait, `HookPhase`, `HookResult`, `HookContext`
6. **`crates/talon-inference/src/pipeline.rs`** - `HookPipeline` executor
7. **`crates/talon-inference/src/hooks/mod.rs`** - Built-in hooks module
8. **`crates/talon-inference/src/hooks/input_sanitizer.rs`** - HTML/script stripping
9. **`crates/talon-inference/src/hooks/pii_detector.rs`** - PII detection with configurable action
10. **`crates/talon-inference/src/hooks/usage_tracker.rs`** - Token budget enforcement

## Type Definitions

### HookPhase
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPhase {
    PreInference,
    ToolCall,
    PostInference,
}
```

### HookResult
```rust
#[derive(Debug, Clone)]
pub enum HookResult {
    Continue(String),
    Block { reason: String },
    RequireApproval { reason: String },
    Pass,
}
```

### HookContext
Uses `talon-types` ID newtypes (TenantId, SenderId, SessionId) for type safety.

### Error Types
- `HookError` - Individual hook failures
- `PipelineError` - Pipeline-level errors (blocked, approval required, hook errors)

Both use `thiserror` following the workspace pattern established in `talon-types`.

## Test Strategy

1. Pipeline ordering - hooks run sorted by priority within phase
2. Pipeline blocking - Block result halts pipeline
3. Pipeline content modification - Continue result updates ctx.content
4. Pipeline pass-through - Pass result leaves content unchanged
5. PII detection - detects emails, phone numbers, SSNs
6. PII redaction - replaces detected PII with [REDACTED]
7. PII blocking - blocks messages containing PII
8. Input sanitizer - strips script tags
9. Input sanitizer - strips HTML tags
10. Input sanitizer - preserves plain text
11. Usage tracker - passes under budget
12. Usage tracker - blocks over budget

## Semver

**Minor bump (0.1.0 -> 0.2.0)**: New public API surface (traits, types, pipeline). However since workspace version is shared and this is the first real content in the crate, staying at 0.1.0 is appropriate since no consumers exist yet.

**Recommendation: No version bump needed** - this is initial implementation of an empty stub crate.
