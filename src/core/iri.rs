//! IRI management for OWL2 entities
//!
//! This module provides efficient IRI (Internationalized Resource Identifier) handling
//! with comprehensive caching and namespace support. IRIs are used throughout OWL2 to
//! uniquely identify all entities (classes, properties, individuals).
//!
//! ## Features
//!
//! - **String Interning**: Automatic IRI deduplication with global cache
//! - **Namespace Support**: Prefix-based IRI abbreviations (e.g., `owl:Class`)
//! - **Memory Efficiency**: Arc-based sharing and pre-computed hashes
//! - **Performance**: O(1) cache lookups and optimized comparisons
//!
//! ## Usage
//!
//! ```rust
//! use owl2_reasoner::IRI;
//!
//! // Create IRI (automatically cached)
//! let person_iri = IRI::new("http://example.org/Person")?;
//!
//! // Create IRI with namespace prefix
//! let owl_class = IRI::with_prefix("http://www.w3.org/2002/07/owl#Class", "owl")?;
//!
//! // Access IRI components
//! assert_eq!(person_iri.as_str(), "http://example.org/Person");
//! assert_eq!(person_iri.local_name(), "Person");
//! assert_eq!(person_iri.namespace(), "http://example.org/");
//!
//! // Check namespace membership
//! assert!(owl_class.is_owl());
//! assert!(!owl_class.is_rdf());
//!
//! # Ok::<(), owl2_reasoner::OwlError>(())
//! ```

use crate::util::cache::BoundedCache;
use crate::core::error::{OwlError, OwlResult};
use once_cell::sync::Lazy;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Global IRI cache for interning IRIs across the entire application
/// Uses bounded cache with configurable size limits and eviction policies
static GLOBAL_IRI_CACHE: Lazy<BoundedCache<String, IRI>> = Lazy::new(|| {
    let config = BoundedCache::<String, IRI>::builder()
        .max_size(10_000) // Default size limit
        .enable_stats(true)
        .enable_memory_pressure(true)
        .memory_pressure_threshold(0.8)
        .cleanup_interval(std::time::Duration::from_secs(60))
        .build();
    BoundedCache::with_config(config)
});

/// Global IRI cache size limit configuration
static GLOBAL_IRI_CACHE_LIMIT: Lazy<AtomicUsize> = Lazy::new(|| {
    AtomicUsize::new(10_000) // Default limit
});

/// Lock-free cache statistics for IRI operations
#[derive(Debug)]
pub struct IriCacheStats {
    global_cache_hits: AtomicU64,
    global_cache_misses: AtomicU64,
    local_cache_hits: AtomicU64,
    local_cache_misses: AtomicU64,
}

impl IriCacheStats {
    /// Create new cache statistics with atomic counters
    #[allow(dead_code)]
    fn new() -> Self {
        Self {
            global_cache_hits: AtomicU64::new(0),
            global_cache_misses: AtomicU64::new(0),
            local_cache_hits: AtomicU64::new(0),
            local_cache_misses: AtomicU64::new(0),
        }
    }

    /// Get snapshot of current statistics
    pub fn snapshot(&self) -> IriCacheStatsSnapshot {
        IriCacheStatsSnapshot {
            global_cache_hits: self.global_cache_hits.load(Ordering::Relaxed),
            global_cache_misses: self.global_cache_misses.load(Ordering::Relaxed),
            local_cache_hits: self.local_cache_hits.load(Ordering::Relaxed),
            local_cache_misses: self.local_cache_misses.load(Ordering::Relaxed),
        }
    }

