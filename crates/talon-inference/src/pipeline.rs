//! Hook pipeline that orchestrates hook execution across inference phases.

use tracing::{debug, info, warn};

use crate::{HookContext, HookPhase, HookResult, InferenceHook, PipelineError};

/// Ordered collection of hooks that processes messages through the
/// inference lifecycle.
///
/// Hooks are registered once, then the pipeline is invoked per-message.
/// Within each phase, hooks execute in ascending priority order (lower
/// values first).  A [`HookResult::Block`] or [`HookResult::RequireApproval`]
/// from any hook short-circuits the remaining hooks in that phase.
pub struct HookPipeline {
    hooks: Vec<Box<dyn InferenceHook>>,
}

impl Default for HookPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl HookPipeline {
    /// Create an empty pipeline with no registered hooks.
    #[tracing::instrument]
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Register a hook.  Hooks are sorted by priority at execution time,
    /// so registration order does not matter.
    #[tracing::instrument(skip(self, hook))]
    pub fn register(&mut self, hook: impl InferenceHook) {
        debug!(hook_id = hook.id(), phase = %hook.phase(), priority = hook.priority(), "registered hook");
        self.hooks.push(Box::new(hook));
    }

    /// Run all pre-inference hooks against the context.
    #[tracing::instrument(skip(self, ctx))]
    pub async fn run_pre_inference(&self, ctx: &mut HookContext) -> Result<(), PipelineError> {
        self.run_phase(HookPhase::PreInference, ctx).await
    }

    /// Run all post-inference hooks against the context.
    #[tracing::instrument(skip(self, ctx))]
    pub async fn run_post_inference(&self, ctx: &mut HookContext) -> Result<(), PipelineError> {
        self.run_phase(HookPhase::PostInference, ctx).await
    }

    /// Run all tool-call hooks against the context.
    #[tracing::instrument(skip(self, ctx))]
    pub async fn run_tool_call(&self, ctx: &mut HookContext) -> Result<(), PipelineError> {
        self.run_phase(HookPhase::ToolCall, ctx).await
    }

