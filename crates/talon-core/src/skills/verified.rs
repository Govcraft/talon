//! Verified skill types
//!
//! Wraps acton-ai's LoadedSkill with security verification metadata
//! including attestation claims and OmniBOR integrity verification.

use acton_ai::skills::LoadedSkill;
use agent_uri::AgentUri;
use agent_uri_attestation::AttestationClaims;
use chrono::{DateTime, Utc};
use mti::prelude::*;
use omnibor::hash_algorithm::Sha256;
use omnibor::ArtifactId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

use crate::skills::error::{SkillSecurityError, SkillSecurityResult};
use crate::skills::CapabilityPath;
use crate::trust::TrustTier;

/// Unique identifier for a skill in the registry
///
/// Uses TypeID format for human-readable, time-sortable, globally unique IDs.
/// Example: `skill_01h455vb4pex5vsknk084sn02q`
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SkillId(MagicTypeId);

/// Error returned when attempting to create an invalid skill ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidSkillId {
    /// TypeID parsing failed
    Parse(String),
    /// Wrong prefix (expected "skill")
    WrongPrefix {
        /// The expected prefix
        expected: &'static str,
        /// The actual prefix found
        actual: String,
    },
}

impl fmt::Display for InvalidSkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "invalid skill ID: {e}"),
            Self::WrongPrefix { expected, actual } => {
                write!(f, "expected prefix '{expected}', got '{actual}'")
            }
        }
    }
}

impl std::error::Error for InvalidSkillId {}

impl SkillId {
    /// The TypeID prefix for skill identifiers
    pub const PREFIX: &'static str = "skill";

    /// Creates a new skill ID with a fresh UUIDv7 (time-sortable)
    #[must_use]
    pub fn new() -> Self {
        Self(Self::PREFIX.create_type_id::<V7>())
    }

    /// Parses a skill ID from a string, validating the prefix
    ///
    /// # Errors
    ///
    /// Returns `InvalidSkillId::Parse` if the string is not a valid TypeID format.
    /// Returns `InvalidSkillId::WrongPrefix` if the TypeID has a different prefix.
    pub fn parse(s: &str) -> Result<Self, InvalidSkillId> {
        let id = MagicTypeId::from_str(s).map_err(|e| InvalidSkillId::Parse(e.to_string()))?;

        let prefix = id.prefix().as_str();
        if prefix != Self::PREFIX {
            return Err(InvalidSkillId::WrongPrefix {
                expected: Self::PREFIX,
                actual: prefix.to_string(),
            });
        }

        Ok(Self(id))
    }

