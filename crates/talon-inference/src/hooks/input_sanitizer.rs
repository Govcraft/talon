//! Input sanitizer hook that strips HTML and script content.

use async_trait::async_trait;
use regex::Regex;
use std::sync::LazyLock;
use tracing::debug;

use crate::{HookContext, HookError, HookPhase, HookResult, InferenceHook};

/// Regex that matches `<script>...</script>` blocks (case-insensitive, dotall).
static SCRIPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex should compile")
});

/// Regex that matches any HTML tag.
static HTML_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("html tag regex should compile"));

/// Pre-inference hook that strips HTML tags and `<script>` blocks from
/// user input.
///
/// Execution order: priority 10 (runs early in the pre-inference phase).
///
/// If the content is modified, the hook returns [`HookResult::Continue`]
/// with the cleaned text.  If no HTML is found, it returns [`HookResult::Pass`].
pub struct InputSanitizer;

#[async_trait]
impl InferenceHook for InputSanitizer {
    fn id(&self) -> &str {
        "input_sanitizer"
    }

    fn phase(&self) -> HookPhase {
        HookPhase::PreInference
    }

    fn priority(&self) -> u32 {
        10
    }

    #[tracing::instrument(skip(self, ctx))]
    async fn execute(&self, ctx: &mut HookContext) -> Result<HookResult, HookError> {
        let original = &ctx.content;

        // First strip script blocks, then remaining HTML tags.
        let without_scripts = SCRIPT_RE.replace_all(original, "");
        let cleaned = HTML_TAG_RE.replace_all(&without_scripts, "");

        if cleaned == *original {
            return Ok(HookResult::Pass);
        }

        let cleaned = cleaned.into_owned();
        debug!(
            hook = "input_sanitizer",
            original_len = original.len(),
            cleaned_len = cleaned.len(),
            "sanitized input"
        );

        Ok(HookResult::Continue(cleaned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HookContext;

    fn pre_ctx(content: &str) -> HookContext {
        HookContext::new("t", "s", "sess", content, HookPhase::PreInference)
    }

    #[tokio::test]
    async fn strips_script_tags() {
        let sanitizer = InputSanitizer;
        let mut ctx = pre_ctx("hello <script>alert('xss')</script> world");
        let result = sanitizer.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(cleaned) => {
                assert_eq!(cleaned, "hello  world");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn strips_html_tags() {
        let sanitizer = InputSanitizer;
        let mut ctx = pre_ctx("<b>bold</b> and <i>italic</i>");
        let result = sanitizer.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(cleaned) => {
                assert_eq!(cleaned, "bold and italic");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn preserves_plain_text() {
        let sanitizer = InputSanitizer;
        let mut ctx = pre_ctx("just plain text");
        let result = sanitizer.execute(&mut ctx).await.unwrap();

        assert!(
            matches!(result, HookResult::Pass),
            "plain text should pass unchanged"
        );
    }

    #[tokio::test]
    async fn strips_multiline_script() {
        let sanitizer = InputSanitizer;
        let mut ctx = pre_ctx(
            "before\n<script type=\"text/javascript\">\nconsole.log('hi');\n</script>\nafter",
        );
        let result = sanitizer.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(cleaned) => {
                assert_eq!(cleaned, "before\n\nafter");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn strips_nested_html() {
        let sanitizer = InputSanitizer;
        let mut ctx = pre_ctx("<div><p>hello</p></div>");
        let result = sanitizer.execute(&mut ctx).await.unwrap();

        match result {
            HookResult::Continue(cleaned) => {
                assert_eq!(cleaned, "hello");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
    }
}
