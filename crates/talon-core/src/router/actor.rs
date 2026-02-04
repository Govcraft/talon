//! Router actor implementation

use acton_reactive::prelude::*;

/// Router actor state
#[acton_actor]
#[derive(Default)]
pub struct Router {
    /// Number of active connections
    active_connections: usize,
}

impl Router {
    /// Get the number of active connections
    #[must_use]
    pub fn active_connections(&self) -> usize {
        self.active_connections
    }

    /// Increment active connection count
    pub fn add_connection(&mut self) {
        self.active_connections += 1;
    }

    /// Decrement active connection count
    pub fn remove_connection(&mut self) {
        self.active_connections = self.active_connections.saturating_sub(1);
    }
}
