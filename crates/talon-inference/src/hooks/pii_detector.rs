//! PII detection hook using regex patterns.

use async_trait::async_trait;
use regex::Regex;
use std::sync::LazyLock;
use tracing::{debug, warn};

use crate::{HookContext, HookError, HookPhase, HookResult, InferenceHook};

/// Email address pattern.
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
        .expect("email regex should compile")
});

/// US phone number pattern (various formats).
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:\+?1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}\b")
        .expect("phone regex should compile")
});

/// US Social Security Number pattern (XXX-XX-XXXX).
static SSN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[0-9]{3}-[0-9]{2}-[0-9]{4}\b").expect("ssn regex should compile")
});

/// What to do when PII is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiiAction {
    /// Block the message entirely.
    Block,
    /// Replace detected PII with `[REDACTED]` and continue.
    Redact,
    /// Log a warning but allow the message through unchanged.
    Warn,
}

/// Regex-based PII detector that scans for email addresses, phone numbers,
/// and Social Security Numbers.
///
/// The [`PiiAction`] controls the hook's behavior when PII is found.
///
/// This hook is useful in both the pre-inference phase (to prevent PII
/// from reaching the LLM) and the post-inference phase (to scrub PII
/// from model output).
pub struct PiiDetector {
    action: PiiAction,
    hook_phase: HookPhase,
}

impl PiiDetector {
    /// Create a pre-inference PII detector with the given action.
    pub fn new(action: PiiAction) -> Self {
        Self {
            action,
            hook_phase: HookPhase::PreInference,
        }
    }

    /// Create a PII detector that runs in the specified phase.
    pub fn with_phase(action: PiiAction, phase: HookPhase) -> Self {
        Self {
            action,
            hook_phase: phase,
        }
    }
}

/// Categories of PII that were detected.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PiiFindings {
    emails: Vec<String>,
    phones: Vec<String>,
    ssns: Vec<String>,
}

impl PiiFindings {
    fn is_empty(&self) -> bool {
        self.emails.is_empty() && self.phones.is_empty() && self.ssns.is_empty()
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.emails.is_empty() {
            parts.push(format!("{} email(s)", self.emails.len()));
        }
        if !self.phones.is_empty() {
            parts.push(format!("{} phone(s)", self.phones.len()));
        }
        if !self.ssns.is_empty() {
            parts.push(format!("{} SSN(s)", self.ssns.len()));
        }
        parts.join(", ")
    }
}

/// Scan the content for all known PII patterns.
fn detect_pii(content: &str) -> PiiFindings {
    PiiFindings {
        emails: EMAIL_RE
            .find_iter(content)
            .map(|m| m.as_str().to_string())
            .collect(),
        phones: PHONE_RE
            .find_iter(content)
            .map(|m| m.as_str().to_string())
            .collect(),
        ssns: SSN_RE
            .find_iter(content)
            .map(|m| m.as_str().to_string())
            .collect(),
    }
}

/// Replace all detected PII in the content with `[REDACTED]`.
fn redact_pii(content: &str) -> String {
    let result = SSN_RE.replace_all(content, "[REDACTED]");
    let result = PHONE_RE.replace_all(&result, "[REDACTED]");
    let result = EMAIL_RE.replace_all(&result, "[REDACTED]");
    result.into_owned()
}

#[async_trait]
impl InferenceHook for PiiDetector {
    fn id(&self) -> &str {
        "pii_detector"
    }

    fn phase(&self) -> HookPhase {
        self.hook_phase
    }

    fn priority(&self) -> u32 {
        20
    }

