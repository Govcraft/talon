//! SecureSkillRegistry with attestation verification
//!
//! Wraps acton-ai's SkillRegistry with additional security:
//! - Agent-URI verification
//! - PASETO attestation validation
//! - OmniBOR integrity checks
//! - Trust tier enforcement
//! - Capability-verified tool execution

mod cache;
mod capabilities;
mod error;
mod extended_claims;
mod integrity;
mod omnibor_id;
mod registry;
mod registry_client;
mod secure_executor;
mod tool_bridge;
mod verification;
mod verified;

pub use cache::{AttestationCache, AttestationCacheConfig, CacheEntry};
pub use capabilities::{
    CapabilityPath, capabilities_cover, find_missing_capabilities, parse_capabilities,
    tool_to_capability, tools_to_capabilities,
};
pub use error::{SkillSecurityError, SkillSecurityResult};
pub use extended_claims::{ExtendedClaims, ExtendedClaimsBuilder};
pub use integrity::{
    IntegrityResult, compute_artifact_id, compute_skill_omnibor_id, verify_integrity,
};
pub use omnibor_id::{InvalidOmniborId, InvalidOmniborIdReason, OmniborId};
pub use registry::{SecureSkillRegistry, SecureSkillRegistryConfig};
pub use registry_client::{
    AttestationResponse, PublicKeyInfo, RegistryClient, RegistryClientConfig, SkillMetadata,
    TrustRootKeys,
};
pub use secure_executor::{ExecutionContext, ExecutionResult, SecureToolExecutor};
pub use tool_bridge::{BridgedTool, ToolBridge};
pub use verification::SkillVerifier;
pub use verified::{InvalidSkillId, SkillId, VerifiedSkill, VerifiedSkillBuilder};
