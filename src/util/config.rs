//! Centralized configuration for the OWL2 Reasoner
//!
//! This module provides a centralized configuration system for all components
//! of the OWL2 reasoner, including caching, reasoning, parsing, and performance settings.

use crate::util::constants::config::*;
use crate::core::error::{OwlError, OwlResult};
use std::time::Duration;

/// Main configuration for the OWL2 Reasoner
#[derive(Debug, Clone)]
pub struct OwlConfig {
    /// Cache configuration
    pub cache: CacheConfig,
    /// Reasoning configuration
    pub reasoning: ReasoningConfig,
    /// Parser configuration
    pub parser: ParserConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
}

impl Default for OwlConfig {
    fn default() -> Self {
        Self {
            cache: CacheConfig::default(),
            reasoning: ReasoningConfig::default(),
            parser: ParserConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl OwlConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder for configuration
    pub fn builder() -> OwlConfigBuilder {
        OwlConfigBuilder::new()
    }

    /// Validate the configuration
    pub fn validate(&self) -> OwlResult<()> {
        self.cache.validate()?;
        self.reasoning.validate()?;
        self.parser.validate()?;
        self.performance.validate()?;
        Ok(())
    }
}

/// Builder for OwlConfig
#[derive(Debug, Clone, Default)]
pub struct OwlConfigBuilder {
    config: OwlConfig,
}

impl OwlConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: OwlConfig::default(),
        }
    }

    /// Set cache configuration
    pub fn cache(mut self, cache: CacheConfig) -> Self {
        self.config.cache = cache;
        self
    }

    /// Set reasoning configuration
    pub fn reasoning(mut self, reasoning: ReasoningConfig) -> Self {
        self.config.reasoning = reasoning;
        self
    }

    /// Set parser configuration
    pub fn parser(mut self, parser: ParserConfig) -> Self {
        self.config.parser = parser;
        self
    }

    /// Set performance configuration
    pub fn performance(mut self, performance: PerformanceConfig) -> Self {
        self.config.performance = performance;
        self
    }

    /// Build the configuration
    pub fn build(self) -> OwlResult<OwlConfig> {
        let config = self.config;
        config.validate()?;
        Ok(config)
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum size for IRI cache
    pub iri_cache_size: usize,
    /// Maximum size for reasoning cache
    pub reasoning_cache_size: usize,
    /// Time-to-live for cache entries
    pub cache_ttl: Duration,
    /// Whether to enable cache statistics
    pub enable_stats: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            iri_cache_size: DEFAULT_CACHE_SIZE,
            reasoning_cache_size: DEFAULT_CACHE_SIZE,
            cache_ttl: Duration::from_secs(CACHE_EXPIRATION_SECONDS),
            enable_stats: true,
        }
    }
}

impl CacheConfig {
    /// Validate cache configuration
    pub fn validate(&self) -> OwlResult<()> {
        if self.iri_cache_size == 0 {
            return Err(OwlError::ConfigError {
                parameter: "iri_cache_size".to_string(),
                message: "IRI cache size must be greater than 0".to_string(),
            });
        }
        if self.reasoning_cache_size == 0 {
            return Err(OwlError::ConfigError {
                parameter: "reasoning_cache_size".to_string(),
                message: "Reasoning cache size must be greater than 0".to_string(),
            });
        }
        Ok(())
    }

    /// Create a builder for cache configuration
    pub fn builder() -> CacheConfigBuilder {
        CacheConfigBuilder::new()
    }
}

/// Builder for CacheConfig
#[derive(Debug, Clone, Default)]
pub struct CacheConfigBuilder {
    config: CacheConfig,
}

impl CacheConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: CacheConfig::default(),
        }
    }

    /// Set IRI cache size
    pub fn iri_cache_size(mut self, size: usize) -> Self {
        self.config.iri_cache_size = size;
        self
    }

    /// Set reasoning cache size
    pub fn reasoning_cache_size(mut self, size: usize) -> Self {
        self.config.reasoning_cache_size = size;
        self
    }

    /// Set cache TTL
    pub fn cache_ttl(mut self, ttl: Duration) -> Self {
        self.config.cache_ttl = ttl;
        self
    }

    /// Enable/disable statistics
    pub fn enable_stats(mut self, enable: bool) -> Self {
        self.config.enable_stats = enable;
        self
    }

    /// Build the cache configuration
    pub fn build(self) -> OwlResult<CacheConfig> {
        let config = self.config;
        config.validate()?;
        Ok(config)
    }
}

/// Reasoning configuration
#[derive(Debug, Clone)]
pub struct ReasoningConfig {
    /// Maximum reasoning depth
    pub max_depth: usize,
    /// Timeout for reasoning operations
    pub timeout: Duration,
    /// Whether to enable incremental reasoning
    pub incremental: bool,
    /// Whether to enable consistency checking
    pub consistency_checking: bool,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        Self {
            max_depth: MAX_REASONING_DEPTH,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            incremental: true,
            consistency_checking: true,
        }
    }
}

