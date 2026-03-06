# Rate Limiting and Circuit Breaker Plan

## Summary

Add rate limiting to gateway HTTP routes and circuit breaker protection to inference calls, using APIs already available via acton-service's `governor` and `resilience` features.

## Analysis of Available APIs

### Governor (Rate Limiting)

acton-service provides `GovernorRateLimit` as an axum middleware using `axum::middleware::from_fn_with_state`. However, it requires `RateLimitConfig`, `CompiledRoutePatterns`, and JWT `Claims` -- a full middleware stack we do not need.

**Simpler approach**: Use `governor` crate directly (transitive dependency via acton-service). Create a standalone `GovernorLayer` using `governor::RateLimiter` with a `NotKeyed` + `InMemoryState` limiter. This is a simple tower layer that can be applied per route group.

The `GovernorConfig` type from acton-service (re-exported in prelude) provides convenient builder methods like `per_minute(30)` but it is only a config struct -- the actual middleware `GovernorRateLimit` requires `RateLimitConfig` which needs JWT infrastructure.

**Decision**: Build a thin rate limiting middleware directly in the gateway using `governor` crate types (already available transitively). This avoids coupling to acton-service's full auth stack while using the same underlying rate limiter.

### Resilience (Circuit Breaker)

acton-service re-exports `tower_resilience_circuitbreaker::CircuitBreakerLayer` and provides `ResilienceConfig` with a builder API:

```rust
use acton_service::middleware::resilience::ResilienceConfig;

let config = ResilienceConfig::default();
let layer: Option<CircuitBreakerLayer<Req, Err>> = config.circuit_breaker_layer();
```

The `CircuitBreakerLayer` wraps a tower `Service` and returns a `CircuitBreaker` service that:
- Monitors failure rates in a sliding window
- Opens circuit when threshold exceeded (rejects requests immediately)
- After wait duration, transitions to half-open for recovery testing
- Supports fallback handlers

**For InferenceService**: The inference methods are not tower services -- they are direct async methods. We have two options:
1. Convert to tower Service (heavy refactor, not warranted)
2. Use a lightweight circuit breaker state machine directly

**Decision**: Implement a simple `InferenceCircuitBreaker` that:
- Tracks failure counts with `AtomicU32`
- Uses three states: Closed, Open, HalfOpen
- On Open state, returns an error immediately without calling the LLM
- After configurable timeout, transitions to HalfOpen
- Single success in HalfOpen closes the circuit

This matches the behavior of `tower-resilience-circuitbreaker` but works with direct async function calls rather than tower services.

## Implementation Plan

### File Changes

#### 1. NEW: `crates/talon-gateway/src/rate_limit.rs`

Simple governor-based rate limiting middleware for axum.

```rust
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

/// Shared rate limiter state.
#[derive(Clone)]
pub struct ChatRateLimiter {
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl ChatRateLimiter {
    /// Create a new rate limiter allowing `per_minute` requests per minute
    /// with a burst allowance of `burst_size`.
    pub fn new(per_minute: u32, burst_size: u32) -> Self { ... }

    /// Axum middleware function.
    pub async fn middleware(
        axum::extract::State(limiter): axum::extract::State<Self>,
        request: Request<Body>,
        next: Next,
    ) -> Response { ... }
}
```

#### 2. NEW: `crates/talon-gateway/src/circuit_breaker.rs`

Lightweight circuit breaker for wrapping async operations.

```rust
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    Closed = 0,
    Open = 1,
    HalfOpen = 2,
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening.
    pub failure_threshold: u32,
    /// Duration to stay open before transitioning to half-open.
    pub reset_timeout: Duration,
    /// Number of successful probe calls to close the circuit.
    pub success_threshold: u32,
}

/// Lightweight circuit breaker for async function calls.
#[derive(Clone)]
pub struct CircuitBreaker { ... }

/// Error returned when the circuit is open.
#[derive(Debug)]
pub struct CircuitOpenError;

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self { ... }

    /// Check if a call is permitted. Returns Err if circuit is open.
    pub fn check(&self) -> Result<(), CircuitOpenError> { ... }

    /// Record a successful call.
    pub fn record_success(&self) { ... }

    /// Record a failed call.
    pub fn record_failure(&self) { ... }

    /// Get current state (lock-free).
    pub fn state(&self) -> CircuitState { ... }
}
```

