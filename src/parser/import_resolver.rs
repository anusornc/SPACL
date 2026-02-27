//! OWL2 Import Resolution System
//!
//! Provides comprehensive support for resolving owl:imports statements in OWL2 ontologies.
//! Supports multiple import sources, caching, circular import detection, and concurrent resolution.

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::ParserFactory;
use hashbrown::HashMap;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Import resolution configuration
#[derive(Debug, Clone)]
pub struct ImportResolverConfig {
    /// Maximum depth of import resolution
    pub max_depth: usize,
    /// Timeout for individual import resolution
    pub timeout: Duration,
    /// Maximum cache size
    pub max_cache_size: usize,
    /// Time-to-live for cache entries
    pub cache_ttl: Duration,
    /// Whether to enable concurrent import resolution
    pub enable_concurrent_resolution: bool,
    /// Maximum number of concurrent resolutions
    pub max_concurrent_resolutions: usize,
    /// Whether to follow HTTP redirects
    pub follow_redirects: bool,
    /// Maximum number of redirects to follow
    pub max_redirects: usize,
    /// User agent for HTTP requests
    pub user_agent: String,
}

impl Default for ImportResolverConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            timeout: Duration::from_secs(30),
            max_cache_size: 100,
            cache_ttl: Duration::from_secs(3600), // 1 hour
            enable_concurrent_resolution: true,
            max_concurrent_resolutions: 4,
            follow_redirects: true,
            max_redirects: 5,
            user_agent: "OWL2-Reasoner/0.1.0".to_string(),
        }
    }
}

/// Represents a cached ontology with metadata
#[derive(Debug, Clone)]
pub struct CachedOntology {
    /// The cached ontology
    pub ontology: Ontology,
    /// When this entry was cached
    pub cached_at: Instant,
    /// Time-to-live for this cache entry
    pub ttl: Duration,
    /// Size estimate in bytes
    pub size_estimate: usize,
    /// Source IRI
    pub source_iri: IRI,
}

impl CachedOntology {
    /// Check if this cache entry is still valid
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }

    /// Create a new cached ontology
    pub fn new(ontology: Ontology, source_iri: IRI, ttl: Duration) -> Self {
        let size_estimate = estimate_ontology_size(&ontology);
        Self {
            ontology,
            cached_at: Instant::now(),
            ttl,
            size_estimate,
            source_iri,
        }
    }
}

/// Estimates the memory size of an ontology for cache management.
///
/// This function provides a rough estimate of the ontology's memory footprint
/// based on the number of entities and axioms. Used for cache size management
/// and eviction policies.
///
/// # Parameters
/// - `ontology`: The ontology to estimate the size of
///
/// # Returns
/// Returns an estimated size in bytes.
fn estimate_ontology_size(ontology: &Ontology) -> usize {
    // Basic size estimation based on entity counts
    let base_size = std::mem::size_of::<Ontology>();
    let entities_size = (ontology.classes().len()
        + ontology.object_properties().len()
        + ontology.data_properties().len()
        + ontology.named_individuals().len())
        * 64; // Rough estimate per entity

    let axioms_size = ontology.axiom_count() * 128; // Rough estimate per axiom

    base_size + entities_size + axioms_size
}

/// Import resolution statistics
#[derive(Debug, Default, Clone)]
pub struct ImportResolutionStats {
    /// Number of imports resolved
    pub imports_resolved: usize,
    /// Number of cache hits
    pub cache_hits: usize,
    /// Number of cache misses
    pub cache_misses: usize,
    /// Number of failed resolutions
    pub failed_resolutions: usize,
    /// Total time spent resolving imports
    pub total_resolution_time: Duration,
    /// Number of circular dependencies detected
    pub circular_dependencies_detected: usize,
    /// Number of concurrent resolutions
    pub concurrent_resolutions: usize,
}

/// Import source trait for different resolution strategies
pub trait ImportSource: Send + Sync {
    /// Check if this source can resolve the given IRI
    fn can_resolve(&self, iri: &IRI) -> bool;

    /// Resolve the given IRI to an ontology
    fn resolve(&self, iri: &IRI, config: &ImportResolverConfig) -> OwlResult<Ontology>;

