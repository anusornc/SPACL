//! Utility modules for the reasoner
//!
//! Provides:
//! - Caching infrastructure
//! - Configuration management
//! - Constants
//! - Memory management
//! - Validation utilities

pub mod cache;
pub mod cache_manager;
pub mod config;
pub mod constants;
pub mod memory;
pub mod memory_protection;
pub mod module_extraction;
pub mod ontology_io;
pub mod profiling;
pub mod utils;
pub mod validation;

// Re-exports (add as items become available)
pub use validation::academic_validation::{
    AcademicValidationReport, AcademicValidator, PerformanceMetrics, ValidationConfig,
    ValidationResult, ValidationStatistics,
};
