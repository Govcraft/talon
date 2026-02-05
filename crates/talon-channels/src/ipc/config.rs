//! IPC client configuration

use std::path::PathBuf;
use std::time::Duration;

use talon_core::ChannelId;

/// Default socket path following XDG conventions
fn default_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("talon")
        .join("talon.sock")
}

/// IPC client configuration
#[derive(Clone, Debug)]
pub struct IpcClientConfig {
    /// Path to the Unix Domain Socket
    pub socket_path: PathBuf,
    /// Authentication token
    pub auth_token: String,
    /// Channel identifier
    pub channel_id: ChannelId,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Response timeout
    pub response_timeout: Duration,
    /// Whether to automatically reconnect on disconnect
    pub auto_reconnect: bool,
    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts
    pub reconnect_delay: Duration,
}

impl IpcClientConfig {
    /// Create a new configuration with required fields
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier
    /// * `auth_token` - The authentication token
    #[must_use]
    pub fn new(channel_id: ChannelId, auth_token: impl Into<String>) -> Self {
        Self {
            socket_path: default_socket_path(),
            auth_token: auth_token.into(),
            channel_id,
            connect_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(30),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(1),
        }
    }

    /// Create a builder for more control over configuration
    #[must_use]
    pub fn builder(channel_id: ChannelId, auth_token: impl Into<String>) -> IpcClientConfigBuilder {
        IpcClientConfigBuilder::new(channel_id, auth_token)
    }
}

/// Builder for `IpcClientConfig`
#[derive(Clone, Debug)]
pub struct IpcClientConfigBuilder {
    config: IpcClientConfig,
}

impl IpcClientConfigBuilder {
    /// Create a new builder with required fields
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier
    /// * `auth_token` - The authentication token
    #[must_use]
    pub fn new(channel_id: ChannelId, auth_token: impl Into<String>) -> Self {
        Self {
            config: IpcClientConfig::new(channel_id, auth_token),
        }
    }

    /// Set the socket path
    #[must_use]
    pub fn socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.socket_path = path.into();
        self
    }

    /// Set the connection timeout
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    /// Set the response timeout
    #[must_use]
    pub fn response_timeout(mut self, timeout: Duration) -> Self {
        self.config.response_timeout = timeout;
        self
    }

    /// Enable or disable auto-reconnect
    #[must_use]
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.config.auto_reconnect = enabled;
        self
    }

    /// Set the maximum number of reconnection attempts
    #[must_use]
    pub fn max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.config.max_reconnect_attempts = attempts;
        self
    }

    /// Set the delay between reconnection attempts
    #[must_use]
    pub fn reconnect_delay(mut self, delay: Duration) -> Self {
        self.config.reconnect_delay = delay;
        self
    }

    /// Build the configuration
    #[must_use]
    pub fn build(self) -> IpcClientConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = IpcClientConfig::new(ChannelId::new("telegram"), "test-token");

        assert_eq!(config.channel_id.as_str(), "telegram");
        assert_eq!(config.auth_token, "test-token");
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.response_timeout, Duration::from_secs(30));
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 5);
    }

    #[test]
    fn test_builder() {
        let config = IpcClientConfig::builder(ChannelId::new("discord"), "secret")
            .socket_path("/custom/path.sock")
            .connect_timeout(Duration::from_secs(10))
            .response_timeout(Duration::from_secs(60))
            .auto_reconnect(false)
            .max_reconnect_attempts(3)
            .reconnect_delay(Duration::from_millis(500))
            .build();

        assert_eq!(config.channel_id.as_str(), "discord");
        assert_eq!(config.socket_path, PathBuf::from("/custom/path.sock"));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.response_timeout, Duration::from_secs(60));
        assert!(!config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 3);
        assert_eq!(config.reconnect_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_default_socket_path() {
        let path = default_socket_path();
        assert!(path.to_string_lossy().contains("talon.sock"));
    }
}