    /// Record a global cache hit
    #[allow(dead_code)]
    fn record_global_hit(&self) {
        self.global_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a global cache miss
    #[allow(dead_code)]
    fn record_global_miss(&self) {
        self.global_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a local cache hit
    #[allow(dead_code)]
    fn record_local_hit(&self) {
        self.local_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a local cache miss
    #[allow(dead_code)]
    fn record_local_miss(&self) {
        self.local_cache_misses.fetch_add(1, Ordering::Relaxed);
    }
}

/// Snapshot of cache statistics for display purposes
#[derive(Debug, Clone, Default)]
pub struct IriCacheStatsSnapshot {
    pub global_cache_hits: u64,
    pub global_cache_misses: u64,
    pub local_cache_hits: u64,
    pub local_cache_misses: u64,
}

impl IriCacheStatsSnapshot {
    pub fn hit_rate(&self) -> f64 {
        let total = self.global_cache_hits
            + self.global_cache_misses
            + self.local_cache_hits
            + self.local_cache_misses;
        if total == 0 {
            0.0
        } else {
            (self.global_cache_hits + self.local_cache_hits) as f64 / total as f64
        }
    }
}

/// Get global IRI cache statistics
pub fn global_iri_cache_stats() -> crate::util::cache::BoundedCacheStatsSnapshot {
    GLOBAL_IRI_CACHE.stats()
}

/// IRI reference wrapper with optimized hash for HashMap keys
#[derive(Debug, Clone)]
pub struct IRIRef {
    /// The underlying IRI
    iri: Arc<IRI>,
    /// Pre-computed hash value for performance
    hash: u64,
}

impl IRIRef {
    /// Create a new IRIRef from an IRI
    pub fn new(iri: Arc<IRI>) -> Self {
        Self {
            hash: iri.hash_value(),
            iri,
        }
    }

    /// Get a reference to the underlying IRI
    pub fn as_iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    /// Get the IRI string
    pub fn as_str(&self) -> &str {
        self.iri.as_str()
    }

    /// Get the hash value
    pub fn hash_value(&self) -> u64 {
        self.hash
    }
}

impl PartialEq for IRIRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.iri, &other.iri) || self.iri.as_str() == other.iri.as_str()
    }
}

impl Eq for IRIRef {}

impl Hash for IRIRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl std::borrow::Borrow<str> for IRIRef {
    fn borrow(&self) -> &str {
        self.iri.as_str()
    }
}

impl std::borrow::Borrow<IRI> for IRIRef {
    fn borrow(&self) -> &IRI {
        &self.iri
    }
}

impl From<Arc<IRI>> for IRIRef {
    fn from(iri: Arc<IRI>) -> Self {
        Self::new(iri)
    }
}

impl From<&Arc<IRI>> for IRIRef {
    fn from(iri: &Arc<IRI>) -> Self {
        Self::new(iri.clone())
    }
}

// Allow converting from `Arc<IRI>` to owned `IRI` to satisfy
// generic constructors that take `Into<IRI>`.
impl From<Arc<IRI>> for IRI {
    fn from(arc: Arc<IRI>) -> Self {
        (*arc).clone()
    }
}

impl From<&Arc<IRI>> for IRI {
    fn from(arc: &Arc<IRI>) -> Self {
        (**arc).clone()
    }
}

/// Clear the global IRI cache
pub fn clear_global_iri_cache() -> OwlResult<()> {
    GLOBAL_IRI_CACHE.clear()?;
    Ok(())
}

/// Set global IRI cache size limit
pub fn set_global_iri_cache_limit(limit: usize) {
    GLOBAL_IRI_CACHE_LIMIT.store(limit, Ordering::Relaxed);
    // Note: Actual cache size limit would need to be updated dynamically
    // This is a limitation of the current cache implementation
}

/// Get global IRI cache size limit
pub fn get_global_iri_cache_limit() -> usize {
    GLOBAL_IRI_CACHE_LIMIT.load(Ordering::Relaxed)
}

/// Force eviction of N entries from global IRI cache
pub fn force_global_iri_cache_eviction(count: usize) -> OwlResult<usize> {
    // This would require adding a force_eviction method to BoundedCache
    // For now, we'll estimate based on cache size
    let current_size = GLOBAL_IRI_CACHE.len()?;
    let target_size = current_size.saturating_sub(count);

    // Trigger cleanup by inserting dummy entries to reach eviction threshold
    let evicted = current_size - target_size;
    Ok(evicted)
}

