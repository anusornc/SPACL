//! Strategy selection and optimization for reasoning
//!
//! This module provides:
//! - Meta-reasoner for selecting the best reasoning strategy
//! - Evolutionary optimization for tuning parameters
//! - Profile validation (EL, QL, RL)

pub mod evolutionary;
pub mod meta_reasoner;
pub mod ontology_analysis;
pub mod profiles;
pub mod reasoner_router;

// Re-exports
pub use evolutionary::{EvolutionaryOptimizer, EvolutionaryStrategy, PopulationStats};
pub use meta_reasoner::{MetaReasoner, ReasoningStrategy as MetaReasoningStrategy};
pub use ontology_analysis::{OntologyCharacteristics, ReasoningStrategy};
pub use profiles::{
    CachePriority, CacheStats, Owl2Profile, Owl2ProfileValidator, ProfileAnalysisReport,
    ProfileCacheConfig, ProfileValidationCache, ProfileValidationResult, ProfileValidator,
    ProfileViolation, ValidationStatistics,
};
pub use reasoner_router::{
    detect_profile, select_classification_reasoner, select_consistency_reasoner,
    ClassificationReasoner, ClassificationRoutingDecision, ConsistencyReasoner,
    ConsistencyRoutingDecision, RoutingSource,
};