    /// Get the name of this source
    fn name(&self) -> &'static str;
}

/// File system import source
pub struct FileSystemImportSource {
    /// Base directories to search for ontologies
    base_directories: Vec<PathBuf>,
    /// File extensions to try
    file_extensions: Vec<&'static str>,
}

impl FileSystemImportSource {
    /// Create a new file system import source
    pub fn new() -> Self {
        Self {
            base_directories: vec![PathBuf::from(".")],
            file_extensions: vec!["owl", "rdf", "ttl", "xml", "owx"],
        }
    }

    /// Add a base directory to search
    pub fn add_base_directory(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.base_directories.push(path.as_ref().to_path_buf());
        self
    }

    /// Add file extensions to try
    pub fn add_file_extension(&mut self, extension: &'static str) -> &mut Self {
        self.file_extensions.push(extension);
        self
    }

    /// Try to find a file for the given IRI
    fn find_file(&self, iri: &IRI) -> Option<PathBuf> {
        let iri_str = iri.as_str();

        // Try to extract a filename from the IRI
        let filename = if iri_str.contains('/') {
            iri_str.split('/').next_back().unwrap_or("ontology.owl")
        } else {
            iri_str
        };

        // Search in base directories
        for base_dir in &self.base_directories {
            // Try exact filename first
            let exact_path = base_dir.join(filename);
            if exact_path.exists() {
                return Some(exact_path);
            }

            // Try with different extensions
            for ext in &self.file_extensions {
                let path_with_ext = base_dir.join(format!("{}.{}", filename, ext));
                if path_with_ext.exists() {
                    return Some(path_with_ext);
                }
            }
        }

        None
    }
}

impl Default for FileSystemImportSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportSource for FileSystemImportSource {
    fn can_resolve(&self, iri: &IRI) -> bool {
        // Can resolve file:// IRIs and relative IRIs
        let iri_str = iri.as_str();
        iri_str.starts_with("file://") || !iri_str.contains("://")
    }

    fn resolve(&self, iri: &IRI, _config: &ImportResolverConfig) -> OwlResult<Ontology> {
        let file_path = self
            .find_file(iri)
            .ok_or_else(|| OwlError::ImportResolutionError {
                iri: iri.clone(),
                message: format!("File not found for IRI: {}", iri),
            })?;

        // Determine parser based on file extension
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("owl");

        let parser = ParserFactory::for_file_extension(extension).ok_or_else(|| {
            OwlError::ImportResolutionError {
                iri: iri.clone(),
                message: format!("No parser available for file extension: {}", extension),
            }
        })?;

        parser.parse_file(&file_path)
    }

    fn name(&self) -> &'static str {
        "FileSystem"
    }
}

/// HTTP import source
#[cfg(feature = "http")]
pub struct HttpImportSource {
    /// HTTP client
    client: reqwest::blocking::Client,
}

#[cfg(feature = "http")]
impl HttpImportSource {
    /// Create a new HTTP import source
    pub fn new() -> OwlResult<Self> {
        let dummy_iri = IRI::new("http://dummy").unwrap_or_else(|_| {
            IRI::new("http://localhost/dummy").unwrap_or_else(|_| {
                IRI::new("urn:dummy").expect("Fallback IRI creation should never fail")
            })
        });

        // Try to create a blocking client to avoid async runtime issues
        let client = reqwest::blocking::Client::builder()
            .user_agent("OWL2-Reasoner/0.1.0")
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| OwlError::ImportResolutionError {
                iri: dummy_iri,
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self { client })
    }

    /// Extract content type from response
    fn extract_content_type(response: &reqwest::blocking::Response) -> Option<String> {
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).to_string())
    }
}

#[cfg(feature = "http")]
impl Default for HttpImportSource {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            // Fallback to basic client if configured client fails
            let client = reqwest::blocking::Client::builder()
                .user_agent("OWL2-Reasoner/0.1.0")
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            Self { client }
        })
    }
}

#[cfg(feature = "http")]
impl ImportSource for HttpImportSource {
    fn can_resolve(&self, iri: &IRI) -> bool {
        let iri_str = iri.as_str();
        iri_str.starts_with("http://") || iri_str.starts_with("https://")
    }

