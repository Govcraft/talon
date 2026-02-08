use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("gateway unavailable: {0}")]
    GatewayUnavailable(String),

    #[error("registration failed: {0}")]
    RegistrationFailed(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("channel error: {0}")]
    Internal(String),
}

impl From<tonic::Status> for ChannelError {
    fn from(status: tonic::Status) -> Self {
        ChannelError::SendFailed(status.message().to_string())
    }
}

impl From<tonic::transport::Error> for ChannelError {
    fn from(err: tonic::transport::Error) -> Self {
        ChannelError::ConnectionFailed(err.to_string())
    }
}
