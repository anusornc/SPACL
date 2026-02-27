//! Profile Validation Cache Module
//!
//! This module provides caching mechanisms for profile validation results
//! to improve performance when validating the same or similar ontologies.

use super::common::{Owl2Profile, ProfileValidationResult};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Priority level for cache entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CachePriority {
    /// Low priority - may be evicted quickly
    Low = 0,
    /// Normal priority
    Normal = 1,
    /// High priority - prefer keeping
    High = 2,
    /// Critical priority - never evict
    Critical = 3,
}

impl Default for CachePriority {
    fn default() -> Self {
        CachePriority::Normal
    }
}

/// Configuration for profile cache
#[derive(Debug, Clone)]
pub struct ProfileCacheConfig {
    /// Maximum number of entries in the cache
    pub max_entries: usize,
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// Enable priority-based eviction
    pub priority_eviction: bool,
    /// Enable statistics tracking
    pub track_statistics: bool,
}

impl Default for ProfileCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            default_ttl: Duration::from_secs(3600), // 1 hour
            priority_eviction: true,
            track_statistics: true,
        }
    }
}

impl ProfileCacheConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = max_entries;
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }
}

/// A cached validation result entry
#[derive(Debug, Clone)]
struct CacheEntry {
    result: ProfileValidationResult,
    timestamp: Instant,
    ttl: Duration,
    priority: CachePriority,
    access_count: usize,
}

impl CacheEntry {
    fn new(result: ProfileValidationResult, ttl: Duration, priority: CachePriority) -> Self {
        Self {
            result,
            timestamp: Instant::now(),
            ttl,
            priority,
            access_count: 0,
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }

    fn record_access(&mut self) {
        self.access_count += 1;
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStatistics {
    /// Total number of cache hits
    pub hits: usize,
    /// Total number of cache misses
    pub misses: usize,
    /// Number of entries currently in cache
    pub current_entries: usize,
    /// Total number of evictions
    pub total_evictions: usize,
    /// Average time to retrieve from cache (microseconds)
    pub avg_retrieval_time_us: f64,
}

impl CacheStatistics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    pub fn record_eviction(&mut self) {
        self.total_evictions += 1;
    }
}

/// Thread-safe profile validation cache
pub struct ProfileValidationCache {
    config: ProfileCacheConfig,
    cache: RwLock<HashMap<String, CacheEntry>>,
    stats: RwLock<CacheStatistics>,
}

impl ProfileValidationCache {
    /// Create a new cache with default configuration
    pub fn new() -> Self {
        Self::with_config(ProfileCacheConfig::default())
    }

    /// Create a new cache with custom configuration
    pub fn with_config(config: ProfileCacheConfig) -> Self {
        Self {
            config,
            cache: RwLock::new(HashMap::new()),
            stats: RwLock::new(CacheStatistics::default()),
        }
    }

    /// Get a cached validation result
    pub fn get(&self, key: &str) -> Option<ProfileValidationResult> {
        let start = Instant::now();

        let result = {
            let mut cache = match self.cache.write() {
                Ok(c) => c,
                Err(_) => {
                    if let Ok(mut stats) = self.stats.write() {
                        stats.record_miss();
                    }
                    return None;
                }
            };

            let entry = match cache.get_mut(key) {
                Some(e) => e,
                None => {
                    drop(cache);
                    if let Ok(mut stats) = self.stats.write() {
                        stats.record_miss();
                    }
                    return None;
                }
            };

            if entry.is_expired() {
                cache.remove(key);
                drop(cache);
                if let Ok(mut stats) = self.stats.write() {
                    stats.record_miss();
                }
                return None;
            }

            entry.record_access();
            Some(entry.result.clone())
        };

        let elapsed = start.elapsed().as_micros() as f64;

        if let Ok(mut stats) = self.stats.write() {
            if result.is_some() {
                stats.record_hit();
            } else {
                stats.record_miss();
            }
            // Update average retrieval time
            let total = stats.hits + stats.misses;
            stats.avg_retrieval_time_us =
                (stats.avg_retrieval_time_us * (total - 1) as f64 + elapsed) / total as f64;
        }

        result
    }

    /// Insert a validation result into the cache
    pub fn insert(&self, key: String, result: ProfileValidationResult, priority: CachePriority) {
        // Check if we need to evict entries
        self.maybe_evict();

        let ttl = self.config.default_ttl;
        let entry = CacheEntry::new(result, ttl, priority);

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, entry);
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.current_entries = self.cache.read().map(|c| c.len()).unwrap_or(0);
        }
    }