    fn resolve(&self, iri: &IRI, config: &ImportResolverConfig) -> OwlResult<Ontology> {
        let response = self
            .client
            .get(iri.as_str())
            .header("User-Agent", &config.user_agent)
            .timeout(config.timeout)
            .send()
            .map_err(|e| OwlError::ImportResolutionError {
                iri: iri.clone(),
                message: format!("HTTP request failed: {}", e),
            })?;

        if !response.status().is_success() {
            return Err(OwlError::ImportResolutionError {
                iri: iri.clone(),
                message: format!("HTTP request failed with status: {}", response.status()),
            });
        }

        let content_type = Self::extract_content_type(&response);
        let content = response
            .text()
            .map_err(|e| OwlError::ImportResolutionError {
                iri: iri.clone(),
                message: format!("Failed to read response content: {}", e),
            })?;

        // Try to determine content type
        let content_type = content_type.or_else(|| {
            // Try to auto-detect from content
            ParserFactory::auto_detect(&content).map(|p| p.format_name().to_string())
        });

        let parser = if let Some(content_type) = content_type {
            ParserFactory::for_content_type(&content_type)
                .or_else(|| ParserFactory::auto_detect(&content))
        } else {
            ParserFactory::auto_detect(&content)
        };

        let parser = parser.ok_or_else(|| OwlError::ImportResolutionError {
            iri: iri.clone(),
            message: "Could not determine parser for HTTP content".to_string(),
        })?;

        parser.parse_str(&content)
    }

    fn name(&self) -> &'static str {
        "HTTP"
    }
}

/// Import cache implementation
pub struct ImportCache {
    /// Cached ontologies
    entries: HashMap<IRI, CachedOntology>,
    /// Current total size of cached entries
    current_size: usize,
    /// Maximum cache size
    max_size: usize,
    /// Cache lock for thread safety
    lock: RwLock<()>,
}

impl ImportCache {
    /// Create a new import cache
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            current_size: 0,
            max_size,
            lock: RwLock::new(()),
        }
    }

    /// Get a cached ontology if it exists and is valid
    pub fn get(&self, iri: &IRI) -> Option<CachedOntology> {
        let _lock = self.lock.read();
        self.entries.get(iri).and_then(|cached| {
            if cached.is_valid() {
                Some(cached.clone())
            } else {
                None
            }
        })
    }

    /// Put an ontology in the cache
    pub fn put(&mut self, iri: IRI, cached: CachedOntology) {
        // Remove expired entries first
        self.remove_expired();

        let _lock = self.lock.write();

        // If cache is full, remove least recently used entries
        while self.current_size + cached.size_estimate > self.max_size && !self.entries.is_empty() {
            if let Some((lru_iri, _)) = self.find_lru_entry() {
                if let Some(removed) = self.entries.remove(&lru_iri) {
                    self.current_size -= removed.size_estimate;
                }
            } else {
                break;
            }
        }

        // Insert new entry
        self.current_size += cached.size_estimate;
        self.entries.insert(iri, cached);
    }

    /// Remove expired entries from cache
    fn remove_expired(&mut self) {
        let expired: Vec<IRI> = self
            .entries
            .iter()
            .filter(|(_, cached)| !cached.is_valid())
            .map(|(iri, _)| iri.clone())
            .collect();

        for iri in expired {
            if let Some(removed) = self.entries.remove(&iri) {
                self.current_size -= removed.size_estimate;
            }
        }
    }

    /// Find least recently used entry
    fn find_lru_entry(&self) -> Option<(IRI, Instant)> {
        self.entries
            .iter()
            .min_by_key(|(_, cached)| cached.cached_at)
            .map(|(iri, cached)| (iri.clone(), cached.cached_at))
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        let _lock = self.lock.write();
        self.entries.clear();
        self.current_size = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> ImportCacheStats {
        let _lock = self.lock.read();
        ImportCacheStats {
            entries: self.entries.len(),
            total_size: self.current_size,
            max_size: self.max_size,
            hit_rate: 0.0, // Would need to track hits/misses
        }
    }
}

