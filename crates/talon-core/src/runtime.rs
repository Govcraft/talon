//! Talon runtime initialization
//!
//! Provides the main runtime that initializes all core components:
//! - Actor system via acton-reactive
//! - Router actor for message routing
//! - Secure skill registry
//! - IPC listener for channel communication

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use acton_reactive::prelude::*;
use tracing::info;

use crate::error::{TalonError, TalonResult};
use crate::ipc::{DefaultIpcHandler, IpcServer, IpcServerConfig, TokenAuthenticator};
use crate::router::{Router, RouterConfig};
use crate::skills::{SecureSkillRegistry, SecureSkillRegistryConfig};

/// Configuration for the Talon runtime
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Path for the IPC Unix socket
    pub ipc_socket_path: PathBuf,
    /// Maximum concurrent conversations
    pub max_conversations: usize,
    /// Conversation timeout duration
    pub conversation_timeout: Duration,
    /// Ollama host URL (e.g., "http://localhost:11434")
    pub ollama_host: String,
    /// Ollama model name (e.g., "llama3.2")
    pub ollama_model: String,
    /// Secret key for token authentication
    pub secret_key: Vec<u8>,
    /// Skill registry configuration
    pub skill_registry_config: SecureSkillRegistryConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        // Default socket path following XDG conventions
        let socket_path = dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("talon")
            .join("talon.sock");

        Self {
            ipc_socket_path: socket_path,
            max_conversations: 1000,
            conversation_timeout: Duration::from_secs(3600),
            ollama_host: "http://localhost:11434".to_string(),
            ollama_model: "llama3.2".to_string(),
            secret_key: generate_default_secret(),
            skill_registry_config: SecureSkillRegistryConfig::default(),
        }
    }
}

/// Generate a default secret key for development
///
/// WARNING: In production, this should be loaded from a secure source
fn generate_default_secret() -> Vec<u8> {
    // Use a fixed development key - in production this would be loaded from config
    b"talon-development-secret-key-32b".to_vec()
}

/// The Talon runtime
///
/// Manages the lifecycle of all core components and provides
/// access to the skill registry and router.
pub struct TalonRuntime {
    /// The actor runtime
    runtime: ActorRuntime,
    /// Secure skill registry
    skill_registry: Arc<SecureSkillRegistry>,
    /// IPC message handler
    ipc_handler: Arc<DefaultIpcHandler>,
    /// IPC server
    ipc_server: Option<Arc<IpcServer>>,
    /// Runtime configuration
    config: RuntimeConfig,
    /// Whether the router has been started
    router_started: bool,
}

impl TalonRuntime {
    /// Create and initialize a new Talon runtime
    ///
    /// # Arguments
    ///
    /// * `config` - Runtime configuration
    ///
    /// # Errors
    ///
    /// Returns error if initialization fails.
    pub async fn new(config: RuntimeConfig) -> TalonResult<Self> {
        info!(
            socket_path = %config.ipc_socket_path.display(),
            max_conversations = config.max_conversations,
            ollama_host = %config.ollama_host,
            "initializing Talon runtime"
        );

        // Initialize the actor runtime
        let mut runtime = ActonApp::launch_async().await;

        // Create the skill registry
        let skill_registry = Arc::new(
            SecureSkillRegistry::with_config(config.skill_registry_config.clone())
                .map_err(|e| TalonError::Config {
                    message: format!("failed to create skill registry: {e}"),
                })?,
        );

        // Create the IPC handler
        let authenticator = TokenAuthenticator::new(&config.secret_key);
        let ipc_handler = Arc::new(DefaultIpcHandler::new(authenticator));

        // Create and start the router actor
        let _router_config = RouterConfig {
            max_conversations: config.max_conversations,
            conversation_timeout: config.conversation_timeout,
        };

        let router_builder = runtime.new_actor::<Router>();

        // Start the router
        let _router_handle = router_builder.start().await;

        info!("Talon runtime initialized successfully");

        Ok(Self {
            runtime,
            skill_registry,
            ipc_handler,
            ipc_server: None,
            config,
            router_started: true,
        })
    }

    /// Check if the router is started
    #[must_use]
    pub fn is_router_started(&self) -> bool {
        self.router_started
    }

    /// Get the skill registry
    #[must_use]
    pub fn skill_registry(&self) -> &Arc<SecureSkillRegistry> {
        &self.skill_registry
    }

    /// Get the IPC handler
    #[must_use]
    pub fn ipc_handler(&self) -> &Arc<DefaultIpcHandler> {
        &self.ipc_handler
    }

    /// Get the runtime configuration
    #[must_use]
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Start the IPC listener
    ///
    /// # Errors
    ///
    /// Returns error if the listener cannot be started.
    pub async fn start_ipc(&mut self) -> TalonResult<()> {
        if self.ipc_server.is_some() {
            return Err(TalonError::Ipc {
                message: "IPC server already started".to_string(),
            });
        }

        let server_config = IpcServerConfig {
            socket_path: self.config.ipc_socket_path.clone(),
            ..IpcServerConfig::default()
        };

        let server = Arc::new(IpcServer::new(
            server_config,
            Arc::clone(&self.ipc_handler) as Arc<dyn crate::ipc::IpcMessageHandler>,
        ));

        server.start().await?;

        self.ipc_server = Some(server);

        info!(
            socket_path = %self.config.ipc_socket_path.display(),
            "IPC server started"
        );

        Ok(())
    }

