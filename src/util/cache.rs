//! Configurable caching system for OWL2 reasoner
//!
//! This module provides a flexible, library-friendly caching system with
//! configurable size limits and eviction policies. Designed for easy integration
//! into Rust applications without external dependencies.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use owl2_reasoner::cache::{BoundedCache, CacheConfig, LruStrategy};
//! use owl2_reasoner::cache::{EvictionStrategy, CacheMetadata};
//!
//! // Simple usage with sensible defaults
//! let cache = BoundedCache::<String, usize>::new(1000);
//!
//! // Advanced configuration with builder
//! let cache = BoundedCache::builder()
//!     .max_size(2000)
//!     .strategy(LruStrategy::new())
//!     .enable_stats(true)
//!     .build();
//!
//! // Custom strategy implementation
//! struct MyCustomStrategy;
//! impl EvictionStrategy for MyCustomStrategy {
//!     fn should_evict(&self, key: &str, value: &usize, metadata: &CacheMetadata) -> bool {
//!         // Custom eviction logic
//!         false
//!     }
//! }
//!
//! let custom_cache = BoundedCache::builder()
//!     .max_size(500)
//!     .strategy(MyCustomStrategy)
//!     .build();
//! ```

use crate::core::error::{OwlError, OwlResult};
use hashbrown::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cache configuration with sensible defaults
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in the cache
    max_size: usize,
    /// Whether to collect statistics
    enable_stats: bool,
    /// Whether to enable memory pressure detection
    enable_memory_pressure: bool,
    /// Memory threshold for pressure detection (as percentage)
    memory_pressure_threshold: f64,
    /// Cleanup interval for memory pressure detection
    cleanup_interval: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000, // Sensible default for most applications
            enable_stats: false,
            enable_memory_pressure: false,
            memory_pressure_threshold: 0.8, // 80% memory usage threshold
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// Builder for cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfigBuilder {
    config: CacheConfig,
}

impl Default for CacheConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheConfigBuilder {
    /// Create a new cache configuration builder with defaults
    pub fn new() -> Self {
        Self {
            config: CacheConfig::default(),
        }
    }

    /// Set maximum cache size
    pub fn max_size(mut self, size: usize) -> Self {
        self.config.max_size = size;
        self
    }

    /// Enable or disable statistics collection
    pub fn enable_stats(mut self, enabled: bool) -> Self {
        self.config.enable_stats = enabled;
        self
    }

    /// Enable or disable memory pressure detection
    pub fn enable_memory_pressure(mut self, enabled: bool) -> Self {
        self.config.enable_memory_pressure = enabled;
        self
    }

    /// Set memory pressure threshold (0.0 to 1.0)
    pub fn memory_pressure_threshold(mut self, threshold: f64) -> Self {
        self.config.memory_pressure_threshold = threshold.clamp(0.1, 0.95);
        self
    }

    /// Set cleanup interval for memory pressure detection
    pub fn cleanup_interval(mut self, interval: Duration) -> Self {
        self.config.cleanup_interval = interval;
        self
    }

    /// Build the cache configuration
    pub fn build(self) -> CacheConfig {
        self.config
    }
}

/// Cache entry metadata for eviction decisions
#[derive(Debug, Clone)]
pub struct CacheMetadata {
    /// When the entry was created
    pub created_at: Instant,
    /// When the entry was last accessed
    pub last_accessed: Instant,
    /// How many times the entry has been accessed
    pub access_count: usize,
    /// Estimated size of the entry in bytes
    pub estimated_size: usize,
}

impl CacheMetadata {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            created_at: now,
            last_accessed: now,
            access_count: 0,
            estimated_size: 0,
        }
    }

    /// Record an access to this entry
    fn record_access(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }
}

/// Lock-free cache statistics
#[derive(Debug)]
pub struct BoundedCacheStats {
    /// Total number of cache hits
    hits: AtomicU64,
    /// Total number of cache misses
    misses: AtomicU64,
    /// Total number of evictions
    evictions: AtomicU64,
    /// Current cache size
    current_size: AtomicUsize,
    /// Maximum cache size reached
    max_size_reached: AtomicUsize,
}