    /// Insert with default priority
    pub fn insert_default(&self, key: String, result: ProfileValidationResult) {
        self.insert(key, result, CachePriority::Normal);
    }

    /// Check if cache contains a valid entry
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Remove an entry from the cache
    pub fn remove(&self, key: &str) -> Option<ProfileValidationResult> {
        let mut cache = self.cache.write().ok()?;
        cache.remove(key).map(|e| e.result)
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
        if let Ok(mut stats) = self.stats.write() {
            stats.current_entries = 0;
        }
    }

    /// Get cache statistics
    pub fn get_statistics(&self) -> CacheStatistics {
        self.stats.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Reset statistics
    pub fn reset_statistics(&self) {
        if let Ok(mut stats) = self.stats.write() {
            *stats = CacheStatistics::default();
        }
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Maybe evict entries if cache is over capacity
    fn maybe_evict(&self) {
        let should_evict = {
            let cache = match self.cache.read() {
                Ok(c) => c,
                Err(_) => return,
            };
            cache.len() >= self.config.max_entries
        };

        if should_evict {
            self.evict_entries();
        }
    }

    /// Evict entries from the cache
    fn evict_entries(&self) {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        // First remove expired entries
        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            cache.remove(&key);
        }

        // If still over capacity, evict by priority and access count
        if cache.len() >= self.config.max_entries && self.config.priority_eviction {
            let mut entries: Vec<(String, CachePriority, usize)> = cache
                .iter()
                .map(|(k, e)| (k.clone(), e.priority, e.access_count))
                .collect();

            // Sort by priority (ascending) then by access count (ascending)
            entries.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

            // Remove lowest priority, least accessed entries
            let to_remove = cache.len() - self.config.max_entries / 2;
            for (key, _, _) in entries.into_iter().take(to_remove) {
                cache.remove(&key);
            }
        }

        if let Ok(mut stats) = self.stats.write() {
            stats.current_entries = cache.len();
        }
    }

    /// Generate a cache key for an ontology and profile combination
    pub fn generate_key(&self, ontology_hash: &str, profile: Owl2Profile) -> String {
        format!("{}:{:?}", ontology_hash, profile)
    }
}

impl Default for ProfileValidationCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let cache = ProfileValidationCache::new();

        assert!(cache.is_empty());

        let result = ProfileValidationResult::valid(Owl2Profile::EL);
        cache.insert_default("test_key".to_string(), result.clone());

        assert!(!cache.is_empty());
        assert!(cache.contains("test_key"));

        let retrieved = cache.get("test_key");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().profile, Owl2Profile::EL);

        cache.remove("test_key");
        assert!(!cache.contains("test_key"));
    }

    #[test]
    fn test_cache_statistics() {
        let cache = ProfileValidationCache::new();

        let stats = cache.get_statistics();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);

        // Miss
        let _ = cache.get("nonexistent");
        let stats = cache.get_statistics();
        assert_eq!(stats.misses, 1);

        // Hit
        let result = ProfileValidationResult::valid(Owl2Profile::EL);
        cache.insert_default("test".to_string(), result);
        let _ = cache.get("test");
        let stats = cache.get_statistics();
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_cache_key_generation() {
        let cache = ProfileValidationCache::new();
        let key = cache.generate_key("hash123", Owl2Profile::EL);
        assert!(key.contains("hash123"));
        assert!(key.contains("EL"));
    }
}