    /// Returns a reference to the underlying MagicTypeId
    #[must_use]
    pub fn inner(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Default for SkillId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SkillId {
    type Err = InvalidSkillId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl AsRef<MagicTypeId> for SkillId {
    fn as_ref(&self) -> &MagicTypeId {
        &self.0
    }
}

impl Serialize for SkillId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SkillId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A skill that has been verified through attestation and integrity checks
///
/// This struct wraps a [`LoadedSkill`] from acton-ai with additional
/// security metadata proving the skill has been properly attested
/// and its integrity verified via OmniBOR.
#[derive(Clone, Debug)]
pub struct VerifiedSkill {
    /// The underlying skill from acton-ai
    pub skill: LoadedSkill,

    /// Unique identifier for this verified skill
    pub id: SkillId,

    /// The agent URI identifying this skill
    pub agent_uri: AgentUri,

    /// The verified attestation claims
    pub attestation: AttestationClaims,

    /// OmniBOR artifact ID computed from skill contents
    pub omnibor_id: ArtifactId<Sha256>,

    /// Capabilities granted by the attestation, mapped to capability paths
    pub capabilities: Vec<CapabilityPath>,

    /// Trust tier this skill operates at
    pub trust_tier: TrustTier,

    /// When the skill was verified
    pub verified_at: DateTime<Utc>,
}

impl VerifiedSkill {
    /// Create a new verified skill
    ///
    /// # Arguments
    ///
    /// * `skill` - The underlying loaded skill from acton-ai
    /// * `agent_uri` - The agent URI identifying this skill
    /// * `attestation` - The verified attestation claims
    /// * `omnibor_id` - The computed OmniBOR artifact ID
    /// * `capabilities` - Capabilities derived from attestation
    /// * `trust_tier` - The trust tier for this skill
    #[must_use]
    pub fn new(
        skill: LoadedSkill,
        agent_uri: AgentUri,
        attestation: AttestationClaims,
        omnibor_id: ArtifactId<Sha256>,
        capabilities: Vec<CapabilityPath>,
        trust_tier: TrustTier,
    ) -> Self {
        Self {
            skill,
            id: SkillId::new(),
            agent_uri,
            attestation,
            omnibor_id,
            capabilities,
            trust_tier,
            verified_at: Utc::now(),
        }
    }

    /// Get the skill name
    #[must_use]
    pub fn name(&self) -> &str {
        self.skill.name()
    }

    /// Get the skill description
    #[must_use]
    pub fn description(&self) -> &str {
        self.skill.description()
    }

    /// Check if the skill has a specific capability
    #[must_use]
    pub fn has_capability(&self, requested: &CapabilityPath) -> bool {
        self.capabilities.iter().any(|cap| cap.grants(requested))
    }

    /// Check if the attestation has expired
    #[must_use]
    pub fn is_attestation_expired(&self) -> bool {
        self.attestation.is_expired()
    }

    /// Check if the attestation is expired at a specific time
    #[must_use]
    pub fn is_attestation_expired_at(&self, at: DateTime<Utc>) -> bool {
        self.attestation.is_expired_at(at)
    }

    /// Get the attestation expiration time
    #[must_use]
    pub fn attestation_expires_at(&self) -> DateTime<Utc> {
        self.attestation.exp
    }

    /// Get the OmniBOR ID as a string
    #[must_use]
    pub fn omnibor_id_string(&self) -> String {
        self.omnibor_id.to_string()
    }

    /// Create a builder for constructing verified skills
    #[must_use]
    pub fn builder() -> VerifiedSkillBuilder {
        VerifiedSkillBuilder::new()
    }
}

/// Builder for creating verified skills
#[derive(Default)]
pub struct VerifiedSkillBuilder {
    skill: Option<LoadedSkill>,
    agent_uri: Option<AgentUri>,
    attestation: Option<AttestationClaims>,
    omnibor_id: Option<ArtifactId<Sha256>>,
    capabilities: Vec<CapabilityPath>,
    trust_tier: Option<TrustTier>,
}

impl VerifiedSkillBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the underlying skill
    #[must_use]
    pub fn skill(mut self, skill: LoadedSkill) -> Self {
        self.skill = Some(skill);
        self
    }

    /// Set the agent URI
    #[must_use]
    pub fn agent_uri(mut self, uri: AgentUri) -> Self {
        self.agent_uri = Some(uri);
        self
    }

    /// Set the attestation claims
    #[must_use]
    pub fn attestation(mut self, claims: AttestationClaims) -> Self {
        self.attestation = Some(claims);
        self
    }

    /// Set the OmniBOR artifact ID
    #[must_use]
    pub fn omnibor_id(mut self, id: ArtifactId<Sha256>) -> Self {
        self.omnibor_id = Some(id);
        self
    }

    /// Add capabilities
    #[must_use]
    pub fn capabilities(mut self, caps: Vec<CapabilityPath>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Set the trust tier
    #[must_use]
    pub fn trust_tier(mut self, tier: TrustTier) -> Self {
        self.trust_tier = Some(tier);
        self
    }

    /// Build the verified skill
    ///
    /// # Errors
    ///
    /// Returns error if required fields are not set.
    pub fn build(self) -> SkillSecurityResult<VerifiedSkill> {
        let skill = self.skill.ok_or_else(|| SkillSecurityError::SkillLoadError {
            skill_name: "unknown".to_string(),
            reason: "skill is required for VerifiedSkill".to_string(),
        })?;

        let agent_uri = self.agent_uri.ok_or_else(|| SkillSecurityError::InvalidAgentUri {
            uri: "none".to_string(),
            reason: "agent_uri is required for VerifiedSkill".to_string(),
        })?;

        let attestation = self.attestation.ok_or_else(|| SkillSecurityError::AttestationNotFound {
            skill_name: skill.name().to_string(),
        })?;

        let omnibor_id = self.omnibor_id.ok_or_else(|| SkillSecurityError::OmniborError {
            message: "omnibor_id is required for VerifiedSkill".to_string(),
        })?;

        let trust_tier = self.trust_tier.ok_or_else(|| SkillSecurityError::TrustTierViolation {
            skill_name: skill.name().to_string(),
            required_tier: 0,
            actual_tier: 0,
        })?;

        Ok(VerifiedSkill::new(
            skill,
            agent_uri,
            attestation,
            omnibor_id,
            self.capabilities,
            trust_tier,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_id_generation() {
        let id1 = SkillId::new();
        let id2 = SkillId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_skill_id_prefix() {
        let id = SkillId::new();
        let id_str = id.to_string();
        assert!(id_str.starts_with("skill_"));
    }

    #[test]
    fn test_skill_id_serialization() {
        let id = SkillId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let parsed: SkillId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_skill_id_parse_valid() {
        let id = SkillId::new();
        let id_str = id.to_string();
        let parsed = SkillId::parse(&id_str);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap(), id);
    }

    #[test]
    fn test_skill_id_parse_wrong_prefix() {
        let result = SkillId::parse("conv_01h455vb4pex5vsknk084sn02q");
        assert!(result.is_err());
        match result.unwrap_err() {
            InvalidSkillId::WrongPrefix { expected, actual } => {
                assert_eq!(expected, "skill");
                assert_eq!(actual, "conv");
            }
            _ => panic!("expected WrongPrefix error"),
        }
    }
}
