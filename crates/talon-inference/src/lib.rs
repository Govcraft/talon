//! Inference pipeline and hook system for the Talon AI gateway.
//!
//! This crate provides a hook-based pipeline for processing messages before
//! and after LLM inference. Hooks can sanitize input, detect PII, enforce
//! usage limits, and filter output -- all without any framework dependencies.
//!
//! # Architecture
//!
//! ```text
//! InboundMessage
//!     |
//!     v
//! [Pre-Inference Hooks]   <- input sanitizer, PII detector, etc.
//!     |
//!     v
//! [LLM Inference]         <- external (not part of this crate)
//!     |
//!     v
//! [Post-Inference Hooks]  <- usage tracker, content filter, etc.
//!     |
//!     v
//! OutboundMessage
//! ```
//!
//! # Example
//!
//! ```rust
//! use talon_inference::{HookPipeline, HookContext, HookPhase};
//! use talon_inference::hooks::{InputSanitizer, PiiDetector, PiiAction, UsageTracker};
//!
//! let mut pipeline = HookPipeline::new();
//! pipeline.register(InputSanitizer);
//! pipeline.register(PiiDetector::new(PiiAction::Redact));
//! pipeline.register(UsageTracker::new(10_000));
//! ```

pub mod error;
pub mod hook;
pub mod hooks;
pub mod pipeline;

pub use error::*;
pub use hook::*;
pub use pipeline::*;
