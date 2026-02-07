//! Low-level IPC connection with length-prefixed framing
//!
//! Provides a Unix Domain Socket connection with message framing:
//! - 4-byte big-endian length prefix
//! - JSON payload bytes
//! - Maximum message size: 16 MiB

use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use super::error::{IpcClientError, IpcClientResult};

/// Maximum message size (16 MiB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Read half of an IPC connection
pub struct IpcReader {
    reader: BufReader<OwnedReadHalf>,
}

/// Write half of an IPC connection
pub struct IpcWriter {
    writer: BufWriter<OwnedWriteHalf>,
}

/// Low-level IPC connection using Unix Domain Sockets
///
/// Uses length-prefixed framing with 4-byte big-endian length prefix
/// followed by JSON payload.
pub struct IpcConnection {
    reader: IpcReader,
    writer: IpcWriter,
}

impl std::fmt::Debug for IpcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcConnection")
            .field("reader", &"<IpcReader>")
            .field("writer", &"<IpcWriter>")
            .finish()
    }
}

impl IpcReader {
    /// Receive a message from the connection
    ///
    /// # Errors
    ///
    /// Returns error if receiving or deserialization fails.
    pub async fn receive<M: DeserializeOwned>(&mut self) -> IpcClientResult<M> {
        // Read length prefix (4 bytes, big-endian)
        let mut len_bytes = [0u8; 4];
        self.reader.read_exact(&mut len_bytes).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                IpcClientError::ConnectionClosed
            } else {
                IpcClientError::ReceiveFailed {
                    message: e.to_string(),
                }
            }
        })?;

        let len = u32::from_be_bytes(len_bytes) as usize;

        // Validate message size
        if len > MAX_MESSAGE_SIZE {
            return Err(IpcClientError::InvalidFrame {
                message: format!("message size {len} exceeds maximum {MAX_MESSAGE_SIZE}"),
            });
        }

        if len == 0 {
            return Err(IpcClientError::InvalidFrame {
                message: "empty message".to_string(),
            });
        }

        // Read payload
        let mut payload = vec![0u8; len];
        self.reader.read_exact(&mut payload).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                IpcClientError::ConnectionClosed
            } else {
                IpcClientError::ReceiveFailed {
                    message: e.to_string(),
                }
            }
        })?;

        // Deserialize JSON
        let message: M = serde_json::from_slice(&payload)?;

        Ok(message)
    }
}

impl IpcWriter {
    /// Send a message over the connection
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send (will be serialized to JSON)
    ///
    /// # Errors
    ///
    /// Returns error if serialization or sending fails.
    pub async fn send<M: Serialize>(&mut self, message: &M) -> IpcClientResult<()> {
        // Serialize to JSON
        let json = serde_json::to_vec(message)?;

        // Check message size
        if json.len() > MAX_MESSAGE_SIZE {
            return Err(IpcClientError::MessageTooLarge {
                size: json.len(),
                max_size: MAX_MESSAGE_SIZE,
            });
        }

        // Write length prefix (4 bytes, big-endian)
        let len = u32::try_from(json.len()).map_err(|_| IpcClientError::MessageTooLarge {
            size: json.len(),
            max_size: MAX_MESSAGE_SIZE,
        })?;

        self.writer
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| IpcClientError::SendFailed {
                message: e.to_string(),
            })?;

        // Write payload
        self.writer
            .write_all(&json)
            .await
            .map_err(|e| IpcClientError::SendFailed {
                message: e.to_string(),
            })?;

        // Flush to ensure message is sent
        self.writer
            .flush()
            .await
            .map_err(|e| IpcClientError::SendFailed {
                message: e.to_string(),
            })?;

        Ok(())
    }

    /// Shutdown the write half
    pub async fn shutdown(&mut self) -> IpcClientResult<()> {
        self.writer
            .shutdown()
            .await
            .map_err(|e| IpcClientError::SendFailed {
                message: format!("shutdown failed: {e}"),
            })
    }
}

