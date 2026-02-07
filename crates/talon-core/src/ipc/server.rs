//! IPC server for channel connections
//!
//! Provides a Unix Domain Socket server that accepts connections from
//! channel binaries and dispatches messages to handlers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::error::{TalonError, TalonResult};
use crate::ipc::IpcMessageHandler;
use crate::ipc::messages::{ChannelToCore, CoreToChannel};
use crate::types::ChannelId;

/// Maximum message size (16 MiB)
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// IPC server configuration
#[derive(Clone, Debug)]
pub struct IpcServerConfig {
    /// Path to the Unix Domain Socket
    pub socket_path: PathBuf,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection read timeout
    pub read_timeout: Duration,
    /// Connection write timeout
    pub write_timeout: Duration,
}

impl Default for IpcServerConfig {
    fn default() -> Self {
        let socket_path = dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("talon")
            .join("talon.sock");

        Self {
            socket_path,
            max_connections: 100,
            read_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(30),
        }
    }
}

/// Handle to an active connection
struct ConnectionHandle {
    /// Sender for outbound messages
    sender: mpsc::Sender<CoreToChannel>,
    /// Channel ID if registered
    channel_id: Option<ChannelId>,
}

/// IPC server for channel connections
pub struct IpcServer {
    /// Server configuration
    config: IpcServerConfig,
    /// Message handler
    handler: Arc<dyn IpcMessageHandler>,
    /// Active connections (connection ID -> handle)
    connections: Arc<DashMap<String, ConnectionHandle>>,
    /// Channel ID to connection ID mapping
    channel_connections: Arc<DashMap<String, String>>,
    /// Whether the server is running
    running: Arc<AtomicBool>,
}

impl IpcServer {
    /// Create a new IPC server
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `handler` - Message handler implementation
    #[must_use]
    pub fn new(config: IpcServerConfig, handler: Arc<dyn IpcMessageHandler>) -> Self {
        Self {
            config,
            handler,
            connections: Arc::new(DashMap::new()),
            channel_connections: Arc::new(DashMap::new()),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the socket path
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.config.socket_path
    }

    /// Check if the server is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start the server
    ///
    /// # Errors
    ///
    /// Returns error if the server cannot be started.
    pub async fn start(&self) -> TalonResult<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(TalonError::Ipc {
                message: "server already running".to_string(),
            });
        }

        // Ensure socket directory exists
        if let Some(parent) = self.config.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove stale socket
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        // Bind the listener
        let listener =
            UnixListener::bind(&self.config.socket_path).map_err(|e| TalonError::Ipc {
                message: format!("failed to bind socket: {e}"),
            })?;

        self.running.store(true, Ordering::SeqCst);
        info!(socket_path = %self.config.socket_path.display(), "IPC server started");

        // Spawn accept loop
        let running = Arc::clone(&self.running);
        let connections = Arc::clone(&self.connections);
        let channel_connections = Arc::clone(&self.channel_connections);
        let handler = Arc::clone(&self.handler);
        let max_connections = self.config.max_connections;

        tokio::spawn(async move {
            Self::accept_loop(
                listener,
                running,
                connections,
                channel_connections,
                handler,
                max_connections,
            )
            .await;
        });

