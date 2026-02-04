//! Router actor for IPC communication
//!
//! The router actor handles incoming IPC messages from channel binaries
//! and routes them to the appropriate conversation actors.

mod actor;

pub use actor::Router;
