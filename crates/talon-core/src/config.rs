//! Talon configuration management

use crate::error::{TalonError, TalonResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main Talon configuration
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TalonConfig {
    /// Core daemon settings
    pub core: CoreConfig,
    /// LLM provider settings
    pub llm: LlmConfig,
    /// Trust configuration
    pub trust: TrustConfig,
}

/// Core daemon configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreConfig {
    /// IPC socket path
    pub socket_path: String,
    /// Log level
    pub log_level: String,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/talon.sock".into(),
            log_level: "info".into(),
        }
    }
}

/// LLM provider configuration
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default provider name
    pub default_provider: Option<String>,
}

/// Trust configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustConfig {
    /// Maximum allowed trust tier
    pub max_tier: u8,
    /// Whether to require attestations
    pub require_attestation: bool,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            max_tier: 2,
            require_attestation: true,
        }
    }
}

impl TalonConfig {
    /// Load configuration from a TOML file
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be read or parsed
    pub fn from_file(path: impl AsRef<Path>) -> TalonResult<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| TalonError::Config {
            message: e.to_string(),
        })
    }

    /// Load configuration from default locations
    ///
    /// Searches in order:
    /// 1. ./talon.toml
    /// 2. ~/.config/talon/config.toml
    ///
    /// # Errors
    ///
    /// Returns error if no config found or parse fails
    pub fn load() -> TalonResult<Self> {
        let paths = [
            "./talon.toml".to_string(),
            dirs::config_dir()
                .map(|p| p.join("talon/config.toml").to_string_lossy().to_string())
                .unwrap_or_default(),
        ];

        for path in &paths {
            if !path.is_empty() && Path::new(path).exists() {
                return Self::from_file(path);
            }
        }

        // Return default config if no file found
        Ok(Self::default())
    }
}
