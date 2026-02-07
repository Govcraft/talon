//! Secure tool execution with capability verification
//!
//! Wraps tool execution with capability checks to ensure skills
//! can only invoke tools they are authorized to use.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::{TalonError, TalonResult};
use crate::skills::registry::SecureSkillRegistry;
use crate::skills::verified::SkillId;
use crate::types::{ConversationId, CorrelationId};

/// Context for a tool execution
///
/// Contains metadata about the skill and conversation that initiated
/// the tool call.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    /// The skill making the tool call
    pub skill_id: SkillId,
    /// Name of the skill for logging
    pub skill_name: String,
    /// Conversation ID for correlation
    pub conversation_id: ConversationId,
    /// Correlation ID for request tracking
    pub correlation_id: CorrelationId,
}

impl ExecutionContext {
    /// Create a new execution context
    #[must_use]
    pub fn new(
        skill_id: SkillId,
        skill_name: impl Into<String>,
        conversation_id: ConversationId,
        correlation_id: CorrelationId,
    ) -> Self {
        Self {
            skill_id,
            skill_name: skill_name.into(),
            conversation_id,
            correlation_id,
        }
    }
}

/// Result of a tool execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// The output from the tool
    pub output: serde_json::Value,
    /// How long the execution took
    #[serde(with = "duration_millis")]
    pub execution_time: Duration,
}

/// Serde helper for Duration as milliseconds
mod duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

impl ExecutionResult {
    /// Create a new execution result
    #[must_use]
    pub fn new(output: serde_json::Value, execution_time: Duration) -> Self {
        Self {
            output,
            execution_time,
        }
    }

    /// Create a result with just output (execution time of zero)
    #[must_use]
    pub fn with_output(output: serde_json::Value) -> Self {
        Self {
            output,
            execution_time: Duration::ZERO,
        }
    }
}

/// Secure tool executor with capability verification
///
/// Before executing any tool, this executor verifies that the
/// requesting skill has the necessary capabilities.
pub struct SecureToolExecutor {
    /// Reference to the skill registry for capability checks
    registry: Arc<SecureSkillRegistry>,
}

impl SecureToolExecutor {
    /// Create a new executor with a skill registry
    ///
    /// # Arguments
    ///
    /// * `registry` - The secure skill registry for capability verification
    #[must_use]
    pub fn new(registry: Arc<SecureSkillRegistry>) -> Self {
        Self { registry }
    }

    /// Check if a tool is allowed for a skill
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Name of the skill
    /// * `tool_name` - Name of the tool to check
    ///
    /// # Returns
    ///
    /// `true` if the skill has the capability to use this tool.
    #[must_use]
    pub fn is_tool_allowed(&self, skill_name: &str, tool_name: &str) -> bool {
        self.registry
            .check_tool_allowed(skill_name, tool_name)
            .is_ok()
    }

    /// Execute a tool with capability verification
    ///
    /// # Arguments
    ///
    /// * `ctx` - The execution context
    /// * `tool_name` - Name of the tool to execute
    /// * `arguments` - Arguments for the tool
    ///
    /// # Errors
    ///
    /// Returns `TalonError::CapabilityDenied` if the skill doesn't have permission.
    /// Returns `TalonError::ToolExecution` if the tool execution fails.
    pub async fn execute(
        &self,
        ctx: &ExecutionContext,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> TalonResult<ExecutionResult> {
        let start = Instant::now();

        // Verify capability
        debug!(
            skill = %ctx.skill_name,
            tool = %tool_name,
            correlation_id = %ctx.correlation_id,
            "checking tool capability"
        );

        self.registry
            .check_tool_allowed(&ctx.skill_name, tool_name)
            .map_err(|e| {
                warn!(
                    skill = %ctx.skill_name,
                    tool = %tool_name,
                    error = %e,
                    "capability check failed"
                );
                TalonError::CapabilityDenied {
                    skill: ctx.skill_name.clone(),
                    capability: tool_name.to_string(),
                }
            })?;

        info!(
            skill = %ctx.skill_name,
            tool = %tool_name,
            correlation_id = %ctx.correlation_id,
            "executing tool"
        );

        // Execute the tool
        // NOTE: Actual tool execution would be delegated to acton-ai
        // For now, we return a placeholder result
        let result = self.execute_tool_inner(tool_name, arguments).await?;

        let execution_time = start.elapsed();

        info!(
            skill = %ctx.skill_name,
            tool = %tool_name,
            execution_time_ms = %execution_time.as_millis(),
            "tool execution complete"
        );

        Ok(ExecutionResult::new(result, execution_time))
    }

    /// Execute multiple tools in sequence
    ///
    /// Stops at the first error and returns all successful results.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The execution context
    /// * `calls` - List of (tool_name, arguments) pairs
    ///
    /// # Returns
    ///
    /// Vector of results for each successful call, or error on first failure.
    pub async fn execute_batch(
        &self,
        ctx: &ExecutionContext,
        calls: Vec<(String, serde_json::Value)>,
    ) -> TalonResult<Vec<ExecutionResult>> {
        let mut results = Vec::with_capacity(calls.len());

        for (tool_name, arguments) in calls {
            let result = self.execute(ctx, &tool_name, arguments).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Inner tool execution (placeholder)
    ///
    /// In the real implementation, this would delegate to acton-ai's
    /// tool execution system.
    async fn execute_tool_inner(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> TalonResult<serde_json::Value> {
        // Placeholder implementation
        // Real implementation would:
        // 1. Look up the tool in acton-ai
        // 2. Validate arguments against tool schema
        // 3. Execute in Hyperlight sandbox if needed
        // 4. Return the result

        Ok(serde_json::json!({
            "tool": tool_name,
            "status": "executed",
            "arguments_received": arguments
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::registry::SecureSkillRegistryConfig;

    fn test_registry() -> Arc<SecureSkillRegistry> {
        let config = SecureSkillRegistryConfig {
            require_attestation: false,
            verify_integrity: false,
            ..Default::default()
        };
        Arc::new(SecureSkillRegistry::with_config(config).expect("registry should be creatable"))
    }

    fn test_context() -> ExecutionContext {
        ExecutionContext::new(
            SkillId::new(),
            "test-skill",
            ConversationId::new(),
            CorrelationId::new(),
        )
    }

    #[test]
    fn test_execution_context_creation() {
        let ctx = test_context();
        assert_eq!(ctx.skill_name, "test-skill");
    }

    #[test]
    fn test_execution_result_serialization() {
        let result = ExecutionResult::new(
            serde_json::json!({"key": "value"}),
            Duration::from_millis(100),
        );

        let json = serde_json::to_string(&result).expect("should serialize");
        let parsed: ExecutionResult = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(parsed.execution_time.as_millis(), 100);
    }

    #[test]
    fn test_execution_result_with_output() {
        let result = ExecutionResult::with_output(serde_json::json!({"ok": true}));
        assert_eq!(result.execution_time, Duration::ZERO);
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let registry = test_registry();
        let executor = SecureToolExecutor::new(registry);

        // Without any skills loaded, all tool checks should fail
        assert!(!executor.is_tool_allowed("nonexistent", "Read"));
    }

    #[tokio::test]
    async fn test_execute_unknown_skill() {
        let registry = test_registry();
        let executor = SecureToolExecutor::new(registry);
        let ctx = test_context();

        let result = executor.execute(&ctx, "Read", serde_json::json!({})).await;

        assert!(result.is_err());
        match result {
            Err(TalonError::CapabilityDenied { skill, .. }) => {
                assert_eq!(skill, "test-skill");
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }
}
