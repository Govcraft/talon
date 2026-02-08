//! Trust tier types for agent capability access control.
//!
//! Each agent is assigned a trust tier that determines which tools and
//! capabilities it may invoke. Tiers form a strict linear ordering so
//! that higher tiers implicitly include the permissions of lower ones.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Trust tier governing an agent's access to tools and capabilities.
///
/// Tiers are ordered from least to most privileged. A higher tier
/// implicitly includes all permissions granted to lower tiers.
///
/// # Ordering
///
/// `Untrusted < Basic < Standard < Elevated < Full`
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum TrustTier {
    /// No tool access permitted.
    #[default]
    Untrusted = 0,
    /// Read-only tools (e.g. file reading, web fetch).
    Basic = 1,
    /// Read and write tools (e.g. file writing, editing).
    Standard = 2,
    /// System-level tools (e.g. bash execution).
    Elevated = 3,
    /// All tools with no restrictions.
    Full = 4,
}

impl fmt::Display for TrustTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Untrusted => "untrusted",
            Self::Basic => "basic",
            Self::Standard => "standard",
            Self::Elevated => "elevated",
            Self::Full => "full",
        };
        write!(f, "{label}")
    }
}

/// Error returned when parsing an invalid trust tier string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTrustTierError {
    invalid: String,
}

impl fmt::Display for ParseTrustTierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid trust tier '{}': expected one of untrusted, basic, standard, elevated, full",
            self.invalid
        )
    }
}

impl std::error::Error for ParseTrustTierError {}

impl FromStr for TrustTier {
    type Err = ParseTrustTierError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "untrusted" | "0" => Ok(Self::Untrusted),
            "basic" | "1" => Ok(Self::Basic),
            "standard" | "2" => Ok(Self::Standard),
            "elevated" | "3" => Ok(Self::Elevated),
            "full" | "4" => Ok(Self::Full),
            _ => Err(ParseTrustTierError {
                invalid: s.to_string(),
            }),
        }
    }
}

impl From<u8> for TrustTier {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Untrusted,
            1 => Self::Basic,
            2 => Self::Standard,
            3 => Self::Elevated,
            4 => Self::Full,
            // Saturate at Full for any value above 4.
            _ => Self::Full,
        }
    }
}

impl From<TrustTier> for u8 {
    fn from(tier: TrustTier) -> Self {
        tier as u8
    }
}

impl TrustTier {
    /// Returns `true` if this tier meets or exceeds the required tier.
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_untrusted() {
        assert_eq!(TrustTier::default(), TrustTier::Untrusted);
    }

    #[test]
    fn test_ordering() {
        assert!(TrustTier::Untrusted < TrustTier::Basic);
        assert!(TrustTier::Basic < TrustTier::Standard);
        assert!(TrustTier::Standard < TrustTier::Elevated);
        assert!(TrustTier::Elevated < TrustTier::Full);
    }

    #[test]
    fn test_display_roundtrip() {
        for tier in [
            TrustTier::Untrusted,
            TrustTier::Basic,
            TrustTier::Standard,
            TrustTier::Elevated,
            TrustTier::Full,
        ] {
            let s = tier.to_string();
            let parsed: TrustTier = s.parse().expect("should parse display output");
            assert_eq!(parsed, tier);
        }
    }

    #[test]
    fn test_from_str_numeric() {
        assert_eq!("0".parse::<TrustTier>().unwrap(), TrustTier::Untrusted);
        assert_eq!("1".parse::<TrustTier>().unwrap(), TrustTier::Basic);
        assert_eq!("2".parse::<TrustTier>().unwrap(), TrustTier::Standard);
        assert_eq!("3".parse::<TrustTier>().unwrap(), TrustTier::Elevated);
        assert_eq!("4".parse::<TrustTier>().unwrap(), TrustTier::Full);
    }

    #[test]
    fn test_from_str_invalid() {
        assert!("invalid".parse::<TrustTier>().is_err());
        assert!("5".parse::<TrustTier>().is_err());
    }

    #[test]
    fn test_from_u8() {
        assert_eq!(TrustTier::from(0u8), TrustTier::Untrusted);
        assert_eq!(TrustTier::from(1u8), TrustTier::Basic);
        assert_eq!(TrustTier::from(4u8), TrustTier::Full);
        // Values above 4 saturate at Full.
        assert_eq!(TrustTier::from(255u8), TrustTier::Full);
    }

    #[test]
    fn test_into_u8() {
        assert_eq!(u8::from(TrustTier::Untrusted), 0);
        assert_eq!(u8::from(TrustTier::Full), 4);
    }

    #[test]
    fn test_satisfies() {
        assert!(TrustTier::Full.satisfies(TrustTier::Elevated));
        assert!(TrustTier::Standard.satisfies(TrustTier::Standard));
        assert!(!TrustTier::Basic.satisfies(TrustTier::Standard));
        assert!(!TrustTier::Untrusted.satisfies(TrustTier::Basic));
    }

    #[test]
    fn test_serde_roundtrip() {
        let tier = TrustTier::Elevated;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"elevated\"");
        let parsed: TrustTier = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tier);
    }
}
