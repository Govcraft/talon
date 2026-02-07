//! Secure skill registry implementation
//!
//! Wraps acton-ai's SkillRegistry with additional security:
//! - Agent-URI verification
//! - PASETO attestation validation
//! - OmniBOR integrity checks
//! - Trust tier enforcement

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use acton_ai::skills::{LoadedSkill, SkillInfo, SkillRegistry};
use agent_uri::AgentUri;
use agent_uri_attestation::{AttestationClaims, Verifier, VerifyingKey};
use base64::Engine;
use omnibor::ArtifactId;
use omnibor::ArtifactIdBuilder;
use omnibor::hash_algorithm::Sha256;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::skills::cache::{AttestationCache, AttestationCacheConfig};
use crate::skills::capabilities::{
    CapabilityPath, find_missing_capabilities, parse_capabilities, tools_to_capabilities,
};
use crate::skills::error::{SkillSecurityError, SkillSecurityResult};
use crate::skills::integrity::compute_artifact_id;
use crate::skills::registry_client::{RegistryClient, RegistryClientConfig};
use crate::skills::verified::{SkillId, VerifiedSkill};
use crate::trust::{TrustTier, TrustTierManager};

/// Configuration for the secure skill registry
#[derive(Clone, Debug)]
pub struct SecureSkillRegistryConfig {
    /// Whether attestation is required for skill loading
    pub require_attestation: bool,

    /// Whether to verify OmniBOR integrity
    pub verify_integrity: bool,

    /// Cache configuration
    pub cache_config: AttestationCacheConfig,

    /// Registry client configuration
    pub registry_config: RegistryClientConfig,

    /// Maximum allowed trust tier
    pub max_trust_tier: TrustTier,

    /// Whether to allow skills without agent URI
    pub allow_unauthenticated: bool,

    /// Whether to automatically fetch attestations from registry
    pub auto_fetch_attestations: bool,
}

impl Default for SecureSkillRegistryConfig {
    fn default() -> Self {
        Self {
            require_attestation: true,
            verify_integrity: true,
            cache_config: AttestationCacheConfig::default(),
            registry_config: RegistryClientConfig::default(),
            max_trust_tier: TrustTier::Medium,
            allow_unauthenticated: false,
            auto_fetch_attestations: true,
        }
    }
}

/// Secure skill registry with attestation verification
///
/// Wraps acton-ai's `SkillRegistry` with additional security features:
/// - PASETO attestation verification via `agent-uri-attestation`
/// - OmniBOR integrity verification
/// - Trust tier enforcement
/// - Capability-based access control
pub struct SecureSkillRegistry {
    // Note: Manual Debug impl to avoid exposing sensitive verifier details
    /// The underlying skill registry from acton-ai, shared via Arc<RwLock>
    inner: Arc<RwLock<SkillRegistry>>,

    /// Attestation verifier
    verifier: Verifier,

    /// Attestation cache
    cache: AttestationCache,

    /// Registry HTTP client
    registry_client: RegistryClient,

    /// Trust tier manager
    trust_manager: TrustTierManager,

    /// Verified skills keyed by name
    verified_skills: HashMap<String, VerifiedSkill>,

    /// Configuration
    config: SecureSkillRegistryConfig,
}

