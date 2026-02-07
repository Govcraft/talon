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

use acton_ai::prelude::*;
use acton_ai::skills::SkillRegistry;
use acton_reactive::prelude::{ActonApp, ActorRuntime};
use tokio::sync::RwLock;
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
    /// TalonHub URL for skill registry
    pub hub_url: Option<String>,
    /// List of skill agent URIs to load at startup
    pub hub_skill_uris: Vec<String>,
    /// Trust root domain to fetch keys from
    pub hub_trust_root_domain: Option<String>,
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
            ollama_host: "http://localhost:11434/v1".to_string(),
            ollama_model: "qwen2.5:7b".to_string(),
            secret_key: generate_default_secret(),
            skill_registry_config: SecureSkillRegistryConfig::default(),
            hub_url: None,
            hub_skill_uris: vec![],
            hub_trust_root_domain: None,
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
    /// Secure skill registry (wrapped in RwLock for runtime mutability)
    skill_registry: Arc<RwLock<SecureSkillRegistry>>,
    /// ActonAI runtime with built-in tools
    acton_ai: Arc<ActonAI>,
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

        // Create the shared inner registry that both SecureSkillRegistry
        // and DefaultIpcHandler will reference
        let shared_registry = Arc::new(RwLock::new(SkillRegistry::new()));

        // Create the secure skill registry with the shared inner
        let skill_registry = SecureSkillRegistry::with_shared_registry(
            config.skill_registry_config.clone(),
            Arc::clone(&shared_registry),
        )
        .map_err(|e| TalonError::Config {
            message: format!("failed to create skill registry: {e}"),
        })?;

        let skill_registry = Arc::new(RwLock::new(skill_registry));

        // Create ActonAI runtime with built-in tools
        let acton_ai = ActonAI::builder()
            .app_name("talon")
            .ollama_at(&config.ollama_host, &config.ollama_model)
            .with_builtins()
            .launch()
            .await
            .map_err(|e| TalonError::Config {
                message: format!("failed to create ActonAI runtime: {e}"),
            })?;

        let acton_ai = Arc::new(acton_ai);

        info!(
            ollama_host = %config.ollama_host,
            ollama_model = %config.ollama_model,
            "ActonAI runtime created with built-in tools"
        );

        // Create the IPC handler with ActonAI and shared skill registry
        let authenticator = TokenAuthenticator::new(&config.secret_key);
        let ipc_handler = Arc::new(DefaultIpcHandler::with_acton_ai_and_skills(
            authenticator,
            Arc::clone(&acton_ai),
            Arc::clone(&shared_registry),
        ));

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
            acton_ai,
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
    pub fn skill_registry(&self) -> &Arc<RwLock<SecureSkillRegistry>> {
        &self.skill_registry
    }

    /// Get the ActonAI runtime
    #[must_use]
    pub fn acton_ai(&self) -> &Arc<ActonAI> {
        &self.acton_ai
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

    /// Load skills from the configured hub
    ///
    /// Fetches trust root keys and loads configured skills.
    /// This should be called after runtime creation but before
    /// accepting user messages.
    ///
    /// # Errors
    ///
    /// Returns error if trust root or skill loading fails.
    pub async fn load_skills(&self) -> TalonResult<()> {
        let mut registry = self.skill_registry.write().await;

        if let Some(domain) = &self.config.hub_trust_root_domain {
            info!(domain = %domain, "fetching trust root keys");
            registry
                .fetch_and_add_trust_root(domain)
                .await
                .map_err(|e| TalonError::Config {
                    message: format!("failed to fetch trust root: {e}"),
                })?;
        }

        for uri in &self.config.hub_skill_uris {
            info!(uri = %uri, "loading skill from hub");
            registry
                .load_from_hub(uri)
                .await
                .map_err(|e| TalonError::Config {
                    message: format!("failed to load skill {uri}: {e}"),
                })?;
        }

        Ok(())
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

    /// Set the TalonHub URL for skill registry
    #[must_use]
    pub fn hub_url(mut self, url: impl Into<String>) -> Self {
        self.config.hub_url = Some(url.into());
        self
    }

    /// Set the list of skill agent URIs to load at startup
    #[must_use]
    pub fn hub_skill_uris(mut self, uris: Vec<String>) -> Self {
        self.config.hub_skill_uris = uris;
        self
    }

    /// Set the trust root domain to fetch keys from
    #[must_use]
    pub fn hub_trust_root_domain(mut self, domain: impl Into<String>) -> Self {
        self.config.hub_trust_root_domain = Some(domain.into());
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
        assert_eq!(config.ollama_host, "http://localhost:11434/v1");
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

    #[test]
    fn test_hub_config_defaults() {
        let config = RuntimeConfig::default();
        assert!(config.hub_url.is_none());
        assert!(config.hub_skill_uris.is_empty());
        assert!(config.hub_trust_root_domain.is_none());
    }

    #[test]
    fn test_config_builder_hub() {
        let config = RuntimeConfigBuilder::new()
            .hub_url("http://localhost:3000")
            .hub_trust_root_domain("talonhub.io")
            .hub_skill_uris(vec!["agent://talonhub.io/skill/example".to_string()])
            .build();

        assert_eq!(config.hub_url.as_deref(), Some("http://localhost:3000"));
        assert_eq!(
            config.hub_trust_root_domain.as_deref(),
            Some("talonhub.io")
        );
        assert_eq!(config.hub_skill_uris.len(), 1);
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