/// Import cache statistics
#[derive(Debug, Clone)]
pub struct ImportCacheStats {
    /// Number of entries in cache
    pub entries: usize,
    /// Total size of cached entries
    pub total_size: usize,
    /// Maximum cache size
    pub max_size: usize,
    /// Cache hit rate
    pub hit_rate: f64,
}

/// Main import resolver
pub struct ImportResolver {
    /// Import sources
    sources: Vec<Box<dyn ImportSource>>,
    /// Import cache
    cache: ImportCache,
    /// Configuration
    config: ImportResolverConfig,
    /// Resolution statistics
    stats: Arc<RwLock<ImportResolutionStats>>,
    /// Currently resolving imports (for circular dependency detection)
    resolving: Arc<RwLock<HashSet<IRI>>>,
}

impl ImportResolver {
    /// Create a new import resolver with default configuration
    pub fn new() -> OwlResult<Self> {
        Self::with_config(ImportResolverConfig::default())
    }

    /// Create a new import resolver with custom configuration
    pub fn with_config(config: ImportResolverConfig) -> OwlResult<Self> {
        let mut sources: Vec<Box<dyn ImportSource>> = Vec::new();

        // Add default sources
        sources.push(Box::new(FileSystemImportSource::default()));

        // Try to add HTTP source if reqwest is available
        #[cfg(feature = "http")]
        {
            if let Ok(http_source) = HttpImportSource::new() {
                sources.push(Box::new(http_source));
            } else {
                log::warn!(
                    "HTTP import source initialization failed, continuing without HTTP support"
                );
            }
        }

        Ok(Self {
            sources,
            cache: ImportCache::new(config.max_cache_size),
            config,
            stats: Arc::new(RwLock::new(ImportResolutionStats::default())),
            resolving: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Add a custom import source
    pub fn add_source(&mut self, source: Box<dyn ImportSource>) {
        self.sources.push(source);
    }

    /// Resolve imports for an ontology
    pub fn resolve_imports(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        self.resolve_imports_with_depth(ontology, 0)
    }

    /// Resolve imports with depth tracking
    fn resolve_imports_with_depth(
        &mut self,
        ontology: &mut Ontology,
        depth: usize,
    ) -> OwlResult<()> {
        if depth > self.config.max_depth {
            let fallback_iri = IRI::new("unknown").unwrap_or_else(|_| {
                IRI::new("urn:unknown").unwrap_or_else(|_| {
                    IRI::new("http://localhost/unknown")
                        .expect("Fallback IRI creation should never fail")
                })
            });

            return Err(OwlError::ImportResolutionError {
                iri: ontology.iri().cloned().unwrap_or(fallback_iri),
                message: format!("Maximum import depth {} exceeded", self.config.max_depth),
            });
        }

        // Get imports from the ontology
        let imports: Vec<IRI> = ontology
            .imports()
            .iter()
            .map(|iri| (**iri).clone())
            .collect();

        if imports.is_empty() {
            return Ok(());
        }

        // Resolve each import
        for import_iri in imports {
            if let Err(e) = self.resolve_single_import(&import_iri, ontology, depth) {
                log::warn!("Failed to resolve import {}: {}", import_iri, e);

                // Update statistics
                let mut stats = self.stats.write();
                stats.failed_resolutions += 1;
            }
        }

        Ok(())
    }

    /// Resolve a single import
    fn resolve_single_import(
        &mut self,
        import_iri: &IRI,
        target_ontology: &mut Ontology,
        depth: usize,
    ) -> OwlResult<()> {
        let start_time = Instant::now();

        // Check for circular dependencies
        {
            let resolving = self.resolving.read();
            if resolving.contains(import_iri) {
                let mut stats = self.stats.write();
                stats.circular_dependencies_detected += 1;

                return Err(OwlError::ImportResolutionError {
                    iri: import_iri.clone(),
                    message: format!("Circular import detected: {}", import_iri),
                });
            }
        }

        // Add to resolving set
        {
            let mut resolving = self.resolving.write();
            resolving.insert(import_iri.clone());
        }

        // Check cache first
        if let Some(cached) = self.cache.get(import_iri) {
            log::debug!("Cache hit for import: {}", import_iri);

            // Merge cached ontology
            self.merge_ontology(target_ontology, &cached.ontology)?;

            // Update statistics
            let mut stats = self.stats.write();
            stats.cache_hits += 1;
            stats.imports_resolved += 1;
            stats.total_resolution_time += start_time.elapsed();

            // Remove from resolving set
            {
                let mut resolving = self.resolving.write();
                resolving.remove(import_iri);
            }

            return Ok(());
        }

        log::debug!("Cache miss for import: {}", import_iri);

        // Cache miss - resolve from source
        let mut stats = self.stats.write();
        stats.cache_misses += 1;
        drop(stats);

        // Find appropriate source
        let source = self
            .sources
            .iter()
            .find(|s| s.can_resolve(import_iri))
            .ok_or_else(|| OwlError::ImportResolutionError {
                iri: import_iri.clone(),
                message: format!("No import source can resolve IRI: {}", import_iri),
            })?;

        log::debug!("Resolving import {} using {}", import_iri, source.name());

        // Resolve with timeout
        let resolved_ontology = if self.config.enable_concurrent_resolution {
            // Use concurrent resolution if enabled
            self.concurrent_resolve(source.as_ref(), import_iri)?
        } else {
            // Sequential resolution
            source.resolve(import_iri, &self.config)?
        };

        // Recursively resolve imports for the imported ontology
        self.resolve_imports_with_depth(&mut resolved_ontology.clone(), depth + 1)?;

        // Cache the resolved ontology
        let cached = CachedOntology::new(
            resolved_ontology.clone(),
            import_iri.clone(),
            self.config.cache_ttl,
        );
        self.cache.put(import_iri.clone(), cached);

        // Merge the resolved ontology
        self.merge_ontology(target_ontology, &resolved_ontology)?;

        // Update statistics
        let mut stats = self.stats.write();
        stats.imports_resolved += 1;
        stats.total_resolution_time += start_time.elapsed();

        // Remove from resolving set
        {
            let mut resolving = self.resolving.write();
            resolving.remove(import_iri);
        }

        Ok(())
    }

    /// Concurrent resolution (simplified version)
    fn concurrent_resolve(&self, source: &dyn ImportSource, iri: &IRI) -> OwlResult<Ontology> {
        // For now, just resolve synchronously
        // In a full implementation, this would use async/await with proper concurrency control
        source.resolve(iri, &self.config)
    }

    /// Merge an imported ontology into the target ontology
    fn merge_ontology(&self, target: &mut Ontology, source: &Ontology) -> OwlResult<()> {
        // Merge all entities
        for class in source.classes() {
            target.add_class((**class).clone())?;
        }

        for prop in source.object_properties() {
            target.add_object_property((**prop).clone())?;
        }

        for prop in source.data_properties() {
            target.add_data_property((**prop).clone())?;
        }

        for individual in source.named_individuals() {
            target.add_named_individual((**individual).clone())?;
        }

        for individual in source.anonymous_individuals() {
            target.add_anonymous_individual((**individual).clone())?;
        }

        for prop in source.annotation_properties() {
            target.add_annotation_property((**prop).clone())?;
        }

        // Merge all axioms
        for axiom in source.axioms() {
            target.add_axiom((**axiom).clone())?;
        }

        // Merge imports
        for import_iri in source.imports() {
            target.add_import((**import_iri).clone());
        }

        // Merge annotations
        for annotation in source.annotations() {
            target.add_annotation(annotation.clone());
        }

        Ok(())
    }

    /// Get resolution statistics
    pub fn stats(&self) -> ImportResolutionStats {
        self.stats.read().clone()
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> ImportCacheStats {
        self.cache.stats()
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get the configuration
    pub fn config(&self) -> &ImportResolverConfig {
        &self.config
    }

    /// Get mutable configuration
    pub fn config_mut(&mut self) -> &mut ImportResolverConfig {
        &mut self.config
    }
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            sources: Vec::new(),
            cache: ImportCache::new(100),
            config: ImportResolverConfig::default(),
            stats: Arc::new(RwLock::new(ImportResolutionStats::default())),
            resolving: Arc::new(RwLock::new(HashSet::new())),
        })
    }
}
