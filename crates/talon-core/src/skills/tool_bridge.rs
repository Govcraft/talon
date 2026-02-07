//! Bridge between agent-skills and acton-ai tools
//!
//! Provides integration between verified skills and the acton-ai
//! tool execution system.

use std::sync::Arc;

use crate::skills::CapabilityPath;
use crate::skills::secure_executor::SecureToolExecutor;
use crate::skills::verified::{SkillId, VerifiedSkill};

/// A tool that has been bridged from a verified skill
///
/// This represents a tool that can be invoked through acton-ai's
/// tool calling mechanism, with capability verification.
#[derive(Clone, Debug)]
pub struct BridgedTool {
    /// Tool name as exposed to the LLM
    name: String,
    /// Tool description for the LLM
    description: String,
    /// The skill this tool belongs to
    skill_id: SkillId,
    /// Skill name for capability checks
    skill_name: String,
    /// Required capability for this tool
    required_capability: Option<CapabilityPath>,
}

impl BridgedTool {
    /// Create a new bridged tool
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        skill_id: SkillId,
        skill_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            skill_id,
            skill_name: skill_name.into(),
            required_capability: None,
        }
    }

    /// Set the required capability for this tool
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityPath) -> Self {
        self.required_capability = Some(capability);
        self
    }

    /// Get the tool name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the tool description
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get the skill ID this tool belongs to
    #[must_use]
    pub fn skill_id(&self) -> &SkillId {
        &self.skill_id
    }

    /// Get the skill name
    #[must_use]
    pub fn skill_name(&self) -> &str {
        &self.skill_name
    }

    /// Get the required capability
    #[must_use]
    pub fn required_capability(&self) -> Option<&CapabilityPath> {
        self.required_capability.as_ref()
    }
}

/// Bridge between verified skills and acton-ai tools
///
/// Creates tool definitions that can be used with acton-ai's
/// LLM tool calling, with capability verification.
pub struct ToolBridge {
    /// The executor for capability-verified tool calls
    executor: Arc<SecureToolExecutor>,
}

impl ToolBridge {
    /// Create a new tool bridge with an executor
    ///
    /// # Arguments
    ///
    /// * `executor` - The secure executor for tool calls
    #[must_use]
    pub fn new(executor: Arc<SecureToolExecutor>) -> Self {
        Self { executor }
    }

    /// Get a reference to the executor
    #[must_use]
    pub fn executor(&self) -> &Arc<SecureToolExecutor> {
        &self.executor
    }

    /// Create bridged tools from a verified skill
    ///
    /// This extracts the tools that a skill is allowed to use
    /// based on its capabilities and creates bridged tool
    /// definitions for acton-ai.
    ///
    /// # Arguments
    ///
    /// * `skill` - The verified skill to create tools from
    ///
    /// # Returns
    ///
    /// A vector of bridged tools for this skill.
    #[must_use]
    pub fn create_tools(&self, skill: &VerifiedSkill) -> Vec<BridgedTool> {
        let mut tools = Vec::new();

        // Map capabilities to available tools
        for capability in &skill.capabilities {
            if let Some(bridged_tools) = capability_to_tools(capability) {
                for (tool_name, description) in bridged_tools {
                    let tool =
                        BridgedTool::new(tool_name, description, skill.id.clone(), skill.name())
                            .with_capability(capability.clone());

                    tools.push(tool);
                }
            }
        }

        tools
    }

    /// Create tools for multiple skills
    ///
    /// # Arguments
    ///
    /// * `skills` - Iterator of verified skills
    ///
    /// # Returns
    ///
    /// A vector of all bridged tools from all skills.
    pub fn create_tools_for_many<'a>(
        &self,
        skills: impl IntoIterator<Item = &'a VerifiedSkill>,
    ) -> Vec<BridgedTool> {
        skills
            .into_iter()
            .flat_map(|skill| self.create_tools(skill))
            .collect()
    }

    /// Check if a skill can use a specific tool
    ///
    /// # Arguments
    ///
    /// * `skill_name` - The skill name
    /// * `tool_name` - The tool to check
    #[must_use]
    pub fn can_use_tool(&self, skill_name: &str, tool_name: &str) -> bool {
        self.executor.is_tool_allowed(skill_name, tool_name)
    }
}

