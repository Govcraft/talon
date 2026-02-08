//! Lightweight circuit breaker for wrapping async operations.
//!
//! Provides fail-fast behaviour when a downstream service (e.g. an LLM backend)
//! is experiencing persistent failures. Instead of timing out on every request
//! the circuit opens and rejects calls immediately, giving the backend time to
//! recover.
//!
//! ## State machine
//!
//! ```text
//!    success             failure_threshold reached
//! +--------+           +--------+
//! | Closed | --------> |  Open  |
//! +--------+           +--------+
//!      ^                   |
//!      |  success in       | reset_timeout elapsed
//!      |  half-open        v
//!      |              +-----------+
//!      +------------- | Half-Open |
//!                     +-----------+
//! ```

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    /// Normal operation -- all calls pass through.
    Closed = 0,
    /// Too many failures -- calls are rejected immediately.
    Open = 1,
    /// Recovery probe -- a limited number of calls are allowed through.
    HalfOpen = 2,
}

impl CircuitState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Open,
            2 => Self::HalfOpen,
            _ => Self::Closed,
        }
    }
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "Closed"),
            Self::Open => write!(f, "Open"),
            Self::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit opens.
    pub failure_threshold: u32,
    /// How long the circuit stays open before transitioning to half-open.
    pub reset_timeout: Duration,
    /// Number of consecutive successes in half-open state required to close.
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Error returned when a call is rejected because the circuit is open.
#[derive(Debug)]
pub struct CircuitOpenError;

impl fmt::Display for CircuitOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "circuit breaker is open")
    }
}

impl std::error::Error for CircuitOpenError {}

/// Internal mutable state protected by a mutex.
struct InnerState {
    consecutive_failures: u32,
    consecutive_successes: u32,
    opened_at: Option<Instant>,
}

/// Lightweight circuit breaker for async function calls.
///
/// Thread-safe and cheaply cloneable. The state is shared via `Arc` so all
/// clones refer to the same circuit.
#[derive(Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    /// Atomic state for lock-free reads (e.g. health checks).
    state: Arc<AtomicU8>,
    /// Total failure count (monotonic, for observability).
    total_failures: Arc<AtomicU32>,
    /// Mutable inner state for transitions.
    inner: Arc<Mutex<InnerState>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(AtomicU8::new(CircuitState::Closed as u8)),
            total_failures: Arc::new(AtomicU32::new(0)),
            inner: Arc::new(Mutex::new(InnerState {
                consecutive_failures: 0,
                consecutive_successes: 0,
                opened_at: None,
            })),
        }
    }

    /// Check whether a call is currently permitted.
    ///
    /// Returns `Ok(())` if the circuit is closed or if the reset timeout has
    /// elapsed (transitioning to half-open). Returns `Err(CircuitOpenError)` if
    /// the circuit is open and the timeout has not elapsed.
    pub async fn check(&self) -> Result<(), CircuitOpenError> {
        let current = self.current_state();
        match current {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => Ok(()),
            CircuitState::Open => {
                // Check if reset timeout has elapsed.
                let mut inner = self.inner.lock().await;
                if let Some(opened_at) = inner.opened_at
                    && opened_at.elapsed() >= self.config.reset_timeout
                {
                    // Transition to half-open.
                    inner.consecutive_successes = 0;
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Release);
                    tracing::info!("circuit breaker transitioning Open -> HalfOpen");
                    return Ok(());
                }
                Err(CircuitOpenError)
            }
        }
    }

    /// Record a successful call.
    ///
    /// In the half-open state, consecutive successes eventually close the
    /// circuit. In the closed state, this resets the failure counter.
    pub async fn record_success(&self) {
        let mut inner = self.inner.lock().await;
        inner.consecutive_failures = 0;

        let current = self.current_state();
        if current == CircuitState::HalfOpen {
            inner.consecutive_successes += 1;
            if inner.consecutive_successes >= self.config.success_threshold {
                inner.opened_at = None;
                inner.consecutive_successes = 0;
                self.state
                    .store(CircuitState::Closed as u8, Ordering::Release);
                tracing::info!("circuit breaker transitioning HalfOpen -> Closed");
            }
        }
    }

    /// Record a failed call.
    ///
    /// In the closed state, consecutive failures eventually open the circuit.
    /// In the half-open state, a single failure re-opens the circuit.
    pub async fn record_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        let mut inner = self.inner.lock().await;
        inner.consecutive_failures += 1;
        inner.consecutive_successes = 0;

        let current = self.current_state();
        match current {
            CircuitState::Closed => {
                if inner.consecutive_failures >= self.config.failure_threshold {
                    inner.opened_at = Some(Instant::now());
                    self.state
                        .store(CircuitState::Open as u8, Ordering::Release);
                    tracing::warn!(
                        failures = inner.consecutive_failures,
                        "circuit breaker transitioning Closed -> Open"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open re-opens the circuit.
                inner.opened_at = Some(Instant::now());
                self.state
                    .store(CircuitState::Open as u8, Ordering::Release);
                tracing::warn!("circuit breaker transitioning HalfOpen -> Open");
            }
            CircuitState::Open => {
                // Already open, just update the timestamp.
                inner.opened_at = Some(Instant::now());
            }
        }
    }

    /// Return the current circuit state (lock-free read).
    pub fn current_state(&self) -> CircuitState {
        CircuitState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Return the total number of recorded failures (monotonic counter).
    pub fn total_failures(&self) -> u32 {
        self.total_failures.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_millis(100),
            success_threshold: 2,
        }
    }

    #[tokio::test]
    async fn test_starts_closed() {
        let cb = CircuitBreaker::new(test_config());
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check().await.is_ok());
    }

    #[tokio::test]
    async fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            cb.record_failure().await;
        }

        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(cb.check().await.is_err());
    }

    #[tokio::test]
    async fn test_does_not_open_below_threshold() {
        let cb = CircuitBreaker::new(test_config());

        cb.record_failure().await;
        cb.record_failure().await;

        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check().await.is_ok());
    }

    #[tokio::test]
    async fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new(test_config());

        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_success().await;
        cb.record_failure().await;
        cb.record_failure().await;

        // Should still be closed: success reset the counter.
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_transitions_to_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(50),
            success_threshold: 1,
        };
        let cb = CircuitBreaker::new(config);

        cb.record_failure().await;
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Wait for the reset timeout.
        tokio::time::sleep(Duration::from_millis(60)).await;

        // check() should transition to HalfOpen.
        assert!(cb.check().await.is_ok());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_closes_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(10),
            success_threshold: 1,
        };
        let cb = CircuitBreaker::new(config);

        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(cb.check().await.is_ok()); // -> HalfOpen

        cb.record_success().await;
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_reopens_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout: Duration::from_millis(10),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(cb.check().await.is_ok()); // -> HalfOpen

        cb.record_failure().await;
        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_total_failures_counter() {
        let cb = CircuitBreaker::new(test_config());

        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_success().await;
        cb.record_failure().await;

        assert_eq!(cb.total_failures(), 3);
    }
}
