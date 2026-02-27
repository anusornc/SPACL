//! Global cache management for OWL2 reasoner
//!
//! This module provides encapsulated management for global caches
//! with proper synchronization and monitoring capabilities.

use crate::util::cache::BoundedCache;
use crate::core::error::OwlError;
use crate::core::iri::IRI;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Global cache manager that encapsulates IRI caching operations
#[derive(Debug)]
pub struct GlobalCacheManager {
    /// IRI cache with bounded size and eviction policies
    iri_cache: Arc<RwLock<BoundedCache<String, IRI>>>,
    /// Cache statistics
    stats: CacheStats,
    /// Configuration settings
    config: GlobalCacheConfig,
}

/// Cache configuration parameters
#[derive(Debug, Clone)]
pub struct GlobalCacheConfig {
    /// Maximum size for IRI cache
    pub iri_cache_max_size: usize,
    /// Memory pressure threshold (0.0 to 1.0)
    pub memory_pressure_threshold: f64,
    /// Cleanup interval for background maintenance
    pub cleanup_interval: Duration,
    /// Enable statistics collection
    pub enable_stats: bool,
    /// Enable memory pressure monitoring
    pub enable_memory_pressure: bool,
}

impl Default for GlobalCacheConfig {
    fn default() -> Self {
        Self {
            iri_cache_max_size: 10_000,
            memory_pressure_threshold: 0.8,
            cleanup_interval: Duration::from_secs(60),
            enable_stats: true,
            enable_memory_pressure: true,
        }
    }
}

/// Cache statistics for monitoring and analysis
#[derive(Debug)]
pub struct CacheStats {
    /// IRI cache hits
    iri_hits: AtomicU64,
    /// IRI cache misses
    iri_misses: AtomicU64,
    /// Total cache evictions
    evictions: AtomicU64,
    /// Memory pressure events
    memory_pressure_events: AtomicU64,
}

impl CacheStats {
    fn new() -> Self {
        Self {
            iri_hits: AtomicU64::new(0),
            iri_misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            memory_pressure_events: AtomicU64::new(0),
        }
    }

    /// Get snapshot of current statistics
    pub fn snapshot(&self) -> CacheStatsSnapshot {
        CacheStatsSnapshot {
            iri_hits: self.iri_hits.load(Ordering::Relaxed),
            iri_misses: self.iri_misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            memory_pressure_events: self.memory_pressure_events.load(Ordering::Relaxed),
        }
    }

