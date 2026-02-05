//! Extended attestation claims with OmniBOR support
//!
//! Wraps `AttestationClaims` from `agent-uri-attestation` with additional
//! fields for OmniBOR integrity verification while maintaining transparent
//! access to base claims through `Deref`.

use agent_uri_attestation::AttestationClaims;
use std::ops::Deref;

use crate::skills::omnibor_id::OmniborId;

/// Extended attestation claims with OmniBOR support
///
/// Composes `AttestationClaims` with additional metadata for integrity
/// verification. Implements `Deref` to `AttestationClaims` for transparent
/// access to base claim fields.
#[derive(Clone, Debug)]
pub struct ExtendedClaims {
    /// The base attestation claims
    inner: AttestationClaims,

    /// OmniBOR ID for content integrity verification (optional for legacy attestations)
    omnibor_id: Option<OmniborId>,

    /// Skill version from registry (optional)
    version: Option<String>,
}

impl ExtendedClaims {
    /// Create extended claims from base claims without OmniBOR ID
    ///
    /// Use this for legacy attestations that don't include integrity verification.
    #[must_use]
    pub fn from_base(claims: AttestationClaims) -> Self {
        Self {
            inner: claims,
            omnibor_id: None,
            version: None,
        }
    }

    /// Create extended claims with OmniBOR ID
    #[must_use]
    pub fn with_omnibor(claims: AttestationClaims, omnibor_id: OmniborId) -> Self {
        Self {
            inner: claims,
            omnibor_id: Some(omnibor_id),
            version: None,
        }
    }

    /// Create a builder for extended claims
    #[must_use]
    pub fn builder(claims: AttestationClaims) -> ExtendedClaimsBuilder {
        ExtendedClaimsBuilder::new(claims)
    }

    /// Get the OmniBOR ID if present
    #[must_use]
    pub fn omnibor_id(&self) -> Option<&OmniborId> {
        self.omnibor_id.as_ref()
    }

    /// Get the version if present
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Check if this attestation includes OmniBOR integrity verification
    #[must_use]
    pub fn has_omnibor(&self) -> bool {
        self.omnibor_id.is_some()
    }

    /// Get the inner AttestationClaims
    #[must_use]
    pub fn inner(&self) -> &AttestationClaims {
        &self.inner
    }

    /// Consume self and return the inner AttestationClaims
    #[must_use]
    pub fn into_inner(self) -> AttestationClaims {
        self.inner
    }
}

impl Deref for ExtendedClaims {
    type Target = AttestationClaims;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<AttestationClaims> for ExtendedClaims {
    fn from(claims: AttestationClaims) -> Self {
        Self::from_base(claims)
    }
}

/// Builder for constructing ExtendedClaims
pub struct ExtendedClaimsBuilder {
    inner: AttestationClaims,
    omnibor_id: Option<OmniborId>,
    version: Option<String>,
}

impl ExtendedClaimsBuilder {
    /// Create a new builder with base claims
    #[must_use]
    pub fn new(claims: AttestationClaims) -> Self {
        Self {
            inner: claims,
            omnibor_id: None,
            version: None,
        }
    }

    /// Set the OmniBOR ID
    #[must_use]
    pub fn omnibor_id(mut self, id: OmniborId) -> Self {
        self.omnibor_id = Some(id);
        self
    }

    /// Set the OmniBOR ID from an optional value
    #[must_use]
    pub fn maybe_omnibor_id(mut self, id: Option<OmniborId>) -> Self {
        self.omnibor_id = id;
        self
    }

    /// Set the version
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the version from an optional value
    #[must_use]
    pub fn maybe_version(mut self, version: Option<String>) -> Self {
        self.version = version;
        self
    }

    /// Build the extended claims
    #[must_use]
    pub fn build(self) -> ExtendedClaims {
        ExtendedClaims {
            inner: self.inner,
            omnibor_id: self.omnibor_id,
            version: self.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample_claims() -> AttestationClaims {
        AttestationClaims::builder()
            .agent_uri("agent://test.com/skill/test")
            .issuer("test-issuer")
            .ttl(Duration::from_secs(3600))
            .build()
            .unwrap()
    }

    fn sample_omnibor_id() -> OmniborId {
        OmniborId::parse(
            "gitoid:blob:sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        )
        .unwrap()
    }

    #[test]
    fn test_from_base() {
        let claims = sample_claims();
        let extended = ExtendedClaims::from_base(claims.clone());

        assert!(!extended.has_omnibor());
        assert!(extended.omnibor_id().is_none());
        assert!(extended.version().is_none());
        assert_eq!(extended.iss, claims.iss);
    }

    #[test]
    fn test_with_omnibor() {
        let claims = sample_claims();
        let omnibor_id = sample_omnibor_id();
        let extended = ExtendedClaims::with_omnibor(claims.clone(), omnibor_id.clone());

        assert!(extended.has_omnibor());
        assert_eq!(extended.omnibor_id(), Some(&omnibor_id));
        assert_eq!(extended.iss, claims.iss);
    }

    #[test]
    fn test_builder() {
        let claims = sample_claims();
        let omnibor_id = sample_omnibor_id();

        let extended = ExtendedClaims::builder(claims.clone())
            .omnibor_id(omnibor_id.clone())
            .version("1.2.3")
            .build();

        assert!(extended.has_omnibor());
        assert_eq!(extended.omnibor_id(), Some(&omnibor_id));
        assert_eq!(extended.version(), Some("1.2.3"));
        assert_eq!(extended.iss, claims.iss);
    }

    #[test]
    fn test_builder_maybe_methods() {
        let claims = sample_claims();

        let extended = ExtendedClaims::builder(claims)
            .maybe_omnibor_id(None)
            .maybe_version(Some("2.0.0".to_string()))
            .build();

        assert!(!extended.has_omnibor());
        assert_eq!(extended.version(), Some("2.0.0"));
    }

    #[test]
    fn test_deref_access() {
        let claims = sample_claims();
        let extended = ExtendedClaims::from_base(claims.clone());

        // Access through Deref
        assert_eq!(extended.agent_uri, claims.agent_uri);
        assert_eq!(extended.iss, claims.iss);
        assert!(!extended.is_expired());
    }

    #[test]
    fn test_into_inner() {
        let claims = sample_claims();
        let extended = ExtendedClaims::from_base(claims.clone());
        let inner = extended.into_inner();

        assert_eq!(inner.agent_uri, claims.agent_uri);
    }

    #[test]
    fn test_from_attestation_claims() {
        let claims = sample_claims();
        let extended: ExtendedClaims = claims.clone().into();

        assert!(!extended.has_omnibor());
        assert_eq!(extended.iss, claims.iss);
    }
}