    #[tracing::instrument(skip(self, ctx))]
    async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError> {
        let findings = detect_pii(&ctx.content);

        if findings.is_empty() {
            return Ok(HookResult::Pass);
        }

        let summary = findings.summary();
        debug!(hook = "pii_detector", findings = %summary, "PII detected");

        match self.action {
            PiiAction::Block => {
                warn!(hook = "pii_detector", findings = %summary, "blocking message with PII");
                Ok(HookResult::Block {
                    reason: format!("PII detected: {summary}"),
                })
            }
            PiiAction::Redact => {
                let redacted = redact_pii(&ctx.content);
                debug!(hook = "pii_detector", "redacted PII from content");
                Ok(HookResult::Continue(redacted))
            }
            PiiAction::Warn => {
                warn!(hook = "pii_detector", findings = %summary, "PII detected (warn-only mode)");
                // Store findings in metadata for downstream hooks to inspect.
                ctx.metadata["pii_findings"] = serde_json::json!({
                    "emails": findings.emails.len(),
                    "phones": findings.phones.len(),
                    "ssns": findings.ssns.len(),
                });
                Ok(HookResult::Pass)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre_ctx(content: &str) -> HookContext {
        HookContext::new("t", "s", "sess", content, HookPhase::PreInference)
    }

    #[tokio::test]
    async fn detects_email() {
        let detector = PiiDetector::new(PiiAction::Block);
        let mut ctx = pre_ctx("contact me at user@example.com");
        let result = detector.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Block { .. }),
            "should block email PII"
        );
    }

    #[tokio::test]
    async fn detects_phone() {
        let detector = PiiDetector::new(PiiAction::Block);
        let mut ctx = pre_ctx("call me at (555) 123-4567");
        let result = detector.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Block { .. }),
            "should block phone PII"
        );
    }

    #[tokio::test]
    async fn detects_ssn() {
        let detector = PiiDetector::new(PiiAction::Block);
        let mut ctx = pre_ctx("my ssn is 123-45-6789");
        let result = detector.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Block { .. }),
            "should block SSN PII"
        );
    }

    #[tokio::test]
    async fn redacts_email() {
        let detector = PiiDetector::new(PiiAction::Redact);
        let mut ctx = pre_ctx("email: user@example.com please");
        let result = detector.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(redacted) => {
                assert_eq!(redacted, "email: [REDACTED] please");
                assert!(
                    !redacted.contains("user@example.com"),
                    "email should be redacted"
                );
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn redacts_ssn() {
        let detector = PiiDetector::new(PiiAction::Redact);
        let mut ctx = pre_ctx("ssn: 123-45-6789");
        let result = detector.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(redacted) => {
                assert_eq!(redacted, "ssn: [REDACTED]");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn redacts_phone() {
        let detector = PiiDetector::new(PiiAction::Redact);
        let mut ctx = pre_ctx("phone: 555-123-4567");
        let result = detector.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(redacted) => {
                assert_eq!(redacted, "phone: [REDACTED]");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn warn_passes_with_metadata() {
        let detector = PiiDetector::new(PiiAction::Warn);
        let mut ctx = pre_ctx("email: user@example.com and ssn: 123-45-6789");
        let result = detector.execute(&mut ctx).await.unwrap();

        assert!(matches!(result, HookResult::Pass));
        assert_eq!(ctx.metadata["pii_findings"]["emails"], 1);
        assert_eq!(ctx.metadata["pii_findings"]["ssns"], 1);
    }

    #[tokio::test]
    async fn no_pii_passes() {
        let detector = PiiDetector::new(PiiAction::Block);
        let mut ctx = pre_ctx("just a normal message");
        let result = detector.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Pass),
            "should pass when no PII found"
        );
    }

    #[tokio::test]
    async fn redacts_multiple_pii() {
        let detector = PiiDetector::new(PiiAction::Redact);
        let mut ctx = pre_ctx("alice@test.com and bob@test.com");
        let result = detector.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(redacted) => {
                assert_eq!(redacted, "[REDACTED] and [REDACTED]");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_inference_phase() {
        let detector = PiiDetector::with_phase(PiiAction::Redact, HookPhase::PostInference);
        assert_eq!(detector.phase(), HookPhase::PostInference);
    }
}
