//! Trust tier definitions and management

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::{TalonError, TalonResult};
use crate::skills::CapabilityPath;

/// Trust tier levels
///
/// | Tier | Risk Level | Capabilities | Verification |
/// |------|------------|--------------|--------------|
/// | 0 | None | Read-only, no network | Signed manifest |
/// | 1 | Low | Local file read, limited network | + Publisher attestation |
/// | 2 | Medium | File write, full network | + Code review attestation |
/// | 3 | High | Shell execution (sandboxed) | + Security audit attestation |
/// | 4 | Critical | System modification | + User explicit approval per-use |
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TrustTier {
    /// No risk - read-only, no network
    None = 0,
    /// Low risk - local file read, limited network
    Low = 1,
    /// Medium risk - file write, full network
    Medium = 2,
    /// High risk - shell execution (sandboxed)
    High = 3,
    /// Critical risk - system modification
    Critical = 4,
}

impl TrustTier {
    /// Create from u8 value
    ///
    /// # Errors
    ///
    /// Returns error if value is out of range
    pub fn from_u8(value: u8) -> TalonResult<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Low),
            2 => Ok(Self::Medium),
            3 => Ok(Self::High),
            4 => Ok(Self::Critical),
            _ => Err(TalonError::TrustTierViolation {
                required: 4,
                actual: value,
            }),
        }
    }

    /// Get the numeric value
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for TrustTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::None => "None (Tier 0)",
            Self::Low => "Low (Tier 1)",
            Self::Medium => "Medium (Tier 2)",
            Self::High => "High (Tier 3)",
            Self::Critical => "Critical (Tier 4)",
        };
        write!(f, "{name}")
    }
}

/// Trust tier manager
///
/// Validates capabilities against trust tiers.
pub struct TrustTierManager {
    /// Maximum allowed tier
    max_tier: TrustTier,
}

impl TrustTierManager {
    /// Create a new trust tier manager
    #[must_use]
    pub fn new(max_tier: TrustTier) -> Self {
        Self { max_tier }
    }

    /// Get the maximum allowed tier
    #[must_use]
    pub fn max_tier(&self) -> TrustTier {
        self.max_tier
    }

    /// Determine required tier for a capability
    #[must_use]
    pub fn required_tier_for(&self, capability: &CapabilityPath) -> TrustTier {
        let path = capability.as_str();

        if path.starts_with("shell") {
            TrustTier::High
        } else if path.starts_with("file/write") || path.starts_with("network") {
            TrustTier::Medium
        } else if path.starts_with("file/read") {
            TrustTier::Low
        } else {
            TrustTier::None
        }
    }

    /// Check if a capability is allowed
    ///
    /// # Errors
    ///
    /// Returns error if capability requires higher tier than allowed
    pub fn check_capability(&self, capability: &CapabilityPath) -> TalonResult<()> {
        let required = self.required_tier_for(capability);
        if required > self.max_tier {
            return Err(TalonError::TrustTierViolation {
                required: required.as_u8(),
                actual: self.max_tier.as_u8(),
            });
        }
        Ok(())
    }
}

impl Default for TrustTierManager {
    fn default() -> Self {
        Self::new(TrustTier::Medium)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_tier_ordering() {
        assert!(TrustTier::None < TrustTier::Low);
        assert!(TrustTier::Low < TrustTier::Medium);
        assert!(TrustTier::Medium < TrustTier::High);
        assert!(TrustTier::High < TrustTier::Critical);
    }

    #[test]
    fn test_required_tier_for_read() {
        let manager = TrustTierManager::default();
        let cap = CapabilityPath::new("file/read");
        assert_eq!(manager.required_tier_for(&cap), TrustTier::Low);
    }

    #[test]
    fn test_required_tier_for_shell() {
        let manager = TrustTierManager::default();
        let cap = CapabilityPath::new("shell/execute");
        assert_eq!(manager.required_tier_for(&cap), TrustTier::High);
    }

    #[test]
    fn test_check_capability_allowed() {
        let manager = TrustTierManager::new(TrustTier::Medium);
        let cap = CapabilityPath::new("file/read");
        assert!(manager.check_capability(&cap).is_ok());
    }

    #[test]
    fn test_check_capability_denied() {
        let manager = TrustTierManager::new(TrustTier::Low);
        let cap = CapabilityPath::new("shell/execute");
        assert!(manager.check_capability(&cap).is_err());
    }
}
