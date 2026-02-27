//! # Tableaux Reasoning Engine
//!
//! A modular, high-performance tableaux-based reasoning engine for OWL2 ontologies.
//! This implementation follows the SROIQ(D) description logic and provides
//! optimized reasoning through a clean, modular architecture.
//!
//! ## Architecture Overview
//!
//! The tableaux engine is organized into several specialized modules:
//!
//! - **[`core`]** - Core data structures and reasoning engine
//! - **[`graph`]** - Graph management and edge storage
//! - **[`memory`]** - Arena allocation and memory management
//! - **[`blocking`]** - Blocking strategies and constraint management
//! - **[`dependency`]** - Dependency-directed backtracking
//! - **[`expansion`]** - Rule expansion and application logic
//!
//! ## Key Features
//!
//! - **Memory Efficiency**: Arena-based allocation with automatic cleanup
//! - **Performance Optimized**: Hash-based indexing and smallvec optimizations
//! - **Backtracking Support**: Dependency-directed backtracking with choice points
//! - **Configurable Blocking**: Multiple blocking strategies (Equality, Subset, Optimized)
//! - **Comprehensive Caching**: Multi-layered caching for consistency and classification
//! - **Performance Monitoring**: Detailed statistics and memory profiling
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use owl2_reasoner::{Ontology, Class, SubClassOfAxiom, ClassExpression};
//! use owl2_reasoner::reasoning::tableaux::TableauxReasoner;
//!
//! // Create ontology with classes
//! let mut ontology = Ontology::new();
//! let person = Class::new("http://example.org/Person");
//! let parent = Class::new("http://example.org/Parent");
//! ontology.add_class(person.clone())?;
//! ontology.add_class(parent.clone())?;
//!
//! // Add subclass relationship
//! let subclass_axiom = SubClassOfAxiom::new(
//!     ClassExpression::Class(parent.clone()),
//!     ClassExpression::Class(person.clone()),
//! );
//! ontology.add_subclass_axiom(subclass_axiom)?;
//!
//! // Create tableaux reasoner and perform reasoning
//! let reasoner = TableauxReasoner::new(ontology);
//! let is_consistent = reasoner.is_consistent()?;
//! let is_subclass = reasoner.is_subclass_of(&parent.iri(), &person.iri())?;
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Time Complexity**: O(n log n) for classification, O(n) for consistency checking
//! - **Space Complexity**: O(n) with arena-based memory management
//! - **Memory Usage**: Optimized through string interning and structural sharing
//! - **Caching**: Configurable TTL-based caching with LRU eviction

pub mod blocking;
pub mod core;
pub mod dependency;
pub mod equality;
pub mod expansion;
pub mod graph;
pub mod memory;
pub mod parallel;

// Reasoning result types
#[derive(Debug, Clone)]
pub struct ReasoningResult {
    pub is_consistent: bool,
    pub has_clash: bool,
    pub reasoning_time_ms: u64,
    pub nodes_expanded: usize,
    pub rules_applied: usize,
}

impl Default for ReasoningResult {
    fn default() -> Self {
        Self {
            is_consistent: true,
            has_clash: false,
            reasoning_time_ms: 0,
            nodes_expanded: 0,
            rules_applied: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReasoningStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_rules: usize,
    pub memory_usage_bytes: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

// Re-export the main reasoners and types for backwards compatibility
pub use core::{
    MemoryStats, NodeId, ReasoningConfig, ReasoningRules, TableauxNode, TableauxReasoner,
};
pub use parallel::{ParallelReasoningCache, ParallelTableauxReasoner, WorkerConfig};

// Re-export other essential types
pub use blocking::{BlockingConstraint, BlockingManager, BlockingStats, BlockingStrategy};
pub use dependency::{ChoicePoint, Dependency, DependencyManager};
pub use expansion::{ExpansionEngine, ExpansionRules};
pub use graph::{EdgeStorage, TableauxGraph};
pub use memory::{
    ArenaEdgeStorage, ArenaManager, ArenaStats, ArenaTableauxGraph, LockFreeArenaNode,
    LockFreeMemoryManager, LockFreeMemoryStats, MemoryManager, MemoryOptimizationStats,
};