    /// Record IRI cache hit
    fn record_iri_hit(&self) {
        self.iri_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record IRI cache miss
    fn record_iri_miss(&self) {
        self.iri_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record memory pressure event
    fn record_memory_pressure(&self) {
        self.memory_pressure_events.fetch_add(1, Ordering::Relaxed);
    }
}

/// Snapshot of cache statistics for display
#[derive(Debug, Clone, Default)]
pub struct CacheStatsSnapshot {
    pub iri_hits: u64,
    pub iri_misses: u64,
    pub evictions: u64,
    pub memory_pressure_events: u64,
}

impl CacheStatsSnapshot {
    /// Calculate IRI cache hit rate
    pub fn iri_hit_rate(&self) -> f64 {
        let total = self.iri_hits + self.iri_misses;
        if total == 0 {
            0.0
        } else {
            self.iri_hits as f64 / total as f64
        }
    }
}

impl Default for GlobalCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalCacheManager {
    /// Create a new global cache manager with default configuration
    pub fn new() -> Self {
        Self::with_config(GlobalCacheConfig::default())
    }

    /// Create a new global cache manager with custom configuration
    pub fn with_config(config: GlobalCacheConfig) -> Self {
        // Create IRI cache - use simple constructor for now
        let iri_cache = Arc::new(RwLock::new(BoundedCache::new(config.iri_cache_max_size)));

        let stats = CacheStats::new();

        Self {
            iri_cache,
            stats,
            config,
        }
    }

    /// Get or create an IRI in the cache
    pub fn get_or_create_iri(&self, iri_str: String) -> Result<Arc<IRI>, OwlError> {
        // Try to get from cache first
        {
            let cache = self.iri_cache.read().map_err(|e| OwlError::CacheError {
                operation: "read".to_string(),
                message: format!("Failed to acquire read lock: {}", e),
            })?;
            if let Ok(Some(iri)) = cache.get(&iri_str) {
                self.stats.record_iri_hit();
                return Ok(Arc::new(iri));
            }
        }

        // Create new IRI and insert into cache
        let iri = IRI::new(iri_str.clone())?;

        {
            let cache = self.iri_cache.write().map_err(|e| OwlError::CacheError {
                operation: "write".to_string(),
                message: format!("Failed to acquire write lock: {}", e),
            })?;
            cache.insert(iri_str, iri.clone())?;
        }

        self.stats.record_iri_miss();
        Ok(Arc::new(iri))
    }

    /// Get an IRI from the cache if it exists
    pub fn get_iri(&self, iri_str: &str) -> Result<Option<Arc<IRI>>, OwlError> {
        let cache = self.iri_cache.read().map_err(|e| OwlError::CacheError {
            operation: "read".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        match cache.get(&iri_str.to_string())? {
            Some(iri) => {
                self.stats.record_iri_hit();
                Ok(Some(Arc::new(iri)))
            }
            None => Ok(None),
        }
    }

    /// Get cache statistics snapshot
    pub fn get_stats(&self) -> CacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Clear IRI cache
    pub fn clear_iri_cache(&self) -> Result<(), OwlError> {
        let mut cache = self.iri_cache.write().map_err(|e| OwlError::CacheError {
            operation: "write".to_string(),
            message: format!("Failed to acquire write lock: {}", e),
        })?;

        // Clear the cache by creating a new empty one
        *cache = BoundedCache::new(self.config.iri_cache_max_size);
        Ok(())
    }

    /// Get IRI cache size
    pub fn get_iri_cache_size(&self) -> Result<usize, OwlError> {
        let cache = self.iri_cache.read().map_err(|e| OwlError::CacheError {
            operation: "read".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        cache.len()
    }

    /// Check if cache is under memory pressure
    pub fn check_memory_pressure(&self) -> Result<bool, OwlError> {
        let size = self.get_iri_cache_size()?;
        let max_size = self.config.iri_cache_max_size;

        let pressure_ratio = size as f64 / max_size as f64;
        let is_under_pressure = pressure_ratio > self.config.memory_pressure_threshold;

        if is_under_pressure {
            self.stats.record_memory_pressure();
        }

        Ok(is_under_pressure)
    }
}

impl Clone for CacheStats {
    fn clone(&self) -> Self {
        Self {
            iri_hits: AtomicU64::new(self.iri_hits.load(Ordering::Relaxed)),
            iri_misses: AtomicU64::new(self.iri_misses.load(Ordering::Relaxed)),
            evictions: AtomicU64::new(self.evictions.load(Ordering::Relaxed)),
            memory_pressure_events: AtomicU64::new(
                self.memory_pressure_events.load(Ordering::Relaxed),
            ),
        }
    }
}

/// Global cache manager instance
static GLOBAL_CACHE_MANAGER: once_cell::sync::Lazy<GlobalCacheManager> =
    once_cell::sync::Lazy::new(GlobalCacheManager::new);

/// Get the global cache manager instance
pub fn global_cache_manager() -> &'static GlobalCacheManager {
    &GLOBAL_CACHE_MANAGER
}

/// Get or create an IRI using the global cache manager
pub fn get_or_create_iri(iri_str: String) -> Result<Arc<IRI>, OwlError> {
    global_cache_manager().get_or_create_iri(iri_str)
}

/// Get an IRI from the global cache manager
pub fn get_iri(iri_str: &str) -> Result<Option<Arc<IRI>>, OwlError> {
    global_cache_manager().get_iri(iri_str)
}

/// Get global cache statistics
pub fn global_cache_stats() -> CacheStatsSnapshot {
    global_cache_manager().get_stats()
}

/// Clear global IRI cache
pub fn clear_global_iri_cache() -> Result<(), OwlError> {
    global_cache_manager().clear_iri_cache()
}
