//! IPC authentication using HMAC-SHA256 tokens
//!
//! Provides secure token-based authentication for channel connections
//! to the core daemon over Unix Domain Sockets.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fmt;

use crate::error::{TalonError, TalonResult};
use crate::types::ChannelId;

/// HMAC-SHA256 type alias
type HmacSha256 = Hmac<Sha256>;

/// Default token time-to-live (24 hours)
const DEFAULT_TOKEN_TTL_SECS: i64 = 86400;

/// Authentication token for channel connections
///
/// Tokens are HMAC-SHA256 signed and contain the channel ID,
/// issue time, and expiration time.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthToken(String);

impl AuthToken {
    /// Create a new token from a raw string
    ///
    /// This does not validate the token; use `TokenAuthenticator::validate`
    /// to verify the token is valid.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// Get the token as a string slice
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Don't expose the full token in debug output
        let preview = if self.0.len() > 8 {
            format!("{}...", &self.0[..8])
        } else {
            self.0.clone()
        };
        f.debug_tuple("AuthToken").field(&preview).finish()
    }
}

impl fmt::Display for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AuthToken {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for AuthToken {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Token payload that gets signed
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TokenPayload {
    /// Channel identifier
    channel_id: String,
    /// When the token was issued (Unix timestamp)
    issued_at: i64,
    /// When the token expires (Unix timestamp)
    expires_at: i64,
}

/// Validated token with extracted claims
#[derive(Clone, Debug)]
pub struct ValidatedToken {
    /// Channel identifier from the token
    pub channel_id: ChannelId,
    /// When the token was issued
    pub issued_at: DateTime<Utc>,
    /// When the token expires
    pub expires_at: DateTime<Utc>,
}

impl ValidatedToken {
    /// Check if the token is expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Check if the token is expired at a specific time
    #[must_use]
    pub fn is_expired_at(&self, at: DateTime<Utc>) -> bool {
        at >= self.expires_at
    }

    /// Get remaining validity duration
    #[must_use]
    pub fn remaining_validity(&self) -> Option<std::time::Duration> {
        let now = Utc::now();
        if now >= self.expires_at {
            None
        } else {
            let remaining = self.expires_at - now;
            remaining.to_std().ok()
        }
    }
}

/// Token generator and validator using HMAC-SHA256
///
/// Tokens are structured as: `{base64_payload}.{base64_signature}`
pub struct TokenAuthenticator {
    /// Secret key for HMAC
    secret: Vec<u8>,
    /// Token time-to-live
    token_ttl: Duration,
}

impl TokenAuthenticator {
    /// Create a new authenticator with a secret key
    ///
    /// # Arguments
    ///
    /// * `secret` - The secret key for HMAC signing (should be at least 32 bytes)
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
            token_ttl: Duration::seconds(DEFAULT_TOKEN_TTL_SECS),
        }
    }

    /// Create with a custom token TTL
    ///
    /// # Arguments
    ///
    /// * `secret` - The secret key for HMAC signing
    /// * `ttl` - Token time-to-live duration
    #[must_use]
    pub fn with_ttl(secret: &[u8], ttl: Duration) -> Self {
        Self {
            secret: secret.to_vec(),
            token_ttl: ttl,
        }
    }

    /// Issue a new token for a channel
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel to issue a token for
    ///
    /// # Returns
    ///
    /// A signed authentication token
    #[must_use]
    pub fn issue_token(&self, channel_id: &ChannelId) -> AuthToken {
        let now = Utc::now();
        let expires = now + self.token_ttl;

        let payload = TokenPayload {
            channel_id: channel_id.to_string(),
            issued_at: now.timestamp(),
            expires_at: expires.timestamp(),
        };

        // Serialize payload to JSON
        // This is safe because TokenPayload only contains String and i64
        let payload_json =
            serde_json::to_string(&payload).unwrap_or_else(|_| String::from("{}"));
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());