    /// Returns the number of registered hooks.
    #[tracing::instrument(skip(self))]
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }

    /// Execute all hooks that match the given phase, in priority order.
    async fn run_phase(
        &self,
        phase: HookPhase,
        ctx: &mut HookContext,
    ) -> Result<(), PipelineError> {
        // Collect indices of hooks matching this phase, sorted by priority.
        let mut phase_indices: Vec<usize> = self
            .hooks
            .iter()
            .enumerate()
            .filter(|(_, h)| h.phase() == phase)
            .map(|(i, _)| i)
            .collect();

        phase_indices.sort_by_key(|&i| self.hooks[i].priority());

        info!(phase = %phase, hook_count = phase_indices.len(), "running pipeline phase");

        for idx in phase_indices {
            let hook = &self.hooks[idx];
            let hook_id = hook.id().to_string();

            debug!(hook_id = %hook_id, priority = hook.priority(), "executing hook");

            let result = hook.execute(ctx).await?;

            match result {
                HookResult::Pass => {
                    debug!(hook_id = %hook_id, "hook passed (no changes)");
                }
                HookResult::Continue(new_content) => {
                    debug!(hook_id = %hook_id, "hook modified content");
                    ctx.content = new_content;
                }
                HookResult::Block { reason } => {
                    warn!(hook_id = %hook_id, reason = %reason, "hook blocked message");
                    return Err(PipelineError::Blocked { hook_id, reason });
                }
                HookResult::RequireApproval { reason } => {
                    warn!(hook_id = %hook_id, reason = %reason, "hook requires approval");
                    return Err(PipelineError::ApprovalRequired { hook_id, reason });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HookError;
    use async_trait::async_trait;

    // ---------- test helpers ----------

    /// A hook that records its execution order via an atomic counter and
    /// returns Pass.
    struct OrderTracker {
        hook_id: String,
        hook_phase: HookPhase,
        hook_priority: u32,
        /// Shared counter; each execution bumps it and records its value
        /// into the context metadata under `"order_<id>"`.
        counter: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    #[async_trait]
    impl InferenceHook for OrderTracker {
        fn id(&self) -> &str {
            &self.hook_id
        }
        fn phase(&self) -> HookPhase {
            self.hook_phase
        }
        fn priority(&self) -> u32 {
            self.hook_priority
        }
        async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError> {
            let order = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let key = format!("order_{}", self.hook_id);
            ctx.metadata[&key] = serde_json::json!(order);
            Ok(HookResult::Pass)
        }
    }

    /// A hook that unconditionally blocks.
    struct Blocker {
        hook_phase: HookPhase,
        hook_priority: u32,
    }

    #[async_trait]
    impl InferenceHook for Blocker {
        fn id(&self) -> &str {
            "blocker"
        }
        fn phase(&self) -> HookPhase {
            self.hook_phase
        }
        fn priority(&self) -> u32 {
            self.hook_priority
        }
        async fn execute(&self, _ctx: &mut HookContext) -> Result<HookResult, HookError> {
            Ok(HookResult::Block {
                reason: "blocked for testing".into(),
            })
        }
    }

    /// A hook that uppercases the content.
    struct Uppercaser {
        hook_phase: HookPhase,
    }

    #[async_trait]
    impl InferenceHook for Uppercaser {
        fn id(&self) -> &str {
            "uppercaser"
        }
        fn phase(&self) -> HookPhase {
            self.hook_phase
        }
        fn priority(&self) -> u32 {
            50
        }
        async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError> {
            Ok(HookResult::Continue(ctx.content.to_uppercase()))
        }
    }

    fn test_ctx(phase: HookPhase) -> HookContext {
        HookContext::new("tenant_1", "sender_1", "session_1", "hello world", phase)
    }

    // ---------- tests ----------

    #[tokio::test]
    async fn hooks_run_in_priority_order() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut pipeline = HookPipeline::new();
        pipeline.register(OrderTracker {
            hook_id: "c".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 30,
            counter: counter.clone(),
        });
        pipeline.register(OrderTracker {
            hook_id: "a".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 10,
            counter: counter.clone(),
        });
        pipeline.register(OrderTracker {
            hook_id: "b".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 20,
            counter: counter.clone(),
        });

        let mut ctx = test_ctx(HookPhase::PreInference);
        pipeline.run_pre_inference(&mut ctx).await.unwrap();

        assert_eq!(
            ctx.metadata["order_a"], 0,
            "a (priority 10) should run first"
        );
        assert_eq!(
            ctx.metadata["order_b"], 1,
            "b (priority 20) should run second"
        );
        assert_eq!(
            ctx.metadata["order_c"], 2,
            "c (priority 30) should run third"
        );
    }

    #[tokio::test]
    async fn block_stops_pipeline() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut pipeline = HookPipeline::new();
        pipeline.register(OrderTracker {
            hook_id: "before".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 10,
            counter: counter.clone(),
        });
        pipeline.register(Blocker {
            hook_phase: HookPhase::PreInference,
            hook_priority: 20,
        });
        pipeline.register(OrderTracker {
            hook_id: "after".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 30,
            counter: counter.clone(),
        });

        let mut ctx = test_ctx(HookPhase::PreInference);
        let err = pipeline.run_pre_inference(&mut ctx).await.unwrap_err();

        assert!(matches!(err, PipelineError::Blocked { .. }));
        // "before" ran, "after" did not.
        assert!(ctx.metadata.get("order_before").is_some());
        assert!(ctx.metadata.get("order_after").is_none());
    }

    #[tokio::test]
    async fn continue_updates_content() {
        let mut pipeline = HookPipeline::new();
        pipeline.register(Uppercaser {
            hook_phase: HookPhase::PreInference,
        });

        let mut ctx = test_ctx(HookPhase::PreInference);
        pipeline.run_pre_inference(&mut ctx).await.unwrap();

        assert_eq!(ctx.content, "HELLO WORLD");
    }

    #[tokio::test]
    async fn pass_leaves_content_unchanged() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut pipeline = HookPipeline::new();
        pipeline.register(OrderTracker {
            hook_id: "noop".into(),
            hook_phase: HookPhase::PostInference,
            hook_priority: 10,
            counter,
        });

        let mut ctx = test_ctx(HookPhase::PostInference);
        pipeline.run_post_inference(&mut ctx).await.unwrap();

        assert_eq!(ctx.content, "hello world");
    }

    #[tokio::test]
    async fn hooks_only_run_for_matching_phase() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        let mut pipeline = HookPipeline::new();
        pipeline.register(OrderTracker {
            hook_id: "pre".into(),
            hook_phase: HookPhase::PreInference,
            hook_priority: 10,
            counter: counter.clone(),
        });
        pipeline.register(OrderTracker {
            hook_id: "post".into(),
            hook_phase: HookPhase::PostInference,
            hook_priority: 10,
            counter: counter.clone(),
        });

        let mut ctx = test_ctx(HookPhase::PreInference);
        pipeline.run_pre_inference(&mut ctx).await.unwrap();

        // Only the pre-inference hook should have run.
        assert!(ctx.metadata.get("order_pre").is_some());
        assert!(ctx.metadata.get("order_post").is_none());
    }

    #[tokio::test]
    async fn empty_pipeline_is_noop() {
        let pipeline = HookPipeline::new();
        let mut ctx = test_ctx(HookPhase::PreInference);
        pipeline.run_pre_inference(&mut ctx).await.unwrap();
        assert_eq!(ctx.content, "hello world");
    }
}