impl ReasoningConfig {
    /// Validate reasoning configuration
    pub fn validate(&self) -> OwlResult<()> {
        if self.max_depth == 0 {
            return Err(OwlError::ConfigError {
                parameter: "max_depth".to_string(),
                message: "Max depth must be greater than 0".to_string(),
            });
        }
        if self.timeout.is_zero() {
            return Err(OwlError::ConfigError {
                parameter: "timeout".to_string(),
                message: "Timeout must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}

/// Parser configuration
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Maximum file size for parsing
    pub max_file_size: usize,
    /// Buffer size for I/O operations
    pub buffer_size: usize,
    /// Whether to enable strict parsing
    pub strict_mode: bool,
    /// Whether to validate during parsing
    pub validate: bool,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_file_size: MAX_FILE_SIZE,
            buffer_size: DEFAULT_BUFFER_SIZE,
            strict_mode: false,
            validate: true,
        }
    }
}

impl ParserConfig {
    /// Validate parser configuration
    pub fn validate(&self) -> OwlResult<()> {
        if self.max_file_size == 0 {
            return Err(OwlError::ConfigError {
                parameter: "max_file_size".to_string(),
                message: "Max file size must be greater than 0".to_string(),
            });
        }
        if self.buffer_size == 0 {
            return Err(OwlError::ConfigError {
                parameter: "buffer_size".to_string(),
                message: "Buffer size must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}

/// Performance configuration
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Memory limit in bytes
    pub memory_limit: usize,
    /// Maximum concurrent operations
    pub max_concurrent_operations: usize,
    /// Whether to enable parallel processing
    pub parallel_processing: bool,
    /// Whether to enable memory profiling
    pub enable_profiling: bool,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            memory_limit: MEMORY_LIMIT_BYTES,
            max_concurrent_operations: MAX_CONCURRENT_OPERATIONS,
            parallel_processing: true,
            enable_profiling: false,
        }
    }
}

impl PerformanceConfig {
    /// Validate performance configuration
    pub fn validate(&self) -> OwlResult<()> {
        if self.memory_limit == 0 {
            return Err(OwlError::ConfigError {
                parameter: "memory_limit".to_string(),
                message: "Memory limit must be greater than 0".to_string(),
            });
        }
        if self.max_concurrent_operations == 0 {
            return Err(OwlError::ConfigError {
                parameter: "max_concurrent_operations".to_string(),
                message: "Max concurrent operations must be greater than 0".to_string(),
            });
        }
        Ok(())
    }
}

/// Predefined configuration profiles
pub mod profiles {
    use super::*;

    /// Development profile with debugging enabled
    pub fn development() -> OwlConfig {
        OwlConfig {
            cache: CacheConfig {
                enable_stats: true,
                ..Default::default()
            },
            reasoning: ReasoningConfig {
                timeout: Duration::from_secs(30), // Longer timeout for debugging
                ..Default::default()
            },
            parser: ParserConfig {
                strict_mode: false,
                validate: true,
                ..Default::default()
            },
            performance: PerformanceConfig {
                enable_profiling: true,
                ..Default::default()
            },
        }
    }

    /// Production profile optimized for performance
    pub fn production() -> OwlConfig {
        OwlConfig {
            cache: CacheConfig {
                iri_cache_size: 50_000, // Larger cache for production
                reasoning_cache_size: 10_000,
                enable_stats: false, // Disable stats in production
                ..Default::default()
            },
            reasoning: ReasoningConfig {
                timeout: Duration::from_millis(5000), // Shorter timeout
                incremental: true,
                ..Default::default()
            },
            parser: ParserConfig {
                strict_mode: true,
                validate: true,
                ..Default::default()
            },
            performance: PerformanceConfig {
                memory_limit: 2_000_000_000, // 2GB memory limit
                max_concurrent_operations: 100,
                parallel_processing: true,
                enable_profiling: false,
            },
        }
    }

    /// Testing profile with small limits
    pub fn testing() -> OwlConfig {
        OwlConfig {
            cache: CacheConfig {
                iri_cache_size: 1000,
                reasoning_cache_size: 100,
                cache_ttl: Duration::from_secs(60), // Short TTL for testing
                enable_stats: true,
            },
            reasoning: ReasoningConfig {
                max_depth: 100, // Limited depth for testing
                timeout: Duration::from_millis(1000),
                ..Default::default()
            },
            parser: ParserConfig {
                max_file_size: 1_000_000, // 1MB limit for testing
                buffer_size: 1024,
                strict_mode: true,
                validate: true,
            },
            performance: PerformanceConfig {
                memory_limit: 100_000_000, // 100MB limit
                max_concurrent_operations: 10,
                parallel_processing: false, // Disable parallel for consistent testing
                enable_profiling: false,
            },
        }
    }
}