        Ok(())
    }

    /// Stop the server
    pub async fn stop(&self) -> TalonResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("stopping IPC server");
        self.running.store(false, Ordering::SeqCst);

        // Close all connections
        self.connections.clear();
        self.channel_connections.clear();

        // Remove socket file
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        info!("IPC server stopped");
        Ok(())
    }

    /// Send a message to a specific channel
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel to send to
    /// * `message` - The message to send
    ///
    /// # Errors
    ///
    /// Returns error if the channel is not connected or send fails.
    pub async fn send_to_channel(
        &self,
        channel_id: &ChannelId,
        message: CoreToChannel,
    ) -> TalonResult<()> {
        let conn_id = self
            .channel_connections
            .get(channel_id.as_str())
            .map(|r| r.value().clone())
            .ok_or_else(|| TalonError::Channel {
                channel: channel_id.to_string(),
                message: "channel not connected".to_string(),
            })?;

        let handle = self
            .connections
            .get(&conn_id)
            .ok_or_else(|| TalonError::Channel {
                channel: channel_id.to_string(),
                message: "connection not found".to_string(),
            })?;

        handle
            .sender
            .send(message)
            .await
            .map_err(|e| TalonError::Ipc {
                message: format!("failed to send to channel {channel_id}: {e}"),
            })
    }

    /// Accept loop for incoming connections
    async fn accept_loop(
        listener: UnixListener,
        running: Arc<AtomicBool>,
        connections: Arc<DashMap<String, ConnectionHandle>>,
        channel_connections: Arc<DashMap<String, String>>,
        handler: Arc<dyn IpcMessageHandler>,
        max_connections: usize,
    ) {
        while running.load(Ordering::SeqCst) {
            // Check connection limit
            if connections.len() >= max_connections {
                warn!(max = max_connections, "connection limit reached, waiting");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            match listener.accept().await {
                Ok((stream, _addr)) => {
                    // Generate unique connection ID using timestamp + counter
                    let conn_id = format!(
                        "conn_{}_{:x}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0),
                        connections.len()
                    );
                    debug!(connection_id = %conn_id, "accepted connection");

                    // Create outbound message channel
                    let (tx, rx) = mpsc::channel::<CoreToChannel>(100);

                    // Store connection handle
                    connections.insert(
                        conn_id.clone(),
                        ConnectionHandle {
                            sender: tx,
                            channel_id: None,
                        },
                    );

                    // Spawn connection handler
                    let connections_clone = Arc::clone(&connections);
                    let channel_connections_clone = Arc::clone(&channel_connections);
                    let handler_clone = Arc::clone(&handler);
                    let conn_id_clone = conn_id.clone();

                    tokio::spawn(async move {
                        Self::handle_connection(
                            stream,
                            conn_id_clone,
                            rx,
                            connections_clone,
                            channel_connections_clone,
                            handler_clone,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    if running.load(Ordering::SeqCst) {
                        error!(error = %e, "failed to accept connection");
                    }
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(
        stream: UnixStream,
        conn_id: String,
        mut outbound_rx: mpsc::Receiver<CoreToChannel>,
        connections: Arc<DashMap<String, ConnectionHandle>>,
        channel_connections: Arc<DashMap<String, String>>,
        handler: Arc<dyn IpcMessageHandler>,
    ) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        loop {
            tokio::select! {
                // Handle inbound messages
                result = receive_message::<ChannelToCore>(&mut reader) => {
                    match result {
                        Ok(message) => {
                            // Extract channel ID for registration tracking
                            let channel_id = match &message {
                                ChannelToCore::Register { channel_id } => Some(channel_id.clone()),
                                ChannelToCore::Authenticate { channel_id, .. } => Some(channel_id.clone()),
                                ChannelToCore::Disconnect { channel_id } => {
                                    // Remove channel mapping
                                    channel_connections.remove(channel_id.as_str());
                                    None
                                }
                                ChannelToCore::UserMessage { sender, .. } => Some(sender.channel_id.clone()),
                            };

                            // Handle the message
                            match handler.handle(message).await {
                                Ok(response) => {
                                    // Update channel connection mapping on successful registration
                                    if let CoreToChannel::Registered { channel_id: ref cid } = response {
                                        channel_connections.insert(cid.to_string(), conn_id.clone());
                                        if let Some(mut handle) = connections.get_mut(&conn_id) {
                                            handle.channel_id = Some(cid.clone());
                                        }
                                    }

                                    if let Err(e) = send_message(&mut writer, &response).await {
                                        error!(connection_id = %conn_id, error = %e, "failed to send response");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!(connection_id = %conn_id, error = %e, "handler error");
                                    let error_response = CoreToChannel::Error {
                                        correlation_id: crate::types::CorrelationId::new(),
                                        message: e.to_string(),
                                    };
                                    if let Err(send_err) = send_message(&mut writer, &error_response).await {
                                        error!(connection_id = %conn_id, error = %send_err, "failed to send error");
                                        break;
                                    }
                                }
                            }

                            // If this was a disconnect message, close the connection
                            if channel_id.is_some() {
                                if let Some(cid) = channel_id {
                                    if !handler.is_authenticated(&cid) {
                                        // Channel disconnected, close connection
                                        debug!(connection_id = %conn_id, channel_id = %cid, "channel disconnected");
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if !matches!(e.kind(), std::io::ErrorKind::UnexpectedEof) {
                                error!(connection_id = %conn_id, error = %e, "receive error");
                            }
                            break;
                        }
                    }
                }

                // Handle outbound messages
                Some(message) = outbound_rx.recv() => {
                    if let Err(e) = send_message(&mut writer, &message).await {
                        error!(connection_id = %conn_id, error = %e, "failed to send outbound message");
                        break;
                    }
                }
            }
        }

        // Clean up connection
        debug!(connection_id = %conn_id, "connection closed");

        // Remove from connections map
        if let Some((_, handle)) = connections.remove(&conn_id) {
            // Remove channel mapping if present
            if let Some(channel_id) = handle.channel_id {
                channel_connections.remove(channel_id.as_str());
            }
        }
    }
}

/// Receive a length-prefixed message
async fn receive_message<M: DeserializeOwned>(
    reader: &mut BufReader<OwnedReadHalf>,
) -> std::io::Result<M> {
    // Read length prefix
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Validate size
    if len > MAX_MESSAGE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {len}"),
        ));
    }

    if len == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "empty message",
        ));
    }

    // Read payload
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;

    // Deserialize
    serde_json::from_slice(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Send a length-prefixed message
async fn send_message<M: Serialize>(
    writer: &mut BufWriter<OwnedWriteHalf>,
    message: &M,
) -> std::io::Result<()> {
    let json = serde_json::to_vec(message)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if json.len() > MAX_MESSAGE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {}", json.len()),
        ));
    }

    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::DefaultIpcHandler;
    use crate::ipc::TokenAuthenticator;

    fn test_handler() -> Arc<dyn IpcMessageHandler> {
        let authenticator = TokenAuthenticator::new(b"test-secret");
        Arc::new(DefaultIpcHandler::new(authenticator))
    }

    #[test]
    fn test_default_config() {
        let config = IpcServerConfig::default();
        assert!(config.socket_path.to_string_lossy().contains("talon.sock"));
        assert_eq!(config.max_connections, 100);
    }

    #[tokio::test]
    async fn test_server_not_running_initially() {
        let config = IpcServerConfig::default();
        let server = IpcServer::new(config, test_handler());
        assert!(!server.is_running());
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let mut config = IpcServerConfig::default();
        config.socket_path = PathBuf::from("/tmp/talon-test-server.sock");

        let server = IpcServer::new(config.clone(), test_handler());

        // Start
        server.start().await.expect("failed to start server");
        assert!(server.is_running());
        assert!(config.socket_path.exists());

        // Stop
        server.stop().await.expect("failed to stop server");
        assert!(!server.is_running());
        assert!(!config.socket_path.exists());
    }

    #[tokio::test]
    async fn test_server_double_start_fails() {
        let mut config = IpcServerConfig::default();
        config.socket_path = PathBuf::from("/tmp/talon-test-double-start.sock");

        let server = IpcServer::new(config.clone(), test_handler());

        server.start().await.expect("first start should succeed");

        let result = server.start().await;
        assert!(result.is_err());

        server.stop().await.ok();
    }
}
