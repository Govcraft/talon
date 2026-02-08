//! Error types for the inference hook system.

use thiserror::Error;

/// Error produced by an individual hook during execution.
#[derive(Debug, Error)]
pub enum HookError {
    /// The hook encountered a failure it could not recover from.
    #[error("hook '{hook_id}' failed: {message}")]
    HookFailed {
        /// Identifier of the hook that failed.
        hook_id: String,
        /// Human-readable description of the failure.
        message: String,
    },

    /// An unexpected internal error occurred.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Error produced by the pipeline when running a phase.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// A hook blocked the message from proceeding.
    #[error("blocked by hook '{hook_id}': {reason}")]
    Blocked {
        /// Identifier of the hook that blocked the message.
        hook_id: String,
        /// Reason the message was blocked.
        reason: String,
    },

    /// A hook requires explicit approval before the message can proceed.
    #[error("approval required by hook '{hook_id}': {reason}")]
    ApprovalRequired {
        /// Identifier of the hook that requires approval.
        hook_id: String,
        /// Reason approval is required.
        reason: String,
    },

    /// An individual hook returned an error.
    #[error("hook error: {0}")]
    Hook(#[from] HookError),
}
