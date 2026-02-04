//! SecureSkillRegistry with attestation verification
//!
//! Wraps acton-ai's SkillRegistry with additional security:
//! - Agent-URI verification
//! - PASETO attestation validation
//! - OmniBOR integrity checks
//! - Trust tier enforcement

mod cache;
mod capabilities;
mod error;
mod registry;
mod registry_client;
mod verified;
mod verification;

pub use cache::{AttestationCache, AttestationCacheConfig, CacheEntry};
pub use capabilities::{
    capabilities_cover, find_missing_capabilities, parse_capabilities, tool_to_capability,
    tools_to_capabilities, CapabilityPath,
};
pub use error::{SkillSecurityError, SkillSecurityResult};
pub use registry::{SecureSkillRegistry, SecureSkillRegistryConfig};
pub use registry_client::{
    AttestationResponse, PublicKeyInfo, RegistryClient, RegistryClientConfig, SkillMetadata,
    TrustRootKeys,
};
pub use verified::{InvalidSkillId, SkillId, VerifiedSkill, VerifiedSkillBuilder};
pub use verification::SkillVerifier;