/// Map a capability path to the tools it grants access to
///
/// Returns a list of (tool_name, description) pairs.
fn capability_to_tools(capability: &CapabilityPath) -> Option<Vec<(&'static str, &'static str)>> {
    let path = capability.as_str();

    Some(match path {
        "file/read" => vec![
            ("Read", "Read the contents of a file"),
            ("Glob", "Find files matching a glob pattern"),
            ("Grep", "Search file contents with regex"),
        ],
        "file/write" => vec![
            ("Write", "Write content to a file"),
            ("Edit", "Edit a file with search and replace"),
        ],
        "file" => vec![
            ("Read", "Read the contents of a file"),
            ("Glob", "Find files matching a glob pattern"),
            ("Grep", "Search file contents with regex"),
            ("Write", "Write content to a file"),
            ("Edit", "Edit a file with search and replace"),
        ],
        "shell/execute" => vec![("Bash", "Execute a shell command")],
        "shell/git" => vec![("Bash", "Execute git commands")],
        "shell/npm" => vec![("Bash", "Execute npm commands")],
        "network/http" => vec![
            ("WebFetch", "Fetch content from a URL"),
            ("HTTP", "Make HTTP requests"),
        ],
        "network" => vec![
            ("WebFetch", "Fetch content from a URL"),
            ("HTTP", "Make HTTP requests"),
        ],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::registry::{SecureSkillRegistry, SecureSkillRegistryConfig};

    fn test_executor() -> Arc<SecureToolExecutor> {
        let config = SecureSkillRegistryConfig {
            require_attestation: false,
            verify_integrity: false,
            ..Default::default()
        };
        let registry = Arc::new(
            SecureSkillRegistry::with_config(config).expect("registry should be creatable"),
        );
        Arc::new(SecureToolExecutor::new(registry))
    }

    #[test]
    fn test_bridged_tool_creation() {
        let tool = BridgedTool::new("Read", "Read file contents", SkillId::new(), "test-skill");

        assert_eq!(tool.name(), "Read");
        assert_eq!(tool.description(), "Read file contents");
        assert_eq!(tool.skill_name(), "test-skill");
        assert!(tool.required_capability().is_none());
    }

    #[test]
    fn test_bridged_tool_with_capability() {
        let tool = BridgedTool::new("Read", "Read file contents", SkillId::new(), "test-skill")
            .with_capability(CapabilityPath::file_read());

        assert!(tool.required_capability().is_some());
        assert_eq!(
            tool.required_capability()
                .expect("should have capability")
                .as_str(),
            "file/read"
        );
    }

    #[test]
    fn test_capability_to_tools_file_read() {
        let capability = CapabilityPath::file_read();
        let tools = capability_to_tools(&capability);

        assert!(tools.is_some());
        let tools = tools.expect("should have tools");
        assert_eq!(tools.len(), 3);

        let names: Vec<&str> = tools.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"Grep"));
    }

    #[test]
    fn test_capability_to_tools_file_write() {
        let capability = CapabilityPath::file_write();
        let tools = capability_to_tools(&capability);

        assert!(tools.is_some());
        let tools = tools.expect("should have tools");
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Edit"));
    }

    #[test]
    fn test_capability_to_tools_shell() {
        let capability = CapabilityPath::shell_execute();
        let tools = capability_to_tools(&capability);

        assert!(tools.is_some());
        let tools = tools.expect("should have tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "Bash");
    }

    #[test]
    fn test_capability_to_tools_unknown() {
        let capability = CapabilityPath::new("unknown/capability");
        let tools = capability_to_tools(&capability);

        assert!(tools.is_none());
    }

    #[test]
    fn test_tool_bridge_creation() {
        let executor = test_executor();
        let bridge = ToolBridge::new(executor);

        // Just verify it can be created
        assert!(Arc::strong_count(bridge.executor()) >= 1);
    }
}