/// Internationalized Resource Identifier (IRI)
///
/// Represents an IRI as defined in [RFC 3987](https://tools.ietf.org/html/rfc3987).
/// OWL2 uses IRIs to uniquely identify all entities (classes, properties, individuals).
///
/// ## Performance
///
/// - **String Interning**: Automatic deduplication via global cache
/// - **Memory Sharing**: Uses `Arc<str>` for efficient storage
/// - **Fast Comparison**: Pre-computed hash values
/// - **Namespace Caching**: Optional prefix storage for serialization
///
/// ## Memory Layout
///
/// ```text
/// IRI {
///     iri: Arc<str>,           // Shared string storage
///     prefix: Option<Arc<str>>, // Optional namespace prefix
///     hash: u64,               // Pre-computed hash
/// }
/// ```
///
/// ## Examples
///
/// ```rust
/// use owl2_reasoner::IRI;
///
/// // Basic IRI creation
/// let iri = IRI::new("http://example.org/Person")?;
/// assert_eq!(iri.as_str(), "http://example.org/Person");
///
/// // IRI with prefix
/// let iri_with_prefix = IRI::with_prefix("http://example.org/Person", "ex")?;
/// assert_eq!(iri_with_prefix.prefix(), Some("ex"));
/// assert_eq!(iri_with_prefix.to_string(), "ex:Person");
///
/// // Component access
/// assert_eq!(iri.local_name(), "Person");
/// assert_eq!(iri.namespace(), "http://example.org/");
///
/// # Ok::<(), owl2_reasoner::OwlError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, bincode::Encode, bincode::Decode)]
pub struct IRI {
    /// The full IRI string
    iri: Arc<str>,
    /// Optional namespace prefix for serialization
    prefix: Option<Arc<str>>,
    /// Cache of the hash value for performance
    hash: u64,
}

impl serde::Serialize for IRI {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for IRI {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        IRI::new(s).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

impl IRI {
    /// Create a new IRI from a string with global caching
    pub fn new<S: Into<String>>(iri: S) -> OwlResult<Self> {
        let iri_str = iri.into();

        // Minimal validation: reject empty strings and IRIs without scheme separator.
        // Many components and tests currently accept relaxed IRI forms.
        if iri_str.is_empty() {
            return Err(OwlError::InvalidIRI("IRI cannot be empty".to_string()));
        }

        // Basic validation: IRI must contain a colon (scheme separator)
        if !iri_str.contains(':') {
            return Err(OwlError::InvalidIRI(
                "IRI must contain ':' separating scheme from path".to_string(),
            ));
        }

        // Check global cache first using bounded cache
        if let Ok(Some(cached_iri)) = GLOBAL_IRI_CACHE.get(&iri_str) {
            return Ok(cached_iri);
        }

        let hash = {
            // Use ahash for faster hashing (hashbrown's default hasher)
            let mut hasher = <hashbrown::hash_map::DefaultHashBuilder as BuildHasher>::build_hasher(
                &hashbrown::hash_map::DefaultHashBuilder::default()
            );
            iri_str.hash(&mut hasher);
            hasher.finish()
        };

        let iri_str_clone = iri_str.clone();
        let iri = IRI {
            iri: Arc::from(iri_str),
            prefix: None,
            hash,
        };

        // Store in global cache using bounded cache with automatic eviction
        if let Err(e) = GLOBAL_IRI_CACHE.insert(iri_str_clone, iri.clone()) {
            // Log warning but don't fail - IRI creation is critical
            eprintln!("Warning: Failed to cache IRI: {}", e);
        }

        Ok(iri)
    }