#### 3. MODIFY: `crates/talon-gateway/src/routes.rs`

Apply rate limiting middleware to the chat route group.

```rust
use crate::rate_limit::ChatRateLimiter;

pub fn build_routes() -> VersionedRoutes {
    let chat_limiter = ChatRateLimiter::new(30, 5); // 30/min, burst 5

    VersionedApiBuilder::new()
        .with_base_path("/api")
        .add_version(ApiVersion::V1, |router| {
            let chat_routes = Router::new()
                .route("/chat", post(handlers::chat))
                .route("/chat/stream", post(handlers::chat_stream))
                .layer(axum::middleware::from_fn_with_state(
                    chat_limiter,
                    ChatRateLimiter::middleware,
                ));

            router
                .merge(chat_routes)
                // Sessions (no rate limit)
                .route("/sessions", get(handlers::list_sessions))
                .route("/sessions/{id}", get(handlers::get_session))
                // ... rest unchanged
        })
        .build_routes()
}
```

#### 4. MODIFY: `crates/talon-gateway/src/inference.rs`

Integrate circuit breaker into prompt methods.

```rust
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitOpenError};

pub struct InferenceService {
    ai: Arc<Mutex<Option<ActonAI>>>,
    circuit_breaker: CircuitBreaker,
}

impl InferenceService {
    fn global() -> Self {
        // Singleton circuit breaker shared across requests
        static CB: LazyLock<CircuitBreaker> = LazyLock::new(|| {
            CircuitBreaker::new(CircuitBreakerConfig {
                failure_threshold: 5,
                reset_timeout: Duration::from_secs(30),
                success_threshold: 2,
            })
        });
        Self { ai: AI.clone(), circuit_breaker: CB.clone() }
    }

    pub async fn prompt(&self, ...) -> Result<ChatResponse, anyhow::Error> {
        self.circuit_breaker.check()
            .map_err(|_| anyhow::anyhow!("inference backend unavailable (circuit open)"))?;

        let result = self.do_prompt(message, system_prompt, session).await;
        match &result {
            Ok(_) => self.circuit_breaker.record_success(),
            Err(_) => self.circuit_breaker.record_failure(),
        }
        result
    }
    // Same pattern for prompt_streaming
}
```

#### 5. MODIFY: `crates/talon-gateway/src/error.rs`

Add rate limit and circuit breaker error variants.

```rust
pub enum GatewayError {
    // ... existing variants
    #[error("rate limit exceeded")]
    RateLimited,
    #[error("inference backend unavailable")]
    InferenceUnavailable,
}
```

#### 6. MODIFY: `crates/talon-gateway/src/lib.rs`

Register new modules.

```rust
pub mod circuit_breaker;
pub mod rate_limit;
```

#### 7. MODIFY: `crates/talon-gateway/Cargo.toml`

Add `governor` as a direct dependency (currently only transitive).

```toml
governor = "0.8"
```

### Error Handling

- Rate limit exceeded: return HTTP 429 with `Retry-After` header
- Circuit open: return HTTP 503 with body `{"error": "inference backend unavailable"}`
- Both integrate into the existing `GatewayError` IntoResponse impl

### Test Strategy

1. **Rate limiter unit tests**: Verify burst allowance, rejection after limit
2. **Circuit breaker unit tests**: Verify state transitions (Closed -> Open -> HalfOpen -> Closed), failure counting, timeout behavior
3. **Integration**: `cargo check --workspace` and `cargo clippy --workspace -- -D warnings`

### Semver

**Patch bump (0.1.0 -> 0.1.1)**: These are internal implementation details. No public API surface changes. The rate limiter and circuit breaker are applied internally to existing routes and methods. External consumers see no API change (just potentially different HTTP status codes, which is operational behavior).

However, since the workspace is already at 0.1.0 and this is new functionality addition, a **minor bump to 0.2.0** may be more appropriate. Given 0.x.y semantics where minor bumps can include breaking changes, and this adds new behavior (429/503 responses), minor is the safer choice.

**Recommendation**: 0.2.0 (minor) -- new observable behavior via 429 and 503 responses.
