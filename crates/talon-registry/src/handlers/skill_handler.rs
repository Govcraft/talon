//! Skill request handler

use crate::error::RegistryResult;
use crate::models::Skill;

/// Handler for skill-related requests
pub struct SkillHandler;

impl SkillHandler {
    /// Create a new skill handler
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Get a skill by ID
    ///
    /// # Errors
    ///
    /// Returns error if skill not found
    pub async fn get_skill(&self, _skill_id: &str) -> RegistryResult<Skill> {
        // Stub: Will query database
        todo!("Implement skill lookup")
    }

    /// List all skills
    ///
    /// # Errors
    ///
    /// Returns error on database failure
    pub async fn list_skills(&self) -> RegistryResult<Vec<Skill>> {
        // Stub: Will query database
        Ok(vec![])
    }
}

impl Default for SkillHandler {
    fn default() -> Self {
        Self::new()
    }
}
