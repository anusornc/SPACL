//! Reasoning engines for OWL2 ontologies
//!
//! This module provides multiple reasoning implementations:
//! - `tableaux`: Traditional tableaux-based reasoning (complete for SROIQ)
//! - `speculative`: SPACL - Speculative Parallel Tableaux with Adaptive Conflict Learning
//! - `simple`: Simplified reasoner with caching
//! - `meta`: Meta-reasoner for strategy selection

pub mod batch_operations;
pub mod classification;
pub mod compact_hierarchy;
pub mod consistency;
pub mod grail_hierarchy;
pub mod hierarchical_classification;
pub mod optimized_hierarchy;
pub mod profile_optimized;
pub mod rules;
pub mod simple;
pub mod speculative;
pub mod tableaux;

// Re-export query module if it exists
pub mod query;

// Core reasoning types
use crate::core::OwlResult;
use crate::core::IRI;

/// Trait for OWL reasoners
pub trait OwlReasoner {
    /// Check if the ontology is consistent
    fn is_consistent(&self) -> OwlResult<bool>;

    /// Check if a class is satisfiable
    fn is_satisfiable(&self, class_iri: &IRI) -> OwlResult<bool>;

    /// Check if one class is a subclass of another
    fn is_subclass_of(&self, sub: &IRI, sup: &IRI) -> OwlResult<bool>;

    /// Classify the ontology
    fn classify(&mut self) -> OwlResult<()>;
}

/// Trait for reasoners
pub trait Reasoner {
    /// Initialize the reasoner
    fn initialize(&mut self) -> OwlResult<()>;

    /// Perform reasoning
    fn reason(&mut self) -> OwlResult<ReasoningResult>;

    /// Get reasoning statistics
    fn get_stats(&self) -> ReasoningStats;
}

/// Result of a reasoning operation
#[derive(Debug, Clone, Default)]
pub struct ReasoningResult {
    pub is_consistent: bool,
    pub has_clash: bool,
    pub reasoning_time_ms: u64,
    pub nodes_expanded: usize,
    pub rules_applied: usize,
}

/// Statistics about reasoning
#[derive(Debug, Clone, Default)]
pub struct ReasoningStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_rules: usize,
    pub memory_usage_bytes: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

/// Features of an ontology for strategy selection
#[derive(Debug, Clone)]
pub struct OntologyFeatures {
    pub num_classes: usize,
    pub num_properties: usize,
    pub num_individuals: usize,
    pub expressiveness_level: ExpressivenessLevel,
    pub has_nominals: bool,
    pub has_cardinality_restrictions: bool,
    pub estimated_complexity: ComplexityLevel,
}

impl OntologyFeatures {
    pub fn new(num_classes: usize, num_properties: usize, num_individuals: usize) -> Self {
        Self {
            num_classes,
            num_properties,
            num_individuals,
            expressiveness_level: ExpressivenessLevel::ALC,
            has_nominals: false,
            has_cardinality_restrictions: false,
            estimated_complexity: ComplexityLevel::Medium,
        }
    }
}

/// Expressiveness level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressivenessLevel {
    EL,
    DLLite,
    ALC,
    ALCQ,
    ALCQI,
    SHIQ,
    SROIQ,
}

impl ExpressivenessLevel {
    pub fn rank(&self) -> u8 {
        match self {
            ExpressivenessLevel::EL => 0,
            ExpressivenessLevel::DLLite => 1,
            ExpressivenessLevel::ALC => 2,
            ExpressivenessLevel::ALCQ => 3,
            ExpressivenessLevel::ALCQI => 4,
            ExpressivenessLevel::SHIQ => 5,
            ExpressivenessLevel::SROIQ => 6,
        }
    }
}

/// Complexity level for reasoning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComplexityLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// Type of reasoning task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReasoningTask {
    ConsistencyCheck,
    Classification,
    SatisfiabilityCheck,
    InstanceRetrieval,
    QueryAnswering,
}