    /// Check if the IPC server is running
    #[must_use]
    pub fn is_ipc_running(&self) -> bool {
        self.ipc_server
            .as_ref()
            .map(|s| s.is_running())
            .unwrap_or(false)
    }

    /// Get access to the token authenticator for issuing channel tokens
    ///
    /// This is useful for the CLI to generate tokens for new channels.
    #[must_use]
    pub fn issue_channel_token(&self, channel_id: &crate::types::ChannelId) -> crate::ipc::AuthToken {
        self.ipc_handler.issue_token(channel_id)
    }

    /// Graceful shutdown of the runtime
    ///
    /// # Errors
    ///
    /// Returns error if shutdown fails.
    pub async fn shutdown(mut self) -> TalonResult<()> {
        info!("shutting down Talon runtime");

        // Stop IPC server first
        if let Some(server) = self.ipc_server.take() {
            server.stop().await?;
        }

        // Shutdown the actor runtime
        self.runtime
            .shutdown_all()
            .await
            .map_err(|e| TalonError::Actor {
                message: format!("shutdown failed: {e}"),
            })?;

        // Clean up the socket (should already be removed by server.stop())
        if self.config.ipc_socket_path.exists() {
            std::fs::remove_file(&self.config.ipc_socket_path)?;
        }

        info!("Talon runtime shutdown complete");
        Ok(())
    }
}

/// Builder for RuntimeConfig
#[derive(Default)]
pub struct RuntimeConfigBuilder {
    config: RuntimeConfig,
}

impl RuntimeConfigBuilder {
    /// Create a new builder with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the IPC socket path
    #[must_use]
    pub fn ipc_socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.ipc_socket_path = path.into();
        self
    }

    /// Set the maximum number of concurrent conversations
    #[must_use]
    pub fn max_conversations(mut self, max: usize) -> Self {
        self.config.max_conversations = max;
        self
    }

    /// Set the conversation timeout
    #[must_use]
    pub fn conversation_timeout(mut self, timeout: Duration) -> Self {
        self.config.conversation_timeout = timeout;
        self
    }

    /// Set the Ollama host URL
    #[must_use]
    pub fn ollama_host(mut self, host: impl Into<String>) -> Self {
        self.config.ollama_host = host.into();
        self
    }

    /// Set the Ollama model name
    #[must_use]
    pub fn ollama_model(mut self, model: impl Into<String>) -> Self {
        self.config.ollama_model = model.into();
        self
    }

    /// Set the secret key for token authentication
    #[must_use]
    pub fn secret_key(mut self, key: Vec<u8>) -> Self {
        self.config.secret_key = key;
        self
    }

    /// Set the skill registry configuration
    #[must_use]
    pub fn skill_registry_config(mut self, config: SecureSkillRegistryConfig) -> Self {
        self.config.skill_registry_config = config;
        self
    }

    /// Build the configuration
    #[must_use]
    pub fn build(self) -> RuntimeConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_conversations, 1000);
        assert_eq!(config.conversation_timeout, Duration::from_secs(3600));
        assert_eq!(config.ollama_host, "http://localhost:11434");
    }

    #[test]
    fn test_config_builder() {
        let config = RuntimeConfigBuilder::new()
            .max_conversations(500)
            .ollama_model("codellama")
            .conversation_timeout(Duration::from_secs(1800))
            .build();

        assert_eq!(config.max_conversations, 500);
        assert_eq!(config.ollama_model, "codellama");
        assert_eq!(config.conversation_timeout, Duration::from_secs(1800));
    }

    #[test]
    fn test_config_builder_socket_path() {
        let config = RuntimeConfigBuilder::new()
            .ipc_socket_path("/custom/path/talon.sock")
            .build();

        assert_eq!(
            config.ipc_socket_path,
            PathBuf::from("/custom/path/talon.sock")
        );
    }

    #[test]
    fn test_generate_default_secret() {
        let secret = generate_default_secret();
        assert_eq!(secret.len(), 32);
    }

    #[tokio::test]
    async fn test_runtime_creation() {
        let config = RuntimeConfigBuilder::new()
            .ipc_socket_path("/tmp/talon-test.sock")
            .max_conversations(10)
            .build();

        let result = TalonRuntime::new(config).await;
        assert!(result.is_ok());

        let runtime = result.expect("runtime should be created");
        assert_eq!(runtime.config().max_conversations, 10);
        assert!(runtime.is_router_started());

        // Clean shutdown
        let shutdown_result = runtime.shutdown().await;
        assert!(shutdown_result.is_ok());
    }
}