impl BoundedCacheStats {
    /// Create new cache statistics with atomic counters
    pub fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            current_size: AtomicUsize::new(0),
            max_size_reached: AtomicUsize::new(0),
        }
    }

    /// Calculate cache hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get a snapshot of current statistics
    pub fn snapshot(&self) -> BoundedCacheStatsSnapshot {
        BoundedCacheStatsSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            current_size: self.current_size.load(Ordering::Relaxed),
            max_size_reached: self.max_size_reached.load(Ordering::Relaxed),
        }
    }

    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an eviction
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Update current size
    pub fn update_size(&self, new_size: usize) {
        self.current_size.store(new_size, Ordering::Relaxed);

        // Update max size if needed
        let current_max = self.max_size_reached.load(Ordering::Relaxed);
        if new_size > current_max {
            self.max_size_reached.store(new_size, Ordering::Relaxed);
        }
    }

    /// Reset all statistics
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.current_size.store(0, Ordering::Relaxed);
        self.max_size_reached.store(0, Ordering::Relaxed);
    }
}

impl Default for BoundedCacheStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of cache statistics for display purposes
#[derive(Debug, Clone, Default)]
pub struct BoundedCacheStatsSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub current_size: usize,
    pub max_size_reached: usize,
}

impl BoundedCacheStatsSnapshot {
    /// Calculate cache hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Trait for cache eviction strategies
pub trait EvictionStrategy: Send + Sync {
    /// Determine if an entry should be evicted
    fn should_evict<K, V>(&self, key: &K, value: &V, metadata: &CacheMetadata) -> bool
    where
        K: Hash + Eq + fmt::Debug + ?Sized,
        V: Clone + fmt::Debug;

    /// Get the name of this strategy
    fn name(&self) -> &'static str;
}

/// Least Recently Used (LRU) eviction strategy
#[derive(Debug, Clone)]
pub struct LruStrategy;

impl LruStrategy {
    /// Create a new LRU strategy
    pub fn new() -> Self {
        Self
    }
}

impl Default for LruStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl EvictionStrategy for LruStrategy {
    fn should_evict<K, V>(&self, _key: &K, _value: &V, _metadata: &CacheMetadata) -> bool
    where
        K: Hash + Eq + fmt::Debug + ?Sized,
        V: Clone + fmt::Debug,
    {
        // For LRU, we don't decide per-entry - the cache implementation handles this
        // This method is more for complex strategies that need per-entry decisions
        false
    }

    fn name(&self) -> &'static str {
        "LRU"
    }
}

/// Least Frequently Used (LFU) eviction strategy
#[derive(Debug, Clone)]
pub struct LfuStrategy {
    /// Minimum access count before considering eviction
    min_access_count: usize,
}

impl LfuStrategy {
    /// Create a new LFU strategy
    pub fn new() -> Self {
        Self {
            min_access_count: 3,
        }
    }

    /// Set minimum access count
    pub fn min_access_count(mut self, count: usize) -> Self {
        self.min_access_count = count;
        self
    }
}

impl Default for LfuStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl EvictionStrategy for LfuStrategy {
    fn should_evict<K, V>(&self, _key: &K, _value: &V, metadata: &CacheMetadata) -> bool
    where
        K: Hash + Eq + fmt::Debug + ?Sized,
        V: Clone + fmt::Debug,
    {
        metadata.access_count < self.min_access_count
    }

    fn name(&self) -> &'static str {
        "LFU"
    }
}

/// First In First Out (FIFO) eviction strategy
#[derive(Debug, Clone)]
pub struct FifoStrategy;

impl FifoStrategy {
    /// Create a new FIFO strategy
    pub fn new() -> Self {
        Self
    }
}

impl Default for FifoStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl EvictionStrategy for FifoStrategy {
    fn should_evict<K, V>(&self, _key: &K, _value: &V, _metadata: &CacheMetadata) -> bool
    where
        K: Hash + Eq + fmt::Debug + ?Sized,
        V: Clone + fmt::Debug,
    {
        // FIFO is handled by the cache implementation
        false
    }

    fn name(&self) -> &'static str {
        "FIFO"
    }
}

/// Random eviction strategy
#[derive(Debug, Clone)]
pub struct RandomStrategy {
    /// Random seed (for reproducible testing)
    seed: u64,
}

