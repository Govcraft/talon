//! IPC message types for channel communication
//!
//! Defines the protocol for communication between channel binaries
//! and the core daemon over Unix Domain Sockets.

mod messages;

pub use messages::*;
