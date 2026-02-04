//! SecureSkillRegistry with attestation verification
//!
//! Wraps acton-ai's SkillRegistry with additional security:
//! - Agent-URI verification
//! - PASETO attestation validation
//! - OmniBOR integrity checks
//! - Trust tier enforcement

mod capabilities;
mod registry;
mod verification;

pub use capabilities::CapabilityPath;
pub use registry::SecureSkillRegistry;
pub use verification::SkillVerifier;