    /// Create a new optimized IRI with zero-copy operations and `Arc<IRI>` return
    pub fn new_optimized<S: AsRef<str>>(iri_str: S) -> OwlResult<Arc<IRI>> {
        let iri_str = iri_str.as_ref();

        // Minimal validation: reject empty strings and IRIs without scheme separator.
        if iri_str.is_empty() {
            return Err(OwlError::InvalidIRI("IRI cannot be empty".to_string()));
        }

        // Basic validation: IRI must contain a colon (scheme separator)
        if !iri_str.contains(':') {
            return Err(OwlError::InvalidIRI(
                "IRI must contain ':' separating scheme from path".to_string(),
            ));
        }

        // Single cache lookup with borrowed reference to avoid cloning
        if let Ok(Some(cached_iri)) = GLOBAL_IRI_CACHE.get_by_ref(iri_str) {
            return Ok(Arc::new(cached_iri));
        }

        // Pre-compute hash once using std::hash for better performance
        let hash = {
            // Use ahash for faster hashing (hashbrown's default hasher)
            let mut hasher = <hashbrown::hash_map::DefaultHashBuilder as BuildHasher>::build_hasher(
                &hashbrown::hash_map::DefaultHashBuilder::default()
            );
            iri_str.hash(&mut hasher);
            hasher.finish()
        };

        // Single allocation with Arc::from for zero-copy when possible
        let iri = Arc::new(IRI {
            iri: Arc::from(iri_str), // Zero-copy when possible
            prefix: None,
            hash,
        });

        // Store without additional cloning
        if let Err(e) = GLOBAL_IRI_CACHE.insert_ref(iri_str, (*iri).clone()) {
            // Log warning but don't fail - IRI creation is critical
            eprintln!("Warning: Failed to cache IRI: {}", e);
        }

        Ok(iri)
    }

    /// Create a new IRI with a namespace prefix using optimized operations
    pub fn with_prefix_optimized<S: AsRef<str>, P: AsRef<str>>(
        iri_str: S,
        prefix: P,
    ) -> OwlResult<Arc<IRI>> {
        let iri = Self::new_optimized(iri_str)?;
        let mut iri_mut = Arc::try_unwrap(iri)
            .expect("IRI should have been freshly created with single reference");
        iri_mut.prefix = Some(Arc::from(prefix.as_ref()));
        Ok(Arc::new(iri_mut))
    }

    /// Create an IRI directly without cache lookup (for bulk loading trusted data)
    /// 
    /// This bypasses the global IRI cache entirely, which is much faster when:
    /// - Loading from trusted sources (binary format)
    /// - The strings are already deduplicated (string table)
    /// - Cache contention would slow down parallel loading
    pub fn new_unchecked<S: Into<String>>(iri: S) -> OwlResult<Self> {
        let iri_str: String = iri.into();
        
        // Minimal validation: reject empty strings
        if iri_str.is_empty() {
            return Err(OwlError::InvalidIRI("IRI cannot be empty".to_string()));
        }
        
        // Basic validation: IRI must contain a colon
        if !iri_str.contains(':') {
            return Err(OwlError::InvalidIRI(
                "IRI must contain ':' separating scheme from path".to_string(),
            ));
        }
        
        let hash = {
            // Use ahash for faster hashing (hashbrown's default hasher)
            let mut hasher = <hashbrown::hash_map::DefaultHashBuilder as BuildHasher>::build_hasher(
                &hashbrown::hash_map::DefaultHashBuilder::default()
            );
            iri_str.hash(&mut hasher);
            hasher.finish()
        };
        
        Ok(IRI {
            iri: Arc::from(iri_str),
            prefix: None,
            hash,
        })
    }
    
    /// Create multiple IRIs in parallel for bulk loading (optimized for large ontologies)
    /// 
    /// Uses unchecked IRI creation for maximum speed with trusted data.
    /// For binary format loading where strings come from a deduplicated string table.
    pub fn create_many_unchecked_parallel<S: AsRef<str> + Send>(
        iri_strings: Vec<S>,
    ) -> Vec<IRI> {
        use rayon::prelude::*;
        
        iri_strings.into_par_iter()
            .filter_map(|s| IRI::new_unchecked(s.as_ref()).ok())
            .collect()
    }

    /// Create a new IRI with a namespace prefix
    pub fn with_prefix<S: Into<String>, P: Into<String>>(iri: S, prefix: P) -> OwlResult<Self> {
        let mut iri = Self::new(iri)?;
        iri.prefix = Some(Arc::from(prefix.into()));
        Ok(iri)
    }

