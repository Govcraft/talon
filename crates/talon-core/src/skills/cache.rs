//! Attestation cache with TTL-based expiration
//!
//! Provides in-memory caching of verified attestations to avoid
//! repeated verification and registry lookups.

use std::collections::HashMap;
use std::time::Duration;

use agent_uri_attestation::AttestationClaims;
use chrono::{DateTime, Utc};

use crate::skills::error::{SkillSecurityError, SkillSecurityResult};

/// Entry in the attestation cache
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// The cached attestation claims
    pub claims: AttestationClaims,

    /// When this entry was cached
    pub cached_at: DateTime<Utc>,

    /// Time-to-live for this cache entry
    pub ttl: Duration,
}

impl CacheEntry {
    /// Create a new cache entry
    #[must_use]
    pub fn new(claims: AttestationClaims, ttl: Duration) -> Self {
        Self {
            claims,
            cached_at: Utc::now(),
            ttl,
        }
    }

    /// Check if this cache entry has expired
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }

    /// Check if this cache entry is expired at a specific time
    #[must_use]
    pub fn is_expired_at(&self, at: DateTime<Utc>) -> bool {
        let ttl_chrono = chrono::Duration::from_std(self.ttl).unwrap_or(chrono::Duration::zero());
        let expires_at = self.cached_at + ttl_chrono;
        at >= expires_at
    }

    /// Check if the underlying attestation has expired
    #[must_use]
    pub fn is_attestation_expired(&self) -> bool {
        self.claims.is_expired()
    }
}

/// Configuration for the attestation cache
#[derive(Clone, Debug)]
pub struct AttestationCacheConfig {
    /// Default TTL for cache entries
    pub default_ttl: Duration,

    /// Maximum number of entries in the cache
    pub max_entries: usize,

    /// Whether to automatically evict expired entries
    pub auto_evict: bool,
}

impl Default for AttestationCacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(300), // 5 minutes
            max_entries: 1000,
            auto_evict: true,
        }
    }
}

/// In-memory cache for verified attestations
///
/// Caches attestation claims by skill name to avoid repeated
/// verification and registry lookups. Entries expire based on
/// a configurable TTL.
pub struct AttestationCache {
    /// Cached entries keyed by skill name
    entries: HashMap<String, CacheEntry>,

    /// Cache configuration
    config: AttestationCacheConfig,
}

impl AttestationCache {
    /// Create a new attestation cache with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(AttestationCacheConfig::default())
    }

    /// Create a new attestation cache with custom configuration
    #[must_use]
    pub fn with_config(config: AttestationCacheConfig) -> Self {
        Self {
            entries: HashMap::new(),
            config,
        }
    }

    /// Get an attestation from the cache
    ///
    /// Returns `None` if not found or if the entry has expired.
    #[must_use]
    pub fn get(&self, skill_name: &str) -> Option<&AttestationClaims> {
        self.entries.get(skill_name).and_then(|entry| {
            if entry.is_expired() || entry.is_attestation_expired() {
                None
            } else {
                Some(&entry.claims)
            }
        })
    }

    /// Insert an attestation into the cache
    ///
    /// Uses the default TTL from configuration.
    ///
    /// # Errors
    ///
    /// Returns error if cache is full and eviction fails.
    pub fn insert(
        &mut self,
        skill_name: impl Into<String>,
        claims: AttestationClaims,
    ) -> SkillSecurityResult<()> {
        self.insert_with_ttl(skill_name, claims, self.config.default_ttl)
    }

    /// Insert an attestation with a custom TTL
    ///
    /// # Errors
    ///
    /// Returns error if cache is full and eviction fails.
    pub fn insert_with_ttl(
        &mut self,
        skill_name: impl Into<String>,
        claims: AttestationClaims,
        ttl: Duration,
    ) -> SkillSecurityResult<()> {
        let key = skill_name.into();

        // Evict expired entries if needed
        if self.config.auto_evict && self.entries.len() >= self.config.max_entries {
            self.evict_expired();
        }

        // Check if still at capacity after eviction
        if self.entries.len() >= self.config.max_entries {
            return Err(SkillSecurityError::CacheError {
                message: format!(
                    "cache is full ({} entries) and no entries could be evicted",
                    self.config.max_entries
                ),
            });
        }

        let entry = CacheEntry::new(claims, ttl);
        self.entries.insert(key, entry);
        Ok(())
    }

    /// Remove an attestation from the cache
    ///
    /// Returns the removed entry if it existed.
    pub fn remove(&mut self, skill_name: &str) -> Option<AttestationClaims> {
        self.entries.remove(skill_name).map(|e| e.claims)
    }

    /// Check if an attestation is in the cache and not expired
    #[must_use]
    pub fn contains(&self, skill_name: &str) -> bool {
        self.get(skill_name).is_some()
    }

    /// Get the number of entries in the cache
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the cache
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Evict all expired entries from the cache
    ///
    /// Returns the number of entries evicted.
    pub fn evict_expired(&mut self) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| !entry.is_expired() && !entry.is_attestation_expired());
        before - self.entries.len()
    }

    /// Get the cache configuration
    #[must_use]
    pub fn config(&self) -> &AttestationCacheConfig {
        &self.config
    }
}

