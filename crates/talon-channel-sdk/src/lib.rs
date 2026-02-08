//! Channel service SDK for the Talon AI gateway.
//!
//! Provides the `ChannelHandler` trait for implementing channel services,
//! a `GatewayClient` for communicating with the gateway, and generated
//! gRPC stubs.

pub mod client;
pub mod error;
pub mod handler;
pub mod proto;

pub use client::GatewayClient;
pub use error::ChannelError;
pub use handler::ChannelHandler;