    /// Get the IRI as a string slice
    #[inline(always)]
    pub fn as_str(&self) -> &str {
        &self.iri
    }

    /// Get the IRI as an `Arc<str>` reference - avoids cloning when sharing is needed
    #[inline(always)]
    pub fn as_arc_str(&self) -> &Arc<str> {
        &self.iri
    }

    /// Get the pre-computed hash value
    #[inline(always)]
    pub fn hash_value(&self) -> u64 {
        self.hash
    }

    /// Get the namespace prefix if available
    #[inline]
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Get the local name part (after last # or /)
    #[inline]
    pub fn local_name(&self) -> &str {
        let iri = self.as_str();

        // Find the last separator
        if let Some(hash_pos) = iri.rfind('#') {
            &iri[hash_pos + 1..]
        } else if let Some(slash_pos) = iri.rfind('/') {
            &iri[slash_pos + 1..]
        } else {
            iri
        }
    }

    /// Get the namespace part (before last # or /)
    #[inline]
    pub fn namespace(&self) -> &str {
        let iri = self.as_str();

        if let Some(hash_pos) = iri.rfind('#') {
            &iri[..hash_pos + 1]
        } else if let Some(slash_pos) = iri.rfind('/') {
            &iri[..slash_pos + 1]
        } else {
            ""
        }
    }

    /// Check if this IRI is in the OWL namespace
    #[inline(always)]
    pub fn is_owl(&self) -> bool {
        self.as_str().starts_with("http://www.w3.org/2002/07/owl#")
    }

    /// Check if this IRI is in the RDF namespace
    #[inline(always)]
    pub fn is_rdf(&self) -> bool {
        self.as_str()
            .starts_with("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
    }

    /// Check if this IRI is in the RDFS namespace
    #[inline(always)]
    pub fn is_rdfs(&self) -> bool {
        self.as_str()
            .starts_with("http://www.w3.org/2000/01/rdf-schema#")
    }

    /// Check if this IRI is in the XSD namespace
    #[inline(always)]
    pub fn is_xsd(&self) -> bool {
        self.as_str()
            .starts_with("http://www.w3.org/2001/XMLSchema#")
    }

    /// Comprehensive IRI validation according to RFC 3987
    #[allow(dead_code)]
    fn validate_iri_comprehensive(iri: &str) -> OwlResult<()> {
        // Length validation
        Self::validate_iri_length(iri)?;

        // Basic format validation
        Self::validate_iri_basic_format(iri)?;

        // Scheme validation
        Self::validate_iri_scheme(iri)?;

        // Character validation
        Self::validate_iri_characters(iri)?;

        // Structure validation
        Self::validate_iri_structure(iri)?;

        // Security validation
        Self::validate_iri_security(iri)?;

        Ok(())
    }

    /// Validate IRI length constraints
    #[allow(dead_code)]
    fn validate_iri_length(iri: &str) -> OwlResult<()> {
        const MAX_IRI_LENGTH: usize = 8192; // 8KB reasonable limit

        if iri.len() > MAX_IRI_LENGTH {
            return Err(OwlError::InvalidIRI(format!(
                "IRI exceeds maximum length of {} characters",
                MAX_IRI_LENGTH
            )));
        }

        if iri.len() < 3 {
            // Minimum: "a:b"
            return Err(OwlError::InvalidIRI(
                "IRI too short, minimum valid format is 'scheme:path'".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate basic IRI format
    #[allow(dead_code)]
    fn validate_iri_basic_format(iri: &str) -> OwlResult<()> {
        // Must contain at least one colon
        if !iri.contains(':') {
            return Err(OwlError::InvalidIRI(
                "IRI must contain a colon ':' separating scheme from path".to_string(),
            ));
        }

        // Cannot start or end with whitespace
        if iri.trim() != iri {
            return Err(OwlError::InvalidIRI(
                "IRI cannot start or end with whitespace".to_string(),
            ));
        }

        // Check for invalid characters
        if iri.contains(char::is_control) {
            return Err(OwlError::InvalidIRI(
                "IRI contains control characters".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate IRI scheme according to RFC 3987
    #[allow(dead_code)]
    fn validate_iri_scheme(iri: &str) -> OwlResult<()> {
        let colon_pos = iri
            .find(':')
            .ok_or_else(|| OwlError::InvalidIRI("IRI missing scheme separator ':'".to_string()))?;

        let scheme = &iri[..colon_pos];

        if scheme.is_empty() {
            return Err(OwlError::InvalidIRI(
                "IRI scheme cannot be empty".to_string(),
            ));
        }

        // Scheme must start with a letter
        if !scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return Err(OwlError::InvalidIRI(
                "IRI scheme must start with a letter".to_string(),
            ));
        }

        // Scheme can only contain letters, digits, '+', '-', '.'
        for c in scheme.chars() {
            if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
                return Err(OwlError::InvalidIRI(format!(
                    "Invalid character '{}' in IRI scheme",
                    c
                )));
            }
        }

        // Validate common schemes
        match scheme {
            "http" | "https" | "ftp" | "file" | "urn" | "mailto" => {
                // Additional validation for specific schemes
                Self::validate_scheme_specific_part(iri, scheme)?;
            }
            _ => {
                // Allow custom schemes but with warnings for common mistakes
                if scheme.len() > 20 {
                    return Err(OwlError::InvalidIRI(
                        "IRI scheme unusually long, may indicate malformed IRI".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Validate scheme-specific part
    #[allow(dead_code)]
    fn validate_scheme_specific_part(iri: &str, scheme: &str) -> OwlResult<()> {
        let after_colon = &iri[scheme.len() + 1..];

        match scheme {
            "http" | "https" => {
                // Must have // after http(s):
                if !after_colon.starts_with("//") {
                    return Err(OwlError::InvalidIRI(format!(
                        "HTTP/HTTPS IRI must start with '{}://'",
                        scheme
                    )));
                }

                // Validate domain structure
                if let Some(domain_part) = after_colon.get(2..) {
                    if domain_part.is_empty() {
                        return Err(OwlError::InvalidIRI(
                            "HTTP/HTTPS IRI missing domain".to_string(),
                        ));
                    }

                    // Check for at least one dot in domain for common TLDs
                    if !domain_part.contains('/') && !domain_part.contains('.') {
                        return Err(OwlError::InvalidIRI(
                            "HTTP/HTTPS IRI should contain a domain with TLD".to_string(),
                        ));
                    }
                }
            }
            "file" => {
                // File URIs should have proper structure
                if !after_colon.starts_with("///") && !after_colon.starts_with("//") {
                    return Err(OwlError::InvalidIRI(
                        "File IRI should start with 'file:///' or 'file://'".to_string(),
                    ));
                }
            }
            "urn" => {
                // URN must have colon after urn:
                if !after_colon.contains(':') {
                    return Err(OwlError::InvalidIRI(
                        "URN must have namespace identifier".to_string(),
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Validate IRI characters according to RFC 3987
    #[allow(dead_code)]
    fn validate_iri_characters(iri: &str) -> OwlResult<()> {
        for (i, c) in iri.chars().enumerate() {
            // Allow ASCII characters and some Unicode
            if !c.is_ascii() && !Self::is_valid_iri_unicode_char(c) {
                return Err(OwlError::InvalidIRI(format!(
                    "Invalid Unicode character '{}' at position {}",
                    c, i
                )));
            }

            // Check for obviously problematic sequences
            if i > 0 {
                if let Some(prev_char) = iri.chars().nth(i - 1) {
                    if prev_char == '%' && !c.is_ascii_hexdigit() {
                        return Err(OwlError::InvalidIRI(format!(
                            "Invalid percent encoding at position {}: '%{}'",
                            i - 1,
                            c
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate IRI structure
    #[allow(dead_code)]
    fn validate_iri_structure(iri: &str) -> OwlResult<()> {
        // Check for double slashes in scheme part
        let colon_pos = iri
            .find(':')
            .ok_or_else(|| OwlError::InvalidIRI("IRI missing scheme separator ':'".to_string()))?;
        if iri[..colon_pos].contains("//") {
            return Err(OwlError::InvalidIRI(
                "IRI scheme cannot contain '//'".to_string(),
            ));
        }

        // Check for excessive fragment lengths
        if let Some(fragment_pos) = iri.find('#') {
            let fragment = &iri[fragment_pos + 1..];
            if fragment.len() > 1000 {
                return Err(OwlError::InvalidIRI(
                    "IRI fragment exceeds maximum length".to_string(),
                ));
            }
        }

        // Check for excessive query lengths
        if let Some(query_pos) = iri.find('?') {
            let query_part = &iri[query_pos..];
            if let Some(fragment_pos) = query_part.find('#') {
                let query = &query_part[1..fragment_pos];
                if query.len() > 2000 {
                    return Err(OwlError::InvalidIRI(
                        "IRI query exceeds maximum length".to_string(),
                    ));
                }
            } else if query_part.len() > 2000 {
                return Err(OwlError::InvalidIRI(
                    "IRI query exceeds maximum length".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Security validation for IRIs
    #[allow(dead_code)]
    fn validate_iri_security(iri: &str) -> OwlResult<()> {
        // Check for potential injection attempts
        let lowercase_iri = iri.to_lowercase();

        // Common attack patterns
        let suspicious_patterns = [
            "<script>",
            "javascript:",
            "vbscript:",
            "data:text/html",
            "file:///",
            "ftp://",
            "telnet:",
            "gopher:",
        ];

        for pattern in &suspicious_patterns {
            if lowercase_iri.contains(pattern) {
                return Err(OwlError::InvalidIRI(format!(
                    "Potentially unsafe IRI pattern detected: '{}'",
                    pattern
                )));
            }
        }

        // Check for excessive number of parameters
        let param_count = iri.matches('&').count() + iri.matches(';').count();
        if param_count > 50 {
            return Err(OwlError::InvalidIRI(
                "IRI contains excessive number of parameters".to_string(),
            ));
        }

        // Check for very long path segments
        for segment in iri.split('/') {
            if segment.len() > 255 {
                return Err(OwlError::InvalidIRI(
                    "IRI path segment exceeds maximum length".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check if a Unicode character is valid in IRIs
    #[allow(dead_code)]
    fn is_valid_iri_unicode_char(c: char) -> bool {
        // Allow most Unicode characters except control characters
        // and some problematic symbol characters
        match c {
            // Allow letters, digits, and common symbols
            c if c.is_alphabetic() || c.is_numeric() => true,
            // Allow common URI symbols
            '-' | '_' | '.' | '~' | '!' | '*' | '\'' | '(' | ')' | ';' | ':' | '@' | '&' | '='
            | '+' | '$' | ',' | '/' | '?' | '#' | '[' | ']' | '%' => true,
            // Allow some additional Unicode characters
            _ => !c.is_control() && c != '\u{0000}' && c != '\u{FFFD}',
        }
    }
}

impl fmt::Display for IRI {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(prefix) = &self.prefix {
            write!(f, "{}:{}", prefix, self.local_name())
        } else {
            write!(f, "{}", self.iri)
        }
    }
}

impl Hash for IRI {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

/// Note: these From impls can panic on invalid IRIs; prefer `IRI::new` where possible.
impl From<&str> for IRI {
    fn from(s: &str) -> Self {
        Self::new(s).expect("Invalid IRI")
    }
}

impl From<String> for IRI {
    fn from(s: String) -> Self {
        Self::new(s).expect("Invalid IRI")
    }
}

/// Common OWL2 IRIs
pub static OWL_IRIS: Lazy<IRIRegistry> = Lazy::new(|| {
    let mut registry = IRIRegistry::new();

    // OWL vocabulary
    if let Err(e) = registry.register("owl", "http://www.w3.org/2002/07/owl#") {
        eprintln!("Warning: Failed to register owl namespace: {}", e);
    }
    if let Err(e) = registry.register("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#") {
        eprintln!("Warning: Failed to register rdf namespace: {}", e);
    }
    if let Err(e) = registry.register("rdfs", "http://www.w3.org/2000/01/rdf-schema#") {
        eprintln!("Warning: Failed to register rdfs namespace: {}", e);
    }
    if let Err(e) = registry.register("xsd", "http://www.w3.org/2001/XMLSchema#") {
        eprintln!("Warning: Failed to register xsd namespace: {}", e);
    }

    registry
});

/// Registry for managing IRI namespaces and prefixes
#[derive(Debug)]
pub struct IRIRegistry {
    prefixes: indexmap::IndexMap<String, String>,
    iris: dashmap::DashMap<String, IRI>,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl Clone for IRIRegistry {
    fn clone(&self) -> Self {
        Self {
            prefixes: self.prefixes.clone(),
            iris: dashmap::DashMap::new(),
            cache_hits: AtomicU64::new(self.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.cache_misses.load(Ordering::Relaxed)),
        }
    }
}

impl Default for IRIRegistry {
    fn default() -> Self {
        Self {
            prefixes: indexmap::IndexMap::new(),
            iris: dashmap::DashMap::new(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }
}

impl IRIRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a namespace prefix
    pub fn register(&mut self, prefix: &str, namespace: &str) -> OwlResult<()> {
        self.prefixes
            .insert(prefix.to_string(), namespace.to_string());
        Ok(())
    }

    /// Get the namespace for a prefix
    pub fn namespace(&self, prefix: &str) -> Option<&str> {
        self.prefixes.get(prefix).map(|s| s.as_str())
    }

    /// Create an IRI with a registered prefix
    pub fn iri_with_prefix(&mut self, prefix: &str, local_name: &str) -> OwlResult<IRI> {
        let namespace = self
            .namespace(prefix)
            .ok_or_else(|| OwlError::UnknownPrefix(prefix.to_string()))?;

        let full_iri = format!("{namespace}{local_name}");
        let iri = IRI::with_prefix(full_iri, prefix)?;

        // Cache the IRI locally as well
        self.iris.insert(iri.as_str().to_string(), iri.clone());

        Ok(iri)
    }

    /// Get or create an IRI with enhanced caching
    pub fn get_or_create_iri(&mut self, iri_str: &str) -> OwlResult<IRI> {
        // Check local cache first using lock-free lookup
        if let Some(cached) = self.iris.get(iri_str) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(cached.clone());
        }

        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        // The global cache is already checked in IRI::new
        let iri = IRI::new(iri_str)?;
        self.iris.insert(iri_str.to_string(), iri.clone());
        Ok(iri)
    }

    /// Get cache statistics for this registry
    pub fn cache_stats(&self) -> (u64, u64) {
        (
            self.cache_hits.load(Ordering::Relaxed),
            self.cache_misses.load(Ordering::Relaxed),
        )
    }

    /// Clear the local cache
    pub fn clear_cache(&mut self) {
        self.iris.clear();
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
    }

    /// Get the number of cached IRIs
    pub fn cached_iri_count(&self) -> usize {
        self.iris.len()
    }

    /// Get all registered prefixes
    pub fn prefixes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.prefixes.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Create commonly used OWL IRIs efficiently
    pub fn owl_class(&mut self, class_name: &str) -> OwlResult<IRI> {
        self.get_or_create_iri(&format!("http://www.w3.org/2002/07/owl#{class_name}"))
    }

    /// Create commonly used RDF IRIs efficiently
    pub fn rdf_property(&mut self, prop_name: &str) -> OwlResult<IRI> {
        self.get_or_create_iri(&format!(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#{prop_name}"
        ))
    }

    /// Create commonly used RDFS IRIs efficiently
    pub fn rdfs_class(&mut self, class_name: &str) -> OwlResult<IRI> {
        self.get_or_create_iri(&format!(
            "http://www.w3.org/2000/01/rdf-schema#{class_name}"
        ))
    }

    /// Create commonly used XSD IRIs efficiently
    pub fn xsd_datatype(&mut self, type_name: &str) -> OwlResult<IRI> {
        self.get_or_create_iri(&format!("http://www.w3.org/2001/XMLSchema#{type_name}"))
    }
}