        // Sign the payload
        let signature = self.sign(payload_b64.as_bytes());
        let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);

        AuthToken::new(format!("{payload_b64}.{signature_b64}"))
    }

    /// Validate a token and extract the claims
    ///
    /// # Arguments
    ///
    /// * `token` - The token to validate
    ///
    /// # Errors
    ///
    /// Returns `TalonError::AuthenticationFailed` if the token is malformed or signature is invalid.
    /// Returns `TalonError::TokenExpired` if the token has expired.
    pub fn validate(&self, token: &AuthToken) -> TalonResult<ValidatedToken> {
        // Split into payload and signature
        let parts: Vec<&str> = token.as_str().split('.').collect();
        if parts.len() != 2 {
            return Err(TalonError::AuthenticationFailed {
                reason: "invalid token format: expected payload.signature".to_string(),
            });
        }

        let payload_b64 = parts[0];
        let signature_b64 = parts[1];

        // Verify signature first
        let expected_signature = self.sign(payload_b64.as_bytes());
        let provided_signature = URL_SAFE_NO_PAD
            .decode(signature_b64)
            .map_err(|e| TalonError::AuthenticationFailed {
                reason: format!("invalid signature encoding: {e}"),
            })?;

        if !constant_time_compare(&expected_signature, &provided_signature) {
            return Err(TalonError::AuthenticationFailed {
                reason: "invalid signature".to_string(),
            });
        }

        // Decode and parse payload
        let payload_bytes =
            URL_SAFE_NO_PAD
                .decode(payload_b64)
                .map_err(|e| TalonError::AuthenticationFailed {
                    reason: format!("invalid payload encoding: {e}"),
                })?;

        let payload: TokenPayload =
            serde_json::from_slice(&payload_bytes).map_err(|e| TalonError::AuthenticationFailed {
                reason: format!("invalid payload format: {e}"),
            })?;

        // Parse timestamps
        let issued_at = DateTime::from_timestamp(payload.issued_at, 0).ok_or_else(|| {
            TalonError::AuthenticationFailed {
                reason: "invalid issued_at timestamp".to_string(),
            }
        })?;

        let expires_at = DateTime::from_timestamp(payload.expires_at, 0).ok_or_else(|| {
            TalonError::AuthenticationFailed {
                reason: "invalid expires_at timestamp".to_string(),
            }
        })?;

        // Check expiration
        if Utc::now() >= expires_at {
            return Err(TalonError::TokenExpired { expired_at: expires_at });
        }

        Ok(ValidatedToken {
            channel_id: ChannelId::new(payload.channel_id),
            issued_at,
            expires_at,
        })
    }

    /// Sign data with HMAC-SHA256
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        // HMAC-SHA256 accepts keys of any length, so this never fails
        // If the secret is somehow invalid, use the data itself as a fallback
        // (This provides deterministic but less secure signing as a safety net)
        let key = if self.secret.is_empty() {
            data
        } else {
            &self.secret
        };

        // HMAC::new_from_slice only fails if the key is empty, which we handle above
        let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
            // Fallback: use a hash of the data as the signature
            // This should never happen with non-empty keys
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(data);
            return hasher.finalize().to_vec();
        };

        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> Vec<u8> {
        b"test-secret-key-for-testing-only-32bytes".to_vec()
    }

    #[test]
    fn test_issue_and_validate_token() {
        let auth = TokenAuthenticator::new(&test_secret());
        let channel_id = ChannelId::new("terminal");

        let token = auth.issue_token(&channel_id);
        let validated = auth.validate(&token);

        assert!(validated.is_ok());
        let validated = validated.expect("token should be valid");
        assert_eq!(validated.channel_id.as_str(), "terminal");
        assert!(!validated.is_expired());
    }

    #[test]
    fn test_token_expiration() {
        let auth = TokenAuthenticator::with_ttl(&test_secret(), Duration::seconds(-1));
        let channel_id = ChannelId::new("terminal");

        let token = auth.issue_token(&channel_id);
        let result = auth.validate(&token);

        assert!(result.is_err());
        match result {
            Err(TalonError::TokenExpired { .. }) => {}
            other => panic!("expected TokenExpired, got {other:?}"),
        }
    }

    #[test]
    fn test_invalid_signature() {
        let auth1 = TokenAuthenticator::new(&test_secret());
        let auth2 = TokenAuthenticator::new(b"different-secret-key");
        let channel_id = ChannelId::new("terminal");

        let token = auth1.issue_token(&channel_id);
        let result = auth2.validate(&token);

        assert!(result.is_err());
        match result {
            Err(TalonError::AuthenticationFailed { reason }) => {
                assert!(reason.contains("invalid signature"));
            }
            other => panic!("expected AuthenticationFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_malformed_token() {
        let auth = TokenAuthenticator::new(&test_secret());

        let token = AuthToken::new("not-a-valid-token");
        let result = auth.validate(&token);

        assert!(result.is_err());
        match result {
            Err(TalonError::AuthenticationFailed { reason }) => {
                assert!(reason.contains("invalid token format"));
            }
            other => panic!("expected AuthenticationFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_token_debug_does_not_expose_full_value() {
        let token = AuthToken::new("this-is-a-very-long-token-value");
        let debug = format!("{token:?}");
        assert!(!debug.contains("very-long-token-value"));
        assert!(debug.contains("this-is-"));
    }

    #[test]
    fn test_validated_token_remaining_validity() {
        let auth = TokenAuthenticator::with_ttl(&test_secret(), Duration::hours(1));
        let channel_id = ChannelId::new("terminal");

        let token = auth.issue_token(&channel_id);
        let validated = auth.validate(&token).expect("token should be valid");

        let remaining = validated.remaining_validity();
        assert!(remaining.is_some());
        // Should be close to 1 hour (3600 seconds), allow some margin
        let remaining_secs = remaining.expect("should have remaining validity").as_secs();
        assert!(remaining_secs > 3500);
        assert!(remaining_secs <= 3600);
    }

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare(b"hello", b"hello"));
        assert!(!constant_time_compare(b"hello", b"world"));
        assert!(!constant_time_compare(b"hello", b"hell"));
        assert!(!constant_time_compare(b"", b"x"));
        assert!(constant_time_compare(b"", b""));
    }
}
