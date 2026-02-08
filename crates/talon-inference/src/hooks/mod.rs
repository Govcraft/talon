//! Built-in inference hooks.
//!
//! These hooks provide common pre- and post-inference processing:
//!
//! - [`InputSanitizer`] -- strips HTML tags and script content from input.
//! - [`PiiDetector`] -- detects personally identifiable information using regex patterns.
//! - [`UsageTracker`] -- enforces per-session token budgets.

pub mod input_sanitizer;
pub mod pii_detector;
pub mod usage_tracker;

pub use input_sanitizer::InputSanitizer;
pub use pii_detector::{PiiAction, PiiDetector};
pub use usage_tracker::UsageTracker;