impl IpcConnection {
    /// Connect to an IPC socket
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path to the Unix Domain Socket
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub async fn connect(socket_path: &Path) -> IpcClientResult<Self> {
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            IpcClientError::ConnectionFailed {
                socket_path: socket_path.to_path_buf(),
                message: e.to_string(),
            }
        })?;

        let (read_half, write_half) = stream.into_split();

        Ok(Self {
            reader: IpcReader {
                reader: BufReader::new(read_half),
            },
            writer: IpcWriter {
                writer: BufWriter::new(write_half),
            },
        })
    }

    /// Split the connection into separate reader and writer
    pub fn split(self) -> (IpcReader, IpcWriter) {
        (self.reader, self.writer)
    }

    /// Send a message over the connection
    pub async fn send<M: Serialize>(&mut self, message: &M) -> IpcClientResult<()> {
        self.writer.send(message).await
    }

    /// Receive a message from the connection
    pub async fn receive<M: DeserializeOwned>(&mut self) -> IpcClientResult<M> {
        self.reader.receive().await
    }

    /// Shutdown the connection
    pub async fn shutdown(&mut self) -> IpcClientResult<()> {
        self.writer.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;
    use tokio::net::UnixListener;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestMessage {
        content: String,
        number: u32,
    }

    #[tokio::test]
    async fn test_connection_to_nonexistent_socket() {
        let result = IpcConnection::connect(Path::new("/nonexistent/path.sock")).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IpcClientError::ConnectionFailed { .. }
        ));
    }

    #[tokio::test]
    async fn test_send_receive_message() {
        // Create a temporary socket
        let socket_path = PathBuf::from("/tmp/talon-test-ipc.sock");
        let _ = std::fs::remove_file(&socket_path);

        // Start a server
        let listener = UnixListener::bind(&socket_path).expect("failed to bind");

        // Spawn server task
        let server_handle = tokio::spawn({
            let socket_path = socket_path.clone();
            async move {
                let (stream, _) = listener.accept().await.expect("failed to accept");
                let (read_half, write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut writer = BufWriter::new(write_half);

                // Read message from client
                let mut len_bytes = [0u8; 4];
                reader
                    .read_exact(&mut len_bytes)
                    .await
                    .expect("failed to read len");
                let len = u32::from_be_bytes(len_bytes) as usize;
                let mut payload = vec![0u8; len];
                reader
                    .read_exact(&mut payload)
                    .await
                    .expect("failed to read payload");
                let msg: TestMessage =
                    serde_json::from_slice(&payload).expect("failed to deserialize");

                // Echo back with modified content
                let response = TestMessage {
                    content: format!("echo: {}", msg.content),
                    number: msg.number + 1,
                };
                let json = serde_json::to_vec(&response).expect("failed to serialize");
                writer
                    .write_all(&(json.len() as u32).to_be_bytes())
                    .await
                    .expect("failed to write len");
                writer
                    .write_all(&json)
                    .await
                    .expect("failed to write payload");
                writer.flush().await.expect("failed to flush");

                // Clean up
                drop(writer);
                drop(reader);
                drop(listener);
                let _ = std::fs::remove_file(&socket_path);
            }
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect client
        let mut conn = IpcConnection::connect(&socket_path)
            .await
            .expect("failed to connect");

        // Send message
        let msg = TestMessage {
            content: "hello".to_string(),
            number: 42,
        };
        conn.send(&msg).await.expect("failed to send");

        // Receive response
        let response: TestMessage = conn.receive().await.expect("failed to receive");
        assert_eq!(response.content, "echo: hello");
        assert_eq!(response.number, 43);

        // Clean up
        conn.shutdown().await.ok();
        server_handle.await.ok();
    }

    #[test]
    fn test_max_message_size_constant() {
        assert_eq!(MAX_MESSAGE_SIZE, 16 * 1024 * 1024);
    }
}
