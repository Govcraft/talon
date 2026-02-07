//! HTTP client for TalonHub registry
//!
//! Provides communication with the TalonHub registry for fetching
//! skill attestations, metadata, and trust root information.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::skills::error::{SkillSecurityError, SkillSecurityResult};

/// Response from the registry for skill attestation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttestationResponse {
    /// The PASETO attestation token
    pub token: String,

    /// OmniBOR ID of the attested skill
    pub omnibor_id: String,

    /// Skill version
    pub version: String,

    /// When the attestation was issued
    pub issued_at: chrono::DateTime<chrono::Utc>,

    /// When the attestation expires
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Response from the registry for skill metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Agent URI of the skill
    pub agent_uri: String,

    /// Skill name
    pub name: String,

    /// Skill description
    pub description: String,

    /// Publisher name
    pub publisher: String,

    /// Allowed tools as specified in SKILL.md
    pub allowed_tools: Vec<String>,

    /// Required trust tier
    pub trust_tier: u8,

    /// Current version
    pub version: String,

    /// When the skill was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When the skill was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Response from the registry for trust root keys
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustRootKeys {
    /// The trust root domain
    pub trust_root: String,

    /// Available keys
    pub keys: Vec<PublicKeyInfo>,
}

/// Public key information from trust root
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicKeyInfo {
    /// Key identifier
    pub kid: String,

    /// Algorithm (e.g., "Ed25519")
    pub algorithm: String,

    /// Base64-encoded public key
    pub public_key: String,

    /// When the key becomes valid
    pub not_before: chrono::DateTime<chrono::Utc>,

    /// When the key expires
    pub not_after: chrono::DateTime<chrono::Utc>,
}

/// Configuration for the registry client
#[derive(Clone, Debug)]
pub struct RegistryClientConfig {
    /// Base URL of the registry
    pub base_url: String,

    /// Request timeout
    pub timeout: Duration,

    /// Number of retries for failed requests
    pub max_retries: u32,

    /// User-Agent header value
    pub user_agent: String,
}

impl Default for RegistryClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://talonhub.io".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            user_agent: format!("talon-core/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// HTTP client for communicating with TalonHub registry
pub struct RegistryClient {
    /// The HTTP client
    client: Client,

    /// Client configuration
    config: RegistryClientConfig,
}

impl RegistryClient {
    /// Create a new registry client with default configuration
    ///
    /// # Errors
    ///
    /// Returns error if the HTTP client cannot be created.
    pub fn new() -> SkillSecurityResult<Self> {
        Self::with_config(RegistryClientConfig::default())
    }