impl Default for AttestationCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_claims() -> AttestationClaims {
        AttestationClaims::builder()
            .agent_uri("agent://test.com/skill/skill_01h455vb4pex5vsknk084sn02q")
            .issuer("test.com")
            .ttl(Duration::from_secs(3600)) // 1 hour
            .build()
            .expect("valid claims")
    }

    fn make_expired_claims() -> AttestationClaims {
        // Claims that expire immediately
        AttestationClaims::builder()
            .agent_uri("agent://test.com/skill/skill_01h455vb4pex5vsknk084sn02q")
            .issuer("test.com")
            .ttl(Duration::from_secs(0))
            .build()
            .expect("valid claims")
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = AttestationCache::new();
        let claims = make_test_claims();

        cache.insert("test-skill", claims.clone()).expect("insert");
        let retrieved = cache.get("test-skill");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().agent_uri, claims.agent_uri);
    }

    #[test]
    fn test_cache_miss() {
        let cache = AttestationCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_expired_attestation() {
        let mut cache = AttestationCache::new();
        let claims = make_expired_claims();

        cache.insert("expired-skill", claims).expect("insert");

        // Wait a moment for expiration
        std::thread::sleep(Duration::from_millis(10));

        // Should return None for expired attestation
        assert!(cache.get("expired-skill").is_none());
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = AttestationCache::new();
        let claims = make_test_claims();

        cache.insert("test-skill", claims).expect("insert");
        assert!(cache.contains("test-skill"));

        let removed = cache.remove("test-skill");
        assert!(removed.is_some());
        assert!(!cache.contains("test-skill"));
    }

    #[test]
    fn test_cache_evict_expired() {
        let mut cache = AttestationCache::with_config(AttestationCacheConfig {
            default_ttl: Duration::from_secs(0), // Immediate expiration
            max_entries: 100,
            auto_evict: false,
        });

        let claims = make_test_claims();
        cache.insert("test-skill", claims).expect("insert");

        // Entry should be in cache but expired
        assert!(cache.entries.contains_key("test-skill"));

        // Wait a tiny bit to ensure expiration
        std::thread::sleep(Duration::from_millis(10));

        let evicted = cache.evict_expired();
        assert_eq!(evicted, 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_max_entries() {
        let mut cache = AttestationCache::with_config(AttestationCacheConfig {
            default_ttl: Duration::from_secs(3600),
            max_entries: 2,
            auto_evict: false,
        });

        let claims = make_test_claims();

        cache.insert("skill1", claims.clone()).expect("insert 1");
        cache.insert("skill2", claims.clone()).expect("insert 2");

        // Third insert should fail
        let result = cache.insert("skill3", claims);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_entry_ttl() {
        let entry = CacheEntry::new(make_test_claims(), Duration::from_secs(10));

        // Entry should not be expired immediately
        assert!(!entry.is_expired());

        // Check with a future time
        let future = Utc::now() + chrono::Duration::seconds(20);
        assert!(entry.is_expired_at(future));
    }
}
