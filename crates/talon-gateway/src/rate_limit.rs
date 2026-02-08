//! Governor-based rate limiting middleware for gateway HTTP routes.
//!
//! Provides a simple, in-memory rate limiter that can be applied as axum
//! middleware to specific route groups. Uses the governor crate's Generic Cell
//! Rate Algorithm internally.

use std::num::NonZeroU32;
use std::sync::Arc;

use acton_service::prelude::*;
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use crate::error::GatewayError;

/// Type alias for the governor rate limiter we use.
type GovernorLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Shared rate limiter state for chat endpoints.
///
/// Wraps a governor `RateLimiter` configured for a fixed request-per-minute
/// quota with burst allowance. Designed to be used as axum middleware state.
#[derive(Clone)]
pub struct ChatRateLimiter {
    limiter: Arc<GovernorLimiter>,
    /// Configured requests per minute (for response headers).
    limit: u32,
}

impl ChatRateLimiter {
    /// Create a new rate limiter.
    ///
    /// # Arguments
    ///
    /// * `per_minute` - Maximum sustained requests per minute.
    /// * `burst_size` - Additional burst capacity above the sustained rate.
    ///
    /// The `burst_size` controls how many requests can be made in a quick burst
    /// before the sustained rate kicks in. A burst of 5 with a rate of 30/min
    /// means 5 requests can fire immediately, then subsequent requests are
    /// spaced at 2-second intervals.
    pub fn new(per_minute: u32, burst_size: u32) -> Self {
        let replenish_interval_ms = 60_000u64 / u64::from(per_minute.max(1));
        let burst = NonZeroU32::new(burst_size.max(1)).expect("burst_size.max(1) is always >= 1");
        let quota = Quota::with_period(std::time::Duration::from_millis(replenish_interval_ms))
            .expect("replenish interval should be valid")
            .allow_burst(burst);
        let limiter = Arc::new(RateLimiter::direct(quota));

        Self {
            limiter,
            limit: per_minute,
        }
    }

    /// Axum middleware function that enforces the rate limit.
    ///
    /// Returns HTTP 429 with a `Retry-After` header when the limit is exceeded.
    /// Adds `X-RateLimit-Limit` header to successful responses.
    pub async fn middleware(
        State(rate_limiter): State<Self>,
        request: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> Response {
        match rate_limiter.limiter.check() {
            Ok(_) => {
                let mut response = next.run(request).await;
                Self::add_headers(&mut response, rate_limiter.limit);
                response
            }
            Err(not_until) => {
                let retry_after = not_until.wait_time_from(governor::clock::Clock::now(
                    &governor::clock::DefaultClock::default(),
                ));

                warn!(
                    retry_after_secs = retry_after.as_secs(),
                    "chat rate limit exceeded"
                );

                let error = GatewayError::RateLimited {
                    retry_after_secs: retry_after.as_secs(),
                };
                error.into_response()
            }
        }
    }

    /// Add rate limit informational headers to a successful response.
    fn add_headers(response: &mut Response, limit: u32) {
        let headers = response.headers_mut();
        if let Ok(value) = HeaderValue::from_str(&limit.to_string()) {
            headers.insert(
                axum::http::HeaderName::from_static("x-ratelimit-limit"),
                value,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_burst() {
        let limiter = ChatRateLimiter::new(30, 5);

        // Should allow the initial burst.
        for _ in 0..5 {
            assert!(limiter.limiter.check().is_ok());
        }

        // Next request should be throttled (burst exhausted).
        assert!(limiter.limiter.check().is_err());
    }

    #[test]
    fn test_single_request_allowed() {
        let limiter = ChatRateLimiter::new(60, 1);
        assert!(limiter.limiter.check().is_ok());
    }

    #[test]
    fn test_stores_limit_value() {
        let limiter = ChatRateLimiter::new(42, 3);
        assert_eq!(limiter.limit, 42);
    }
}