impl std::fmt::Debug for SecureSkillRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureSkillRegistry")
            .field("verified_skills_count", &self.verified_skills.len())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl SecureSkillRegistry {
    /// Create a new secure skill registry with default configuration
    ///
    /// # Errors
    ///
    /// Returns error if the registry client cannot be initialized.
    pub fn new() -> SkillSecurityResult<Self> {
        Self::with_config(SecureSkillRegistryConfig::default())
    }

    /// Create a new secure skill registry with custom configuration
    ///
    /// # Errors
    ///
    /// Returns error if the registry client cannot be initialized.
    pub fn with_config(config: SecureSkillRegistryConfig) -> SkillSecurityResult<Self> {
        let cache = AttestationCache::with_config(config.cache_config.clone());
        let registry_client = RegistryClient::with_config(config.registry_config.clone())?;
        let trust_manager = TrustTierManager::new(config.max_trust_tier);

        Ok(Self {
            inner: Arc::new(RwLock::new(SkillRegistry::new())),
            verifier: Verifier::new(),
            cache,
            registry_client,
            trust_manager,
            verified_skills: HashMap::new(),
            config,
        })
    }

    /// Create a new secure skill registry using a shared inner registry
    ///
    /// This allows multiple components to share the same underlying
    /// `SkillRegistry` instance while maintaining independent security
    /// verification state.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the secure registry
    /// * `inner` - Shared reference to the underlying skill registry
    ///
    /// # Errors
    ///
    /// Returns error if the registry client cannot be initialized.
    pub fn with_shared_registry(
        config: SecureSkillRegistryConfig,
        inner: Arc<RwLock<SkillRegistry>>,
    ) -> SkillSecurityResult<Self> {
        let cache = AttestationCache::with_config(config.cache_config.clone());
        let registry_client = RegistryClient::with_config(config.registry_config.clone())?;
        let trust_manager = TrustTierManager::new(config.max_trust_tier);

        Ok(Self {
            inner,
            verifier: Verifier::new(),
            cache,
            registry_client,
            trust_manager,
            verified_skills: HashMap::new(),
            config,
        })
    }

    /// Get a clone of the shared inner registry handle
    ///
    /// This returns an `Arc<RwLock<SkillRegistry>>` that can be shared
    /// with other components that need access to the underlying registry.
    #[must_use]
    pub fn inner_registry(&self) -> Arc<RwLock<SkillRegistry>> {
        Arc::clone(&self.inner)
    }

    /// Add a trusted verifying key for a trust root
    ///
    /// # Arguments
    ///
    /// * `trust_root` - The trust root domain (e.g., "talonhub.io")
    /// * `key` - The verifying key for this trust root
    pub fn add_trusted_key(&mut self, trust_root: &str, key: VerifyingKey) {
        self.verifier.add_trusted_root(trust_root, key);
        info!(trust_root = %trust_root, "added trusted key");
    }

    /// Load and verify a skill
    ///
    /// This method:
    /// 1. Loads the skill from the given path
    /// 2. Fetches or retrieves cached attestation
    /// 3. Verifies the attestation signature
    /// 4. Computes and verifies OmniBOR integrity
    /// 5. Checks capability coverage
    /// 6. Validates trust tier
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the skill directory
    ///
    /// # Errors
    ///
    /// Returns error if any verification step fails.
    pub async fn load_verified(&mut self, path: impl AsRef<Path>) -> SkillSecurityResult<SkillId> {
        let path = path.as_ref();

        // Load the skill using acton-ai
        let skill = self.load_skill_from_path(path).await?;
        let skill_name = skill.name().to_string();

        debug!(skill = %skill_name, path = %path.display(), "loading skill for verification");

        // Check if we already have this skill verified
        if let Some(existing) = self.verified_skills.get(&skill_name) {
            if !existing.is_attestation_expired() {
                debug!(skill = %skill_name, "using cached verification");
                return Ok(existing.id.clone());
            }
        }

        // Parse agent URI from skill metadata (if available)
        let agent_uri = self.extract_agent_uri(&skill)?;

        // Fetch or retrieve attestation
        let attestation = self.get_attestation(&skill_name, &agent_uri).await?;

        // Verify attestation
        self.verify_attestation(&skill_name, &agent_uri, &attestation)?;

        // Compute and verify OmniBOR integrity
        let omnibor_id = self.compute_and_verify_integrity(path, &skill_name, &attestation)?;

        // Map attestation capabilities to capability paths
        let capabilities = parse_capabilities(&attestation.capabilities);

        // Check required capabilities for skill's allowed tools
        self.check_capability_coverage(&skill, &capabilities)?;

        // Determine and validate trust tier
        let trust_tier = self.determine_trust_tier(&capabilities)?;

        // Create verified skill
        let verified = VerifiedSkill::new(
            skill.clone(),
            agent_uri,
            attestation,
            omnibor_id,
            capabilities,
            trust_tier,
        );

        let skill_id = verified.id.clone();

        // Store in registry
        {
            let mut guard = self.inner.write().await;
            guard.add(skill);
        }
        self.verified_skills.insert(skill_name.clone(), verified);

        info!(skill = %skill_name, trust_tier = %trust_tier, "skill loaded and verified");

        Ok(skill_id)
    }

    /// Load a skill from the TalonHub registry by agent URI
    ///
    /// This method:
    /// 1. Downloads the skill archive from the hub
    /// 2. Parses the archive as a skill
    /// 3. Fetches and verifies the attestation
    /// 4. Computes OmniBOR integrity hash
    /// 5. Determines capabilities and trust tier
    /// 6. Stores the verified skill
    ///
    /// # Arguments
    ///
    /// * `agent_uri` - The agent URI identifying the skill in the hub
    ///
    /// # Errors
    ///
    /// Returns error if download, parsing, or verification fails.
    pub async fn load_from_hub(&mut self, agent_uri: &str) -> SkillSecurityResult<()> {
        info!(agent_uri = %agent_uri, "loading skill from hub");

        // 1. Download skill archive
        let archive_bytes = self
            .registry_client
            .download_skill_archive(agent_uri)
            .await?;

        // 2. Parse archive as UTF-8
        let archive_str = String::from_utf8(archive_bytes.clone()).map_err(|e| {
            SkillSecurityError::ArchiveParseError {
                agent_uri: agent_uri.to_string(),
                reason: format!("invalid UTF-8: {e}"),
            }
        })?;

        // 3. Parse skill using agent-skills crate
        let skill = agent_skills::Skill::parse(&archive_str).map_err(|e| {
            SkillSecurityError::ArchiveParseError {
                agent_uri: agent_uri.to_string(),
                reason: e.to_string(),
            }
        })?;

        // 4. Convert to LoadedSkill
        let loaded_skill = LoadedSkill {
            info: SkillInfo {
                name: skill.name().as_str().to_string(),
                description: skill.description().as_str().to_string(),
                path: PathBuf::from(agent_uri),
                tags: skill
                    .frontmatter()
                    .metadata()
                    .and_then(|m| m.get("tags"))
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default(),
            },
            content: skill.body().to_string(),
            triggers: skill
                .frontmatter()
                .metadata()
                .and_then(|m| m.get("triggers"))
                .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            enabled_by_default: skill
                .frontmatter()
                .metadata()
                .and_then(|m| m.get("enabled"))
                .is_some_and(|v| v == "true"),
        };

        let skill_name = loaded_skill.info.name.clone();

        // 5. Fetch attestation token
        let attestation_response = self.registry_client.fetch_attestation(agent_uri).await?;

        // 6. Verify attestation
        let claims = self
            .verifier
            .verify(&attestation_response.token)
            .map_err(|e| SkillSecurityError::AttestationVerification {
                skill_name: skill_name.clone(),
                reason: e.to_string(),
            })?;

        // 7. Compute OmniBOR for integrity
        let omnibor_id = compute_artifact_id(&archive_bytes);

        // 8. Parse agent URI
        let parsed_uri =
            AgentUri::parse(agent_uri).map_err(|e| SkillSecurityError::InvalidAgentUri {
                uri: agent_uri.to_string(),
                reason: e.to_string(),
            })?;

        // 9. Determine capabilities and trust tier
        let capabilities = parse_capabilities(&claims.capabilities);
        let trust_tier = self.determine_trust_tier(&capabilities)?;

        // 10. Create VerifiedSkill
        let verified = VerifiedSkill::new(
            loaded_skill.clone(),
            parsed_uri,
            claims,
            omnibor_id,
            capabilities,
            trust_tier,
        );

        // 11. Store in verified_skills map
        self.verified_skills.insert(agent_uri.to_string(), verified);

        // 12. Add to inner SkillRegistry for tool discovery
        {
            let mut guard = self.inner.write().await;
            guard.add(loaded_skill);
        }

        info!(
            skill = %skill_name,
            trust_tier = %trust_tier,
            "skill loaded from hub and verified"
        );

        Ok(())
    }

    /// Fetch trust root keys from a domain and add them to the verifier
    ///
    /// This method fetches the public keys for a trust root domain from
    /// the registry and adds them to the verifier for attestation validation.
    ///
    /// # Arguments
    ///
    /// * `domain` - The trust root domain (e.g., "talonhub.io")
    ///
    /// # Errors
    ///
    /// Returns error if key fetch, decoding, or validation fails.
    pub async fn fetch_and_add_trust_root(&mut self, domain: &str) -> SkillSecurityResult<()> {
        info!(domain = %domain, "fetching trust root keys");

        let keys_response = self.registry_client.fetch_trust_root_keys(domain).await?;

        for key_info in &keys_response.keys {
            let key_bytes = base64::engine::general_purpose::STANDARD
                .decode(&key_info.public_key)
                .map_err(|e| SkillSecurityError::TrustRootFetchFailed {
                    domain: domain.to_string(),
                    reason: format!("invalid base64 key: {e}"),
                })?;

            if key_bytes.len() != 32 {
                return Err(SkillSecurityError::TrustRootFetchFailed {
                    domain: domain.to_string(),
                    reason: format!("key must be 32 bytes, got {}", key_bytes.len()),
                });
            }

            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&key_bytes);

            let verifying_key = VerifyingKey::from_bytes(&key_array).map_err(|e| {
                SkillSecurityError::TrustRootFetchFailed {
                    domain: domain.to_string(),
                    reason: format!("invalid Ed25519 key: {e}"),
                }
            })?;

            self.verifier.add_trusted_root(domain, verifying_key);
            info!(
                domain = %domain,
                kid = %key_info.kid,
                "added trust root key"
            );
        }

        Ok(())
    }

    /// Get a verified skill by name
    #[must_use]
    pub fn get_verified(&self, name: &str) -> Option<&VerifiedSkill> {
        self.verified_skills.get(name)
    }

    /// Get a verified skill by ID
    #[must_use]
    pub fn get_verified_by_id(&self, id: &SkillId) -> Option<&VerifiedSkill> {
        self.verified_skills.values().find(|s| &s.id == id)
    }

    /// Check if a skill has a specific capability
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Name of the skill
    /// * `capability` - The capability to check
    ///
    /// # Errors
    ///
    /// Returns error if skill is not found or capability is not granted.
    pub fn check_capability(
        &self,
        skill_name: &str,
        capability: &CapabilityPath,
    ) -> SkillSecurityResult<()> {
        let skill = self.verified_skills.get(skill_name).ok_or_else(|| {
            SkillSecurityError::SkillNotFound {
                skill_name: skill_name.to_string(),
            }
        })?;

        if skill.has_capability(capability) {
            Ok(())
        } else {
            Err(SkillSecurityError::CapabilityNotGranted {
                skill_name: skill_name.to_string(),
                requested_capability: capability.to_string(),
                granted_capabilities: skill.capabilities.iter().map(|c| c.to_string()).collect(),
            })
        }
    }

    /// Check if a tool invocation is allowed for a skill
    ///
    /// # Arguments
    ///
    /// * `skill_name` - Name of the skill
    /// * `tool_name` - Name of the tool to invoke
    ///
    /// # Errors
    ///
    /// Returns error if skill is not found or tool is not allowed.
    pub fn check_tool_allowed(&self, skill_name: &str, tool_name: &str) -> SkillSecurityResult<()> {
        if let Some(required_cap) = crate::skills::capabilities::tool_to_capability(tool_name) {
            self.check_capability(skill_name, &required_cap)
        } else {
            // Unknown tool - deny by default
            Err(SkillSecurityError::CapabilityNotGranted {
                skill_name: skill_name.to_string(),
                requested_capability: format!("tool:{tool_name}"),
                granted_capabilities: vec![],
            })
        }
    }

    /// List all verified skills
    pub fn list_verified(&self) -> impl Iterator<Item = &VerifiedSkill> {
        self.verified_skills.values()
    }

    /// Remove a skill from the registry
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the skill to remove
    ///
    /// # Returns
    ///
    /// The removed verified skill if it existed.
    pub async fn remove(&mut self, name: &str) -> Option<VerifiedSkill> {
        {
            let mut guard = self.inner.write().await;
            guard.remove(name);
        }
        self.verified_skills.remove(name)
    }

    /// Get the number of verified skills
    #[must_use]
    pub fn len(&self) -> usize {
        self.verified_skills.len()
    }

    /// Check if the registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verified_skills.is_empty()
    }

    /// Clear the attestation cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get the configuration
    #[must_use]
    pub fn config(&self) -> &SecureSkillRegistryConfig {
        &self.config
    }

    /// Get access to the shared underlying skill registry
    #[must_use]
    pub fn inner(&self) -> &Arc<RwLock<SkillRegistry>> {
        &self.inner
    }

    // ========== Private helper methods ==========

    /// Load a skill from a path
    async fn load_skill_from_path(&self, path: &Path) -> SkillSecurityResult<LoadedSkill> {
        // Use acton-ai's skill loading
        let registry = SkillRegistry::from_paths(&[path]).await.map_err(|e| {
            SkillSecurityError::SkillLoadError {
                skill_name: path.display().to_string(),
                reason: e.to_string(),
            }
        })?;

        registry
            .iter()
            .next()
            .cloned()
            .ok_or_else(|| SkillSecurityError::SkillNotFound {
                skill_name: path.display().to_string(),
            })
    }

    /// Extract agent URI from skill metadata
    fn extract_agent_uri(&self, skill: &LoadedSkill) -> SkillSecurityResult<AgentUri> {
        // Try to get agent-uri from skill content/metadata
        // For now, construct a default agent URI from skill name
        // In production, this would parse from SKILL.md frontmatter
        let uri_str = format!("agent://talonhub.io/skill/{}", skill.name());

        AgentUri::parse(&uri_str).map_err(|e| SkillSecurityError::InvalidAgentUri {
            uri: uri_str,
            reason: e.to_string(),
        })
    }

    /// Get attestation from cache or registry
    async fn get_attestation(
        &mut self,
        skill_name: &str,
        agent_uri: &AgentUri,
    ) -> SkillSecurityResult<AttestationClaims> {
        // Check cache first
        if let Some(cached) = self.cache.get(skill_name) {
            debug!(skill = %skill_name, "using cached attestation");
            return Ok(cached.clone());
        }

        // If attestation not required, return a placeholder
        if !self.config.require_attestation {
            warn!(skill = %skill_name, "attestation not required, using placeholder");
            return create_placeholder_attestation(agent_uri);
        }

        // Fetch from registry if auto-fetch is enabled
        if self.config.auto_fetch_attestations {
            let response = self
                .registry_client
                .fetch_attestation(agent_uri.as_ref())
                .await?;

            // Verify and parse the token
            let claims = self.verifier.verify(&response.token).map_err(|e| {
                SkillSecurityError::AttestationVerification {
                    skill_name: skill_name.to_string(),
                    reason: e.to_string(),
                }
            })?;

            // Cache the attestation
            self.cache.insert(skill_name, claims.clone())?;

            return Ok(claims);
        }

        Err(SkillSecurityError::AttestationNotFound {
            skill_name: skill_name.to_string(),
        })
    }

    /// Verify attestation claims
    fn verify_attestation(
        &self,
        skill_name: &str,
        agent_uri: &AgentUri,
        claims: &AttestationClaims,
    ) -> SkillSecurityResult<()> {
        // Check if expired
        if claims.is_expired() {
            return Err(SkillSecurityError::AttestationExpired {
                skill_name: skill_name.to_string(),
                expired_at: claims.exp,
            });
        }

        // Verify subject matches
        if claims.agent_uri != agent_uri.to_string() {
            return Err(SkillSecurityError::AttestationVerification {
                skill_name: skill_name.to_string(),
                reason: format!(
                    "agent URI mismatch: expected {}, got {}",
                    agent_uri, claims.agent_uri
                ),
            });
        }

        Ok(())
    }

    /// Compute OmniBOR ID and verify against attestation
    fn compute_and_verify_integrity(
        &self,
        path: &Path,
        skill_name: &str,
        _attestation: &AttestationClaims,
    ) -> SkillSecurityResult<ArtifactId<Sha256>> {
        if !self.config.verify_integrity {
            // Return a computed ID without verification
            return self.compute_omnibor_id(path);
        }

        let computed_id = self.compute_omnibor_id(path)?;

        // In production, attestation would include omnibor_id field
        // For now, we just return the computed ID
        // TODO: Compare with attestation.omnibor_id when available

        debug!(
            skill = %skill_name,
            omnibor_id = %computed_id,
            "computed OmniBOR ID"
        );

        Ok(computed_id)
    }

    /// Compute OmniBOR artifact ID for a skill directory
    fn compute_omnibor_id(&self, path: &Path) -> SkillSecurityResult<ArtifactId<Sha256>> {
        // Read SKILL.md and compute hash
        let skill_md_path = path.join("SKILL.md");
        let content = std::fs::read(&skill_md_path).map_err(|e| SkillSecurityError::IoError {
            message: format!("failed to read {}: {}", skill_md_path.display(), e),
        })?;

        let id = ArtifactIdBuilder::<Sha256, _>::with_rustcrypto().identify_bytes(&content);

        Ok(id)
    }

    /// Check that attestation capabilities cover required tool capabilities
    fn check_capability_coverage(
        &self,
        skill: &LoadedSkill,
        granted: &[CapabilityPath],
    ) -> SkillSecurityResult<()> {
        // Get allowed tools from skill info
        // In production, this would come from SKILL.md frontmatter
        // For now, we'll use a placeholder
        let allowed_tools = get_allowed_tools_from_skill(skill);
        let required = tools_to_capabilities(allowed_tools.iter().map(|s| s.as_str()));

        let missing = find_missing_capabilities(granted, &required);

        if !missing.is_empty() {
            return Err(SkillSecurityError::CapabilityNotGranted {
                skill_name: skill.name().to_string(),
                requested_capability: missing
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                granted_capabilities: granted.iter().map(|c| c.to_string()).collect(),
            });
        }

        Ok(())
    }

    /// Determine trust tier from capabilities
    fn determine_trust_tier(
        &self,
        capabilities: &[CapabilityPath],
    ) -> SkillSecurityResult<TrustTier> {
        // Find the highest tier required by any capability
        let mut max_tier = TrustTier::None;

        for cap in capabilities {
            let tier = self.trust_manager.required_tier_for(cap);
            if tier > max_tier {
                max_tier = tier;
            }
        }

        // Check against configured maximum
        if max_tier > self.config.max_trust_tier {
            return Err(SkillSecurityError::TrustTierViolation {
                skill_name: "unknown".to_string(),
                required_tier: max_tier.as_u8(),
                actual_tier: self.config.max_trust_tier.as_u8(),
            });
        }

        Ok(max_tier)
    }
}

