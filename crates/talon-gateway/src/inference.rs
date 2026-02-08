use std::sync::Arc;
use std::time::Duration;

use acton_ai::prelude::*;
use acton_service::prelude::{AppState, SseEvent};
use talon_types::{ChatResponse, Session};
use tokio::sync::Mutex;

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

/// Wraps `ActonAI` for LLM inference with circuit breaker protection.
///
/// When the LLM backend experiences repeated failures, the circuit breaker
/// opens and subsequent calls fail fast with an error instead of timing out.
/// After a configurable reset timeout the breaker enters half-open state and
/// allows probe requests through to test recovery.
pub struct InferenceService {
    ai: Arc<Mutex<Option<ActonAI>>>,
    circuit_breaker: CircuitBreaker,
}

impl InferenceService {
    /// Obtain an inference service from application state.
    ///
    /// For Phase 1 this ignores the state and returns the global singleton.
    #[tracing::instrument(skip(_state))]
    pub async fn from_state(_state: &AppState) -> std::result::Result<Self, anyhow::Error> {
        Ok(Self::global())
    }

    /// Return the global singleton inference service.
    fn global() -> Self {
        use std::sync::LazyLock;

        static AI: LazyLock<Arc<Mutex<Option<ActonAI>>>> =
            LazyLock::new(|| Arc::new(Mutex::new(None)));

        static CB: LazyLock<CircuitBreaker> = LazyLock::new(|| {
            CircuitBreaker::new(CircuitBreakerConfig {
                failure_threshold: 5,
                reset_timeout: Duration::from_secs(30),
                success_threshold: 2,
            })
        });

        Self {
            ai: AI.clone(),
            circuit_breaker: CB.clone(),
        }
    }

    /// Ensure the underlying `ActonAI` runtime is initialised.
    #[tracing::instrument(skip(self))]
    async fn ensure_ai(&self) -> std::result::Result<(), anyhow::Error> {
        let mut guard = self.ai.lock().await;
        if guard.is_none() {
            let ai = ActonAI::builder()
                .app_name("talon-gateway")
                .from_config()
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .with_builtins()
                .launch()
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            *guard = Some(ai);
        }
        Ok(())
    }

    /// Send a prompt and collect the complete response.
    ///
    /// The call is gated by the circuit breaker. If the LLM backend has been
    /// failing repeatedly the breaker will be open and this method returns an
    /// error immediately without contacting the backend.
    #[tracing::instrument(skip(self))]
    pub async fn prompt(
        &self,
        message: &str,
        system_prompt: Option<&str>,
        session: &Session,
    ) -> std::result::Result<ChatResponse, anyhow::Error> {
        self.circuit_breaker
            .check()
            .await
            .map_err(|_| anyhow::anyhow!("inference backend unavailable (circuit open)"))?;

        let result = self.do_prompt(message, system_prompt, session).await;

        match &result {
            Ok(_) => self.circuit_breaker.record_success().await,
            Err(_) => self.circuit_breaker.record_failure().await,
        }

        result
    }

    /// Inner prompt implementation without circuit breaker logic.
    async fn do_prompt(
        &self,
        message: &str,
        system_prompt: Option<&str>,
        session: &Session,
    ) -> std::result::Result<ChatResponse, anyhow::Error> {
        self.ensure_ai().await?;
        let guard = self.ai.lock().await;
        let ai = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ActonAI not initialised"))?;

        let mut prompt = ai.prompt(message);
        if let Some(sys) = system_prompt {
            prompt = prompt.system(sys);
        }

        let response = prompt.collect().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(ChatResponse {
            text: response.text,
            session_id: session.id.as_str().to_string(),
            model: ai.default_provider_name().to_string(),
            token_count: response.token_count as u32,
        })
    }

    /// Send a prompt and stream tokens as SSE events over the provided channel.
    ///
    /// Like [`prompt`](Self::prompt), this is gated by the circuit breaker.
    #[tracing::instrument(skip(self, tx))]
    pub async fn prompt_streaming(
        &self,
        message: &str,
        system_prompt: Option<&str>,
        session: &Session,
        tx: tokio::sync::mpsc::Sender<std::result::Result<SseEvent, std::convert::Infallible>>,
    ) -> std::result::Result<(), anyhow::Error> {
        self.circuit_breaker
            .check()
            .await
            .map_err(|_| anyhow::anyhow!("inference backend unavailable (circuit open)"))?;

        let result = self
            .do_prompt_streaming(message, system_prompt, session, tx)
            .await;

        match &result {
            Ok(()) => self.circuit_breaker.record_success().await,
            Err(_) => self.circuit_breaker.record_failure().await,
        }

        result
    }

    /// Inner streaming prompt implementation without circuit breaker logic.
    async fn do_prompt_streaming(
        &self,
        message: &str,
        system_prompt: Option<&str>,
        _session: &Session,
        tx: tokio::sync::mpsc::Sender<std::result::Result<SseEvent, std::convert::Infallible>>,
    ) -> std::result::Result<(), anyhow::Error> {
        self.ensure_ai().await?;
        let guard = self.ai.lock().await;
        let ai = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ActonAI not initialised"))?;

        let tx_token = tx.clone();
        let mut prompt = ai.prompt(message);
        if let Some(sys) = system_prompt {
            prompt = prompt.system(sys);
        }
        prompt = prompt.on_token(move |token: &str| {
            let data = serde_json::json!({"type": "token", "data": token}).to_string();
            let event = SseEvent::default().data(data);
            let _ = tx_token.try_send(Ok(event));
        });

        let response = prompt.collect().await.map_err(|e| anyhow::anyhow!("{e}"))?;

        // Send the final done event.
        let final_data = serde_json::json!({
            "type": "done",
            "data": {
                "text": response.text,
                "token_count": response.token_count
            }
        })
        .to_string();
        let event = SseEvent::default().data(final_data);
        let _ = tx.send(Ok(event)).await;

        Ok(())
    }
}
