//! Gateway error types with HTTP response conversion.

use acton_service::prelude::*;

/// Errors produced by the gateway service.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("database unavailable")]
    DatabaseUnavailable,

    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("rate limit exceeded")]
    RateLimited {
        /// Seconds until the client may retry.
        retry_after_secs: u64,
    },

    #[error("inference backend unavailable")]
    InferenceUnavailable,
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, message, extra_headers) = match &self {
            GatewayError::DatabaseUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string(), None)
            }
            GatewayError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error".into(),
                None,
            ),
            GatewayError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone(), None),
            GatewayError::Conflict(m) => (StatusCode::CONFLICT, m.clone(), None),
            GatewayError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone(), None),
            GatewayError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".into(),
                None,
            ),
            GatewayError::RateLimited { retry_after_secs } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded".into(),
                Some(("retry-after", retry_after_secs.to_string())),
            ),
            GatewayError::InferenceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "inference backend unavailable".into(),
                None,
            ),
        };
        let body = Json(serde_json::json!({"error": message}));
        let mut response = (status, body).into_response();
        if let Some((name, value)) = extra_headers
            && let Ok(header_value) = HeaderValue::from_str(&value)
        {
            response
                .headers_mut()
                .insert(axum::http::HeaderName::from_static(name), header_value);
        }
        response
    }
}

impl From<surrealdb::Error> for GatewayError {
    fn from(err: surrealdb::Error) -> Self {
        GatewayError::Database(err.to_string())
    }
}