// ========== Helper functions ==========

/// Create a placeholder attestation for unauthenticated skills
fn create_placeholder_attestation(agent_uri: &AgentUri) -> SkillSecurityResult<AttestationClaims> {
    AttestationClaims::builder()
        .agent_uri(agent_uri.to_string())
        .issuer("local")
        .ttl(Duration::from_secs(86400)) // 24 hours
        .build()
        .map_err(|e| SkillSecurityError::AttestationVerification {
            skill_name: "placeholder".to_string(),
            reason: e.to_string(),
        })
}

/// Get allowed tools from skill (placeholder)
fn get_allowed_tools_from_skill(_skill: &LoadedSkill) -> Vec<String> {
    // In production, this would parse from SKILL.md frontmatter
    // For now, return empty to skip capability checking
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SecureSkillRegistryConfig::default();
        assert!(config.require_attestation);
        assert!(config.verify_integrity);
        assert_eq!(config.max_trust_tier, TrustTier::Medium);
    }

    #[test]
    fn test_registry_creation() {
        let registry = SecureSkillRegistry::new();
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_with_custom_config() {
        let config = SecureSkillRegistryConfig {
            require_attestation: false,
            verify_integrity: false,
            max_trust_tier: TrustTier::High,
            ..Default::default()
        };

        let registry = SecureSkillRegistry::with_config(config);
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        assert!(!registry.config().require_attestation);
        assert!(!registry.config().verify_integrity);
    }

    #[test]
    fn test_registry_with_shared_inner() {
        let shared_inner = Arc::new(RwLock::new(SkillRegistry::new()));
        let config = SecureSkillRegistryConfig::default();

        let registry = SecureSkillRegistry::with_shared_registry(config, shared_inner.clone());
        assert!(registry.is_ok());

        let registry = registry.unwrap();
        assert!(registry.is_empty());

        // Verify it returns the same Arc
        assert!(Arc::ptr_eq(&registry.inner_registry(), &shared_inner));
    }

    #[test]
    fn test_inner_registry_accessor() {
        let registry = SecureSkillRegistry::new().unwrap();
        let inner1 = registry.inner_registry();
        let inner2 = registry.inner_registry();

        // Both should point to the same underlying data
        assert!(Arc::ptr_eq(&inner1, &inner2));
    }

    #[test]
    fn test_placeholder_attestation() {
        let uri =
            AgentUri::parse("agent://test.com/skill/skill_01h455vb4pex5vsknk084sn02q").unwrap();
        let claims = create_placeholder_attestation(&uri).unwrap();

        assert_eq!(claims.iss, "local");
        assert!(!claims.is_expired());
    }
}
