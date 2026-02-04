//! Capability path definitions and tool mapping
//!
//! Provides capability path type and functions for mapping
//! acton-ai tool names to capability paths for access control.

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

    /// Parse a capability path from a string
    ///
    /// Returns None if the string is empty.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            None
        } else {
            Some(Self(s.to_string()))
        }
    }

    /// File system read capability
    #[must_use]
    pub fn file_read() -> Self {
        Self::new("file/read")
    }

    /// File system write capability
    #[must_use]
    pub fn file_write() -> Self {
        Self::new("file/write")
    }

    /// Shell execution capability (any command)
    #[must_use]
    pub fn shell_execute() -> Self {
        Self::new("shell/execute")
    }

    /// Git command execution capability
    #[must_use]
    pub fn shell_git() -> Self {
        Self::new("shell/git")
    }

    /// npm command execution capability
    #[must_use]
    pub fn shell_npm() -> Self {
        Self::new("shell/npm")
    }

    /// HTTP network access capability
    #[must_use]
    pub fn network_http() -> Self {
        Self::new("network/http")
    }

    /// Full network access capability
    #[must_use]
    pub fn network() -> Self {
        Self::new("network")
    }
}

impl fmt::Display for CapabilityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for CapabilityPath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CapabilityPath {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Map an acton-ai tool name to the required capability path
///
/// This is a pure function that maps tool names from SKILL.md
/// allowed-tools to capability paths.
///
/// # Arguments
///
/// * `tool_name` - The tool name as specified in allowed-tools
///
/// # Returns
///
/// The capability path required for this tool, or None if the tool
/// is not recognized.
#[must_use]
pub fn tool_to_capability(tool_name: &str) -> Option<CapabilityPath> {
    // Handle scoped bash tools like "Bash(git:*)" or "Bash(npm:*)"
    if let Some(scope) = parse_bash_scope(tool_name) {
        return Some(CapabilityPath::new(format!("shell/{scope}")));
    }

    match tool_name {
        // File system tools
        "Read" | "read_file" => Some(CapabilityPath::file_read()),
        "Glob" | "glob" => Some(CapabilityPath::file_read()),
        "Grep" | "grep" => Some(CapabilityPath::file_read()),
        "Write" | "write_file" => Some(CapabilityPath::file_write()),
        "Edit" | "edit_file" => Some(CapabilityPath::file_write()),

        // Shell execution
        "Bash" | "bash" => Some(CapabilityPath::shell_execute()),

        // Network tools
        "WebFetch" | "web_fetch" => Some(CapabilityPath::network_http()),
        "HTTP" | "http" => Some(CapabilityPath::network_http()),

        _ => None,
    }
}

/// Parse a scoped Bash tool specification
///
/// Handles formats like:
/// - "Bash(git:*)" -> "git"
/// - "Bash(npm:*)" -> "npm"
/// - "Bash(cargo:*)" -> "cargo"
fn parse_bash_scope(tool_name: &str) -> Option<&str> {
    if !tool_name.starts_with("Bash(") || !tool_name.ends_with(')') {
        return None;
    }

    let inner = &tool_name[5..tool_name.len() - 1];
    let colon_pos = inner.find(':')?;
    let scope = &inner[..colon_pos];

    if scope.is_empty() {
        None
    } else {
        Some(scope)
    }
}

/// Map multiple tool names to capability paths
///
/// # Arguments
///
/// * `tool_names` - Iterator of tool names
///
/// # Returns
///
/// Vector of unique capability paths required for all tools.
pub fn tools_to_capabilities<'a>(
    tool_names: impl IntoIterator<Item = &'a str>,
) -> Vec<CapabilityPath> {
    let mut capabilities: Vec<CapabilityPath> = tool_names
        .into_iter()
        .filter_map(tool_to_capability)
        .collect();

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    capabilities.retain(|cap| seen.insert(cap.clone()));

    capabilities
}

/// Check if the granted capabilities cover all required capabilities
///
/// # Arguments
///
/// * `granted` - Capabilities that have been granted
/// * `required` - Capabilities that are required
///
/// # Returns
///
/// List of capabilities that are not covered by granted capabilities.
#[must_use]
pub fn find_missing_capabilities<'a>(
    granted: &[CapabilityPath],
    required: &'a [CapabilityPath],
) -> Vec<&'a CapabilityPath> {
    required
        .iter()
        .filter(|req| !granted.iter().any(|g| g.grants(req)))
        .collect()
}

