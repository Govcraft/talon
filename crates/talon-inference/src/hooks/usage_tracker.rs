//! Usage tracker hook for per-session token budget enforcement.

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::{HookContext, HookError, HookPhase, HookResult, InferenceHook};

/// Post-inference hook that checks whether a session's cumulative token
/// usage has exceeded a configured budget.
///
/// The hook reads `token_count` and `total_tokens` from the context's
/// metadata.  If `total_tokens` exceeds `max_tokens_per_session`, the
/// message is blocked.
///
/// Execution order: priority 90 (runs late in the post-inference phase).
pub struct UsageTracker {
    max_tokens_per_session: u64,
}

impl UsageTracker {
    /// Create a tracker that blocks when total tokens exceed the given limit.
    pub fn new(max_tokens_per_session: u64) -> Self {
        Self {
            max_tokens_per_session,
        }
    }
}

#[async_trait]
impl InferenceHook for UsageTracker {
    fn id(&self) -> &str {
        "usage_tracker"
    }

    fn phase(&self) -> HookPhase {
        HookPhase::PostInference
    }

    fn priority(&self) -> u32 {
        90
    }

    #[tracing::instrument(skip(self, ctx))]
    async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError> {
        let total_tokens = ctx
            .metadata
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let token_count = ctx
            .metadata
            .get("token_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let new_total = total_tokens.saturating_add(token_count);

        debug!(
            hook = "usage_tracker",
            total_tokens = new_total,
            limit = self.max_tokens_per_session,
            "checking token budget"
        );

        if new_total > self.max_tokens_per_session {
            warn!(
                hook = "usage_tracker",
                total_tokens = new_total,
                limit = self.max_tokens_per_session,
                "session token budget exceeded"
            );
            return Ok(HookResult::Block {
                reason: format!(
                    "session token budget exceeded ({new_total} / {} tokens)",
                    self.max_tokens_per_session
                ),
            });
        }

        // Update the running total in metadata for downstream consumers.
        ctx.metadata["total_tokens"] = serde_json::json!(new_total);

        Ok(HookResult::Pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn post_ctx_with_tokens(total: u64, current: u64) -> HookContext {
        let mut ctx = HookContext::new("t", "s", "sess", "response text", HookPhase::PostInference);
        ctx.metadata["total_tokens"] = json!(total);
        ctx.metadata["token_count"] = json!(current);
        ctx
    }

    #[tokio::test]
    async fn passes_under_budget() {
        let tracker = UsageTracker::new(10_000);
        let mut ctx = post_ctx_with_tokens(5_000, 100);
        let result = tracker.execute(&mut ctx).await.unwrap();

        assert!(matches!(result, HookResult::Pass));
        assert_eq!(ctx.metadata["total_tokens"], 5_100);
    }

    #[tokio::test]
    async fn blocks_over_budget() {
        let tracker = UsageTracker::new(10_000);
        let mut ctx = post_ctx_with_tokens(9_900, 200);
        let result = tracker.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Block { .. }),
            "should block when budget exceeded"
        );
    }

    #[tokio::test]
    async fn blocks_at_exact_boundary() {
        let tracker = UsageTracker::new(10_000);
        // 10_000 + 1 = 10_001 > 10_000
        let mut ctx = post_ctx_with_tokens(10_000, 1);
        let result = tracker.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Block { .. }),
            "should block when total exceeds limit"
        );
    }

    #[tokio::test]
    async fn passes_at_exact_limit() {
        let tracker = UsageTracker::new(10_000);
        // 9_999 + 1 = 10_000 which is not > 10_000
        let mut ctx = post_ctx_with_tokens(9_999, 1);
        let result = tracker.execute(&mut ctx).await.unwrap();

        assert!(matches!(result, HookResult::Pass));
        assert_eq!(ctx.metadata["total_tokens"], 10_000);
    }

    #[tokio::test]
    async fn handles_missing_metadata() {
        let tracker = UsageTracker::new(10_000);
        let mut ctx = HookContext::new("t", "s", "sess", "response text", HookPhase::PostInference);
        // No token metadata at all -- should default to 0 and pass.
        let result = tracker.execute(&mut ctx).await.unwrap();

        assert!(matches!(result, HookResult::Pass));
        assert_eq!(ctx.metadata["total_tokens"], 0);
    }

    #[tokio::test]
    async fn saturates_on_overflow() {
        let tracker = UsageTracker::new(u64::MAX);
        let mut ctx = post_ctx_with_tokens(u64::MAX - 1, u64::MAX - 1);
        // Should saturate at u64::MAX, not overflow.
        let result = tracker.execute(&mut ctx).await.unwrap();

        // u64::MAX is not > u64::MAX, so this passes.
        assert!(matches!(result, HookResult::Pass));
    }
}