impl RandomStrategy {
    /// Create a new random strategy
    pub fn new() -> Self {
        Self {
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Set random seed
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}

impl Default for RandomStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl EvictionStrategy for RandomStrategy {
    fn should_evict<K, V>(&self, _key: &K, _value: &V, _metadata: &CacheMetadata) -> bool
    where
        K: Hash + Eq + fmt::Debug + ?Sized,
        V: Clone + fmt::Debug,
    {
        // Random eviction is handled by the cache implementation
        false
    }

    fn name(&self) -> &'static str {
        "Random"
    }
}

/// A bounded cache with configurable eviction strategy
#[derive(Debug)]
pub struct BoundedCache<K, V, S = LruStrategy>
where
    K: Hash + Eq + fmt::Debug + Clone + Send + Sync + 'static,
    V: Clone + fmt::Debug + Send + Sync + 'static,
    S: EvictionStrategy + Send + Sync + 'static,
{
    config: CacheConfig,
    strategy: S,
    entries: Arc<RwLock<HashMap<K, (V, CacheMetadata)>>>,
    stats: Arc<BoundedCacheStats>,
    // For LRU: maintain access order
    access_order: Arc<RwLock<Vec<K>>>,
    // For FIFO: maintain insertion order
    insertion_order: Arc<RwLock<Vec<K>>>,
}

impl<K, V> BoundedCache<K, V, LruStrategy>
where
    K: Hash + Eq + fmt::Debug + Clone + Send + Sync + 'static,
    V: Clone + fmt::Debug + Send + Sync + 'static,
{
    /// Create a new cache with default configuration
    pub fn new(max_size: usize) -> Self {
        Self::with_config(CacheConfig {
            max_size,
            ..Default::default()
        })
    }

    /// Create a cache builder
    pub fn builder() -> CacheConfigBuilder {
        CacheConfigBuilder::new()
    }

    /// Create a cache from a builder
    pub fn from_builder(builder: CacheConfigBuilder) -> Self {
        Self::with_config(builder.build())
    }
}

impl<K, V, S> BoundedCache<K, V, S>
where
    K: Hash + Eq + fmt::Debug + Clone + Send + Sync + 'static,
    V: Clone + fmt::Debug + Send + Sync + 'static,
    S: EvictionStrategy + Default + Send + Sync + 'static,
{
    /// Create a new cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        let stats = BoundedCacheStats::new();
        stats.update_size(0); // Initialize with 0 size

        Self {
            config: config.clone(),
            strategy: S::default(),
            entries: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(stats),
            access_order: Arc::new(RwLock::new(Vec::new())),
            insertion_order: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get a value from the cache
    pub fn get(&self, key: &K) -> OwlResult<Option<V>> {
        // Fast path: try read lock first
        let entries = self.entries.read().map_err(|e| OwlError::CacheError {
            operation: "get".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        if let Some((value, _metadata)) = entries.get(key) {
            // Record hit and clone value while we have the read lock
            if self.config.enable_stats {
                self.stats.record_hit();
            }
            let value = value.clone();

            // Drop read lock before updating metadata to avoid contention
            drop(entries);

            // Slow path: upgrade to write lock only if we need to update metadata
            self.update_metadata_on_access(key)?;

            Ok(Some(value))
        } else {
            // Record miss
            if self.config.enable_stats {
                self.stats.record_miss();
            }
            Ok(None)
        }
    }

    /// Get a value from the cache using a borrowed reference (zero-copy lookup)
    pub fn get_by_ref<Q>(&self, key: &Q) -> OwlResult<Option<V>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // Fast path: try read lock first
        let entries = self.entries.read().map_err(|e| OwlError::CacheError {
            operation: "get_by_ref".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        if let Some((value, _)) = entries.get(key) {
            // Record hit and clone value while we have the read lock
            if self.config.enable_stats {
                self.stats.record_hit();
            }
            let value = value.clone();

            // Drop read lock before updating metadata to avoid contention
            drop(entries);

            // Convert borrowed key to owned key for metadata update
            // This is a limitation but necessary for LRU tracking
            if let Some(owned_key) = self.find_key_by_ref(key)? {
                self.update_metadata_on_access(&owned_key)?;
            }

            Ok(Some(value))
        } else {
            // Record miss
            if self.config.enable_stats {
                self.stats.record_miss();
            }
            Ok(None)
        }
    }

    /// Insert a value into the cache using borrowed reference (zero-copy insertion)
    pub fn insert_ref<Q>(&self, key: &Q, value: V) -> OwlResult<()>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        let owned_key = key.to_owned();
        self.insert(owned_key, value)
    }

    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) -> OwlResult<()> {
        let mut entries = self.entries.write().map_err(|e| OwlError::CacheError {
            operation: "insert".to_string(),
            message: format!("Failed to acquire write lock: {}", e),
        })?;

        // Check if we need to evict entries
        if entries.len() >= self.config.max_size {
            self.evict_entries(&mut entries)?;
        }

        // Insert the new entry
        let mut metadata = CacheMetadata::new();
        metadata.record_access();

        entries.insert(key.clone(), (value.clone(), metadata));

        // Update orders
        self.update_insertion_order(&key)?;
        self.update_access_order(&key)?;

        if self.config.enable_stats {
            self.stats.update_size(entries.len());
        }

        Ok(())
    }

    /// Remove a value from the cache
    pub fn remove(&self, key: &K) -> OwlResult<Option<V>> {
        let mut entries = self.entries.write().map_err(|e| OwlError::CacheError {
            operation: "remove".to_string(),
            message: format!("Failed to acquire write lock: {}", e),
        })?;

        let removed = entries.remove(key);

        // Clean up order tracking
        self.cleanup_order_tracking(key)?;

        if self.config.enable_stats {
            self.stats.update_size(entries.len());
        }

        Ok(removed.map(|(value, _)| value))
    }

    /// Clear all entries from the cache
    pub fn clear(&self) -> OwlResult<()> {
        let mut entries = self.entries.write().map_err(|e| OwlError::CacheError {
            operation: "clear".to_string(),
            message: format!("Failed to acquire write lock: {}", e),
        })?;

        entries.clear();

        // Clear order tracking
        if let Ok(mut order) = self.access_order.write() {
            order.clear();
        }

        if let Ok(mut order) = self.insertion_order.write() {
            order.clear();
        }

        if self.config.enable_stats {
            self.stats.update_size(0);
        }

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> BoundedCacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get the current cache size
    pub fn len(&self) -> OwlResult<usize> {
        self.entries
            .read()
            .map(|entries| entries.len())
            .map_err(|e| OwlError::CacheError {
                operation: "len".to_string(),
                message: format!("Failed to acquire read lock: {}", e),
            })
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> OwlResult<bool> {
        self.entries
            .read()
            .map(|entries| entries.is_empty())
            .map_err(|e| OwlError::CacheError {
                operation: "is_empty".to_string(),
                message: format!("Failed to acquire read lock: {}", e),
            })
    }

    /// Get the cache configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Get the eviction strategy
    pub fn strategy(&self) -> &S {
        &self.strategy
    }

    /// Evict entries based on the configured strategy
    fn evict_entries(&self, entries: &mut HashMap<K, (V, CacheMetadata)>) -> OwlResult<()> {
        let to_evict = self.select_entries_for_eviction(entries)?;

        for key in to_evict {
            entries.remove(&key);

            if self.config.enable_stats {
                self.stats.record_eviction();
                self.stats.update_size(entries.len());
            }

            // Clean up order tracking
            self.cleanup_order_tracking(&key)?;
        }

        Ok(())
    }

    /// Select entries for eviction based on strategy
    fn select_entries_for_eviction(
        &self,
        entries: &HashMap<K, (V, CacheMetadata)>,
    ) -> OwlResult<Vec<K>> {
        match self.strategy.name() {
            "LRU" => self.select_lru_entries(entries),
            "LFU" => self.select_lfu_entries(entries),
            "FIFO" => self.select_fifo_entries(entries),
            "Random" => self.select_random_entries(entries),
            _ => self.select_lru_entries(entries), // Default to LRU
        }
    }

    /// Select entries using LRU strategy
    fn select_lru_entries(&self, _entries: &HashMap<K, (V, CacheMetadata)>) -> OwlResult<Vec<K>> {
        let access_order = self.access_order.read().map_err(|e| OwlError::CacheError {
            operation: "lru_selection".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        // Evict the oldest 10% of entries
        let to_evict_count = (self.config.max_size / 10).max(1);
        Ok(access_order.iter().take(to_evict_count).cloned().collect())
    }

    /// Select entries using LFU strategy
    fn select_lfu_entries(&self, entries: &HashMap<K, (V, CacheMetadata)>) -> OwlResult<Vec<K>> {
        let mut entries_with_freq: Vec<_> = entries.iter().collect();

        // Sort by access count (ascending)
        entries_with_freq.sort_by_key(|(_, (_, metadata))| metadata.access_count);

        // Evict the least frequently used entries (10% of max size)
        let to_evict_count = (self.config.max_size / 10).max(1);
        Ok(entries_with_freq
            .into_iter()
            .take(to_evict_count)
            .map(|(key, _)| key.clone())
            .collect())
    }

    /// Select entries using FIFO strategy
    fn select_fifo_entries(&self, _entries: &HashMap<K, (V, CacheMetadata)>) -> OwlResult<Vec<K>> {
        let insertion_order = self
            .insertion_order
            .read()
            .map_err(|e| OwlError::CacheError {
                operation: "fifo_selection".to_string(),
                message: format!("Failed to acquire read lock: {}", e),
            })?;

        // Evict the oldest 10% of entries
        let to_evict_count = (self.config.max_size / 10).max(1);
        Ok(insertion_order
            .iter()
            .take(to_evict_count)
            .cloned()
            .collect())
    }

    /// Select entries using Random strategy
    fn select_random_entries(&self, entries: &HashMap<K, (V, CacheMetadata)>) -> OwlResult<Vec<K>> {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let keys: Vec<_> = entries.keys().cloned().collect();
        let to_evict_count = (self.config.max_size / 10).max(1);

        let mut rng = thread_rng();
        let selected: Vec<_> = keys
            .choose_multiple(&mut rng, to_evict_count)
            .cloned()
            .collect();

        Ok(selected)
    }

    /// Update access order for LRU tracking
    fn update_access_order(&self, key: &K) -> OwlResult<()> {
        let mut access_order = self
            .access_order
            .write()
            .map_err(|e| OwlError::CacheError {
                operation: "update_access_order".to_string(),
                message: format!("Failed to acquire write lock: {}", e),
            })?;

        // Remove key if it exists and re-add to end
        access_order.retain(|k| k != key);
        access_order.push(key.clone());

        Ok(())
    }

    /// Update insertion order for FIFO tracking
    fn update_insertion_order(&self, key: &K) -> OwlResult<()> {
        let mut insertion_order =
            self.insertion_order
                .write()
                .map_err(|e| OwlError::CacheError {
                    operation: "update_insertion_order".to_string(),
                    message: format!("Failed to acquire write lock: {}", e),
                })?;

        insertion_order.push(key.clone());
        Ok(())
    }

    /// Clean up order tracking when a key is removed
    fn cleanup_order_tracking(&self, key: &K) -> OwlResult<()> {
        // Clean up access order
        if let Ok(mut access_order) = self.access_order.write() {
            access_order.retain(|k| k != key);
        }

        // Clean up insertion order
        if let Ok(mut insertion_order) = self.insertion_order.write() {
            insertion_order.retain(|k| k != key);
        }

        Ok(())
    }

    /// Update metadata on access (only called when entry exists)
    fn update_metadata_on_access(&self, key: &K) -> OwlResult<()> {
        let mut entries = self.entries.write().map_err(|e| OwlError::CacheError {
            operation: "update_metadata_on_access".to_string(),
            message: format!("Failed to acquire write lock: {}", e),
        })?;

        if let Some((_, metadata)) = entries.get_mut(key) {
            metadata.record_access();
        }

        // Update access order for LRU tracking
        self.update_access_order(key)?;

        Ok(())
    }

    /// Find owned key by borrowed reference (helper for get_by_ref)
    fn find_key_by_ref<Q>(&self, key: &Q) -> OwlResult<Option<K>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let entries = self.entries.read().map_err(|e| OwlError::CacheError {
            operation: "find_key_by_ref".to_string(),
            message: format!("Failed to acquire read lock: {}", e),
        })?;

        // Find the owned key that matches the borrowed reference
        let owned_key = entries
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(k, _)| k.clone());

        Ok(owned_key)
    }
}

impl<K, V, S> BoundedCache<K, V, S>
where
    K: Hash + Eq + fmt::Debug + Clone + Send + Sync + 'static,
    V: Clone + fmt::Debug + Send + Sync + 'static,
    S: EvictionStrategy + Send + Sync + 'static,
{
    /// Create a cache with a custom strategy
    pub fn with_strategy(config: CacheConfig, strategy: S) -> Self {
        let stats = BoundedCacheStats::new();
        stats.update_size(0); // Initialize with 0 size

        Self {
            config: config.clone(),
            strategy,
            entries: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(stats),
            access_order: Arc::new(RwLock::new(Vec::new())),
            insertion_order: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl<K, V, S> Clone for BoundedCache<K, V, S>
where
    K: Hash + Eq + fmt::Debug + Clone + Send + Sync + 'static,
    V: Clone + fmt::Debug + Send + Sync + 'static,
    S: EvictionStrategy + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            strategy: self.strategy.clone(),
            entries: self.entries.clone(),
            stats: self.stats.clone(),
            access_order: self.access_order.clone(),
            insertion_order: self.insertion_order.clone(),
        }
    }
}