/// Check if granted capabilities cover all required capabilities
///
/// This is a pure function for capability coverage checking.
///
/// # Arguments
///
/// * `granted` - Capabilities that have been granted
/// * `required` - Capabilities that are required
///
/// # Returns
///
/// true if all required capabilities are covered, false otherwise.
#[must_use]
pub fn capabilities_cover(granted: &[CapabilityPath], required: &[CapabilityPath]) -> bool {
    find_missing_capabilities(granted, required).is_empty()
}

/// Parse capability strings from attestation claims
///
/// Converts string capabilities from attestation format to CapabilityPath.
pub fn parse_capabilities(capability_strings: &[String]) -> Vec<CapabilityPath> {
    capability_strings
        .iter()
        .filter_map(|s| CapabilityPath::parse(s))
        .collect()
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

    #[test]
    fn test_tool_to_capability_read() {
        let cap = tool_to_capability("Read");
        assert_eq!(cap, Some(CapabilityPath::file_read()));
    }

    #[test]
    fn test_tool_to_capability_write() {
        let cap = tool_to_capability("Write");
        assert_eq!(cap, Some(CapabilityPath::file_write()));
    }

    #[test]
    fn test_tool_to_capability_bash() {
        let cap = tool_to_capability("Bash");
        assert_eq!(cap, Some(CapabilityPath::shell_execute()));
    }

    #[test]
    fn test_tool_to_capability_scoped_bash() {
        let cap = tool_to_capability("Bash(git:*)");
        assert_eq!(cap, Some(CapabilityPath::new("shell/git")));
    }

    #[test]
    fn test_tool_to_capability_scoped_npm() {
        let cap = tool_to_capability("Bash(npm:*)");
        assert_eq!(cap, Some(CapabilityPath::new("shell/npm")));
    }

    #[test]
    fn test_tool_to_capability_unknown() {
        let cap = tool_to_capability("UnknownTool");
        assert!(cap.is_none());
    }

    #[test]
    fn test_tools_to_capabilities() {
        let tools = ["Read", "Glob", "Write", "Read"]; // Note duplicate
        let caps = tools_to_capabilities(tools.iter().copied());

        // Should have 2 unique capabilities (file/read and file/write)
        assert_eq!(caps.len(), 2);
        assert!(caps.contains(&CapabilityPath::file_read()));
        assert!(caps.contains(&CapabilityPath::file_write()));
    }

    #[test]
    fn test_find_missing_capabilities() {
        let granted = vec![CapabilityPath::file_read()];
        let required = vec![CapabilityPath::file_read(), CapabilityPath::file_write()];

        let missing = find_missing_capabilities(&granted, &required);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], &CapabilityPath::file_write());
    }

    #[test]
    fn test_capabilities_cover_success() {
        let granted = vec![CapabilityPath::new("file")]; // Grants file/*
        let required = vec![CapabilityPath::file_read(), CapabilityPath::file_write()];

        assert!(capabilities_cover(&granted, &required));
    }

    #[test]
    fn test_capabilities_cover_failure() {
        let granted = vec![CapabilityPath::file_read()];
        let required = vec![CapabilityPath::file_read(), CapabilityPath::file_write()];

        assert!(!capabilities_cover(&granted, &required));
    }

    #[test]
    fn test_parse_bash_scope() {
        assert_eq!(parse_bash_scope("Bash(git:*)"), Some("git"));
        assert_eq!(parse_bash_scope("Bash(npm:*)"), Some("npm"));
        assert_eq!(parse_bash_scope("Bash(cargo:build)"), Some("cargo"));
        assert_eq!(parse_bash_scope("Bash"), None);
        assert_eq!(parse_bash_scope("Read"), None);
    }

    #[test]
    fn test_parse_capabilities() {
        let strings = vec![
            "file/read".to_string(),
            "file/write".to_string(),
            "".to_string(), // Should be filtered
        ];
        let caps = parse_capabilities(&strings);
        assert_eq!(caps.len(), 2);
    }
}