    /// Create a new registry client with custom configuration
    ///
    /// # Errors
    ///
    /// Returns error if the HTTP client cannot be created.
    pub fn with_config(config: RegistryClientConfig) -> SkillSecurityResult<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| SkillSecurityError::RegistryError {
                message: format!("failed to create HTTP client: {e}"),
            })?;

        Ok(Self { client, config })
    }

    /// Fetch attestation for a skill
    ///
    /// # Arguments
    ///
    /// * `agent_uri` - The agent URI of the skill
    ///
    /// # Errors
    ///
    /// Returns error if the request fails or the skill is not found.
    pub async fn fetch_attestation(
        &self,
        agent_uri: &str,
    ) -> SkillSecurityResult<AttestationResponse> {
        let encoded_uri = urlencoding::encode(agent_uri);
        let url = format!(
            "{}/api/v1/skills/{}/attestation",
            self.config.base_url, encoded_uri
        );

        self.get_with_retry(&url).await
    }

    /// Fetch metadata for a skill
    ///
    /// # Arguments
    ///
    /// * `agent_uri` - The agent URI of the skill
    ///
    /// # Errors
    ///
    /// Returns error if the request fails or the skill is not found.
    pub async fn fetch_skill_metadata(
        &self,
        agent_uri: &str,
    ) -> SkillSecurityResult<SkillMetadata> {
        let encoded_uri = urlencoding::encode(agent_uri);
        let url = format!("{}/api/v1/skills/{}", self.config.base_url, encoded_uri);

        self.get_with_retry(&url).await
    }

    /// Fetch trust root keys
    ///
    /// # Arguments
    ///
    /// * `domain` - The trust root domain
    ///
    /// # Errors
    ///
    /// Returns error if the request fails or the trust root is not found.
    pub async fn fetch_trust_root_keys(&self, domain: &str) -> SkillSecurityResult<TrustRootKeys> {
        let url = format!(
            "{}/api/v1/trust-roots/{}/keys",
            self.config.base_url, domain
        );

        self.get_with_retry(&url).await
    }

    /// Fetch trust root keys from well-known endpoint
    ///
    /// # Arguments
    ///
    /// * `domain` - The trust root domain
    ///
    /// # Errors
    ///
    /// Returns error if the request fails.
    pub async fn fetch_well_known_keys(&self, domain: &str) -> SkillSecurityResult<TrustRootKeys> {
        let url = format!("https://{}/.well-known/agent-keys.json", domain);

        self.get_with_retry(&url).await
    }

    /// Download the skill archive from the registry
    ///
    /// Returns the raw bytes of the skill archive (typically a SKILL.md file).
    ///
    /// # Arguments
    ///
    /// * `agent_uri` - The agent URI of the skill to download
    ///
    /// # Errors
    ///
    /// Returns error if the download fails or the skill is not found.
    pub async fn download_skill_archive(
        &self,
        agent_uri: &str,
    ) -> SkillSecurityResult<Vec<u8>> {
        let encoded_uri = urlencoding::encode(agent_uri);
        let url = format!(
            "{}/api/v1/skills/{}/download",
            self.config.base_url, encoded_uri
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SkillSecurityError::ArchiveDownloadFailed {
                agent_uri: agent_uri.to_string(),
                reason: e.to_string(),
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(SkillSecurityError::ArchiveDownloadFailed {
                agent_uri: agent_uri.to_string(),
                reason: format!("HTTP {status}"),
            });
        }

        response
            .bytes()
            .await
            .map_err(|e| SkillSecurityError::ArchiveDownloadFailed {
                agent_uri: agent_uri.to_string(),
                reason: e.to_string(),
            })
            .map(|b| b.to_vec())
    }

    /// Check if a skill exists in the registry
    ///
    /// # Arguments
    ///
    /// * `agent_uri` - The agent URI of the skill
    ///
    /// # Errors
    ///
    /// Returns error if the request fails.
    pub async fn skill_exists(&self, agent_uri: &str) -> SkillSecurityResult<bool> {
        let encoded_uri = urlencoding::encode(agent_uri);
        let url = format!("{}/api/v1/skills/{}", self.config.base_url, encoded_uri);

        let response = self.client.head(&url).send().await?;

        Ok(response.status().is_success())
    }

    /// Perform a GET request with retries
    async fn get_with_retry<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> SkillSecurityResult<T> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                // Exponential backoff
                let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                tokio::time::sleep(delay).await;
            }

            match self.client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return response.json::<T>().await.map_err(|e| {
                            SkillSecurityError::SerializationError {
                                message: format!("failed to parse response: {e}"),
                            }
                        });
                    }

                    if status.as_u16() == 404 {
                        return Err(SkillSecurityError::RegistryError {
                            message: "resource not found".to_string(),
                        });
                    }

                    // Don't retry client errors (4xx)
                    if status.is_client_error() {
                        return Err(SkillSecurityError::HttpError {
                            status: Some(status.as_u16()),
                            message: format!("request failed: {status}"),
                        });
                    }

                    // Server errors (5xx) - will retry
                    last_error = Some(SkillSecurityError::HttpError {
                        status: Some(status.as_u16()),
                        message: format!("server error: {status}"),
                    });
                }
                Err(e) => {
                    last_error = Some(e.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| SkillSecurityError::RegistryError {
            message: "request failed after retries".to_string(),
        }))
    }

    /// Get the client configuration
    #[must_use]
    pub fn config(&self) -> &RegistryClientConfig {
        &self.config
    }

    /// Get the base URL
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RegistryClientConfig::default();
        assert_eq!(config.base_url, "https://talonhub.io");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_client_creation() {
        let client = RegistryClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_custom_config() {
        let config = RegistryClientConfig {
            base_url: "https://custom.registry.io".to_string(),
            timeout: Duration::from_secs(10),
            max_retries: 5,
            user_agent: "custom-agent/1.0".to_string(),
        };

        let client = RegistryClient::with_config(config);
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.base_url(), "https://custom.registry.io");
    }
}
