//! Core trait and types for inference hooks.

use async_trait::async_trait;
use serde_json::Value;

use crate::HookError;

/// Phase of the inference pipeline where a hook executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    /// Before sending the prompt to the LLM.
    PreInference,
    /// When a tool call is about to be executed.
    ToolCall,
    /// After receiving the LLM response.
    PostInference,
}

impl std::fmt::Display for HookPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::PreInference => "pre-inference",
            Self::ToolCall => "tool-call",
            Self::PostInference => "post-inference",
        };
        write!(f, "{label}")
    }
}

/// Result returned by a hook after processing a message.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// The hook modified the content; continue with the updated value.
    Continue(String),
    /// The hook determined the message should not proceed.
    Block {
        /// Why the message was blocked.
        reason: String,
    },
    /// The hook requires explicit human approval before proceeding.
    RequireApproval {
        /// Why approval is needed.
        reason: String,
    },
    /// The hook found nothing to change; continue with the original content.
    Pass,
}

/// Mutable context threaded through all hooks in a pipeline phase.
///
/// Hooks read and optionally modify the `content` field.  The `metadata`
/// field carries arbitrary JSON that hooks can use to communicate
/// out-of-band information (e.g. token counts, PII findings).
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Tenant that owns this session.
    pub tenant_id: String,
    /// Platform-specific sender identifier.
    pub sender_id: String,
    /// Session identifier for the conversation.
    pub session_id: String,
    /// The message content being processed.  Hooks may mutate this.
    pub content: String,
    /// Which phase this context is executing in.
    pub phase: HookPhase,
    /// Arbitrary metadata for inter-hook communication.
    pub metadata: Value,
}

impl HookContext {
    /// Create a new context for the given phase and content.
    pub fn new(
        tenant_id: impl Into<String>,
        sender_id: impl Into<String>,
        session_id: impl Into<String>,
        content: impl Into<String>,
        phase: HookPhase,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            sender_id: sender_id.into(),
            session_id: session_id.into(),
            content: content.into(),
            phase,
            metadata: Value::Object(serde_json::Map::new()),
        }
    }
}

/// A single hook in the inference pipeline.
///
/// Hooks are stateless processors that inspect or transform message content
/// at a specific phase of the inference lifecycle.  They declare a unique
/// `id`, the `phase` they belong to, and a numeric `priority` that controls
/// execution order within that phase (lower values run first).
#[async_trait]
pub trait InferenceHook: Send + Sync + 'static {
    /// Unique identifier for this hook (e.g. `"input_sanitizer"`).
    fn id(&self) -> &str;

    /// The pipeline phase this hook participates in.
    fn phase(&self) -> HookPhase;

    /// Execution priority within the phase.  Lower values run first.
    fn priority(&self) -> u32;

    /// Process the context, returning a [`HookResult`] that tells the
    /// pipeline how to proceed.
    async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError>;
}
