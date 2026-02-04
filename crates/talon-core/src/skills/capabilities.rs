//! Capability path definitions

use serde::{Deserialize, Serialize};
use std::fmt;

/// A capability path representing allowed operations
///
/// Follows the format: "category/subcategory/action"
/// Examples:
/// - "file/read"
/// - "file/write"
/// - "network/http/get"
/// - "shell/execute"
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityPath(String);

impl CapabilityPath {
    /// Create a new capability path
    ///
    /// # Panics
    ///
    /// Panics if the path is empty
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        assert!(!path.is_empty(), "capability path cannot be empty");
        Self(path)
    }

    /// Get the path as a string slice
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this capability grants the requested capability
    ///
    /// A capability grants another if it is equal or a prefix.
    /// For example, "file" grants "file/read".
    #[must_use]
    pub fn grants(&self, requested: &Self) -> bool {
        requested.0.starts_with(&self.0)
            && (requested.0.len() == self.0.len()
                || requested.0.chars().nth(self.0.len()) == Some('/'))
    }
}

impl fmt::Display for CapabilityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_grants_exact() {
        let cap = CapabilityPath::new("file/read");
        let requested = CapabilityPath::new("file/read");
        assert!(cap.grants(&requested));
    }

    #[test]
    fn test_capability_grants_prefix() {
        let cap = CapabilityPath::new("file");
        let requested = CapabilityPath::new("file/read");
        assert!(cap.grants(&requested));
    }

    #[test]
    fn test_capability_does_not_grant_unrelated() {
        let cap = CapabilityPath::new("file/read");
        let requested = CapabilityPath::new("network/http");
        assert!(!cap.grants(&requested));
    }

    #[test]
    fn test_capability_does_not_grant_partial_match() {
        let cap = CapabilityPath::new("file");
        let requested = CapabilityPath::new("filesystem/read");
        assert!(!cap.grants(&requested));
    }
}
