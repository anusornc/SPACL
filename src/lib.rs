//! # OWL2 Reasoner (Tableauxx)
//!
//! A high-performance, feature-complete OWL2 reasoning engine with novel
//! speculative parallel reasoning (SPACL algorithm).
//!
//! ## Features
//!
//! - **SPACL Algorithm**: Novel Speculative Parallel Tableaux with Adaptive Conflict Learning
//! - **Complete OWL2 DL support**: Full SROIQ(D) description logic
//! - **Multi-format parsing**: Turtle, RDF/XML, OWL/XML, N-Triples, JSON-LD
//! - **Profile optimization**: Automatic selection of EL/QL/RL optimizations
//! - **Meta-reasoning**: ML-based strategy selection
//! - **Evolutionary optimization**: Self-tuning parameters
//!
//! ## Project Structure
//!
//! ```text
//! src/
//! ├── core/          # Core types: IRI, entities, ontology, errors
//! ├── logic/         # Axioms, class expressions, datatypes
//! ├── parser/        # Input parsers (Turtle, RDF/XML, etc.)
//! ├── reasoner/      # Reasoning engines
//! │   ├── tableaux/  # Traditional tableaux
//! │   ├── speculative.rs  # SPACL (our novel algorithm)
//! │   └── simple.rs  # Cached simple reasoner
//! ├── strategy/      # Strategy selection & optimization
//! │   ├── meta_reasoner.rs
//! │   ├── evolutionary.rs
//! │   └── profiles/  # EL/QL/RL validation
//! ├── util/          # Utilities: cache, config, validation
//! └── app/           # Application-specific (EPCIS)
//! ```
//!
//! ## Quick Start
//!
//! ```rust
//! use owl2_reasoner::{
//!     Ontology, Class, SimpleReasoner, SubClassOfAxiom, ClassExpression
//! };
//!
//! let mut ontology = Ontology::new();
//! let person = Class::new("http://example.org/Person");
//! let parent = Class::new("http://example.org/Parent");
//!
//! ontology.add_class(person.clone())?;
//! ontology.add_class(parent.clone())?;
//!
//! let axiom = SubClassOfAxiom::new(
//!     ClassExpression::Class(parent.clone()),
//!     ClassExpression::Class(person.clone()),
//! );
//! ontology.add_subclass_axiom(axiom)?;
//!
//! let reasoner = SimpleReasoner::new(ontology);
//! assert!(reasoner.is_consistent()?);
//! assert!(reasoner.is_subclass_of(&parent.iri(), &person.iri())?);
//! # Ok::<(), owl2_reasoner::OwlError>(())
//! ```

#![allow(ambiguous_glob_reexports)]

// Core modules
pub mod core;
pub mod logic;
pub mod parser;
pub mod reasoner;
pub mod serializer;
pub mod strategy;
pub mod util;

// Application modules
pub mod app;

// Storage (for future backends)
pub mod storage;

// Backwards-compatible module aliases for doctests/examples
pub mod entities {
    pub use crate::core::entities::*;
}

pub mod axioms {
    pub use crate::logic::axioms::*;
}

pub mod reasoning {
    pub mod tableaux {
        pub use crate::reasoner::tableaux::*;
    }

    pub mod batch_operations {
        pub use crate::reasoner::batch_operations::*;
    }
}

// Re-exports for convenience
pub use core::{
    entities::*,
    error::{OwlError, OwlResult},
    iri::IRI,
    ontology::Ontology,
};

pub use logic::axioms::*;

pub use parser::{
    ImportResolver, ImportResolverConfig, JsonLdParser, ManchesterParser, OntologyParser,
    OwlFunctionalSyntaxParser, ParserFactory,
};

pub use reasoner::{
    batch_operations::*, classification::*, consistency::*,
    hierarchical_classification::HierarchicalClassificationEngine, profile_optimized::*,
    simple::SimpleReasoner, speculative::*, tableaux::*, ComplexityLevel, ExpressivenessLevel,
    OntologyFeatures, OwlReasoner, Reasoner, ReasoningResult, ReasoningStats, ReasoningTask,
};

pub use strategy::{
    evolutionary::{EvolutionaryOptimizer, EvolutionaryStrategy, PopulationStats},
    meta_reasoner::{MetaReasoner, ReasoningStrategy as MetaReasoningStrategy},
    ontology_analysis::{OntologyCharacteristics, ReasoningStrategy},
    profiles::{
        CachePriority, CacheStats, Owl2Profile, Owl2ProfileValidator, ProfileAnalysisReport,
        ProfileCacheConfig, ProfileValidationCache, ProfileValidationResult, ProfileValidator,
        ProfileViolation, ValidationStatistics,
    },
    reasoner_router::{
        detect_profile, select_classification_reasoner, select_consistency_reasoner,
        ClassificationReasoner, ClassificationRoutingDecision, ConsistencyReasoner,
        ConsistencyRoutingDecision, RoutingSource,
    },
};

pub use util::{
    cache::*, cache_manager::*, config::*, constants::*, memory::*, memory_protection::*, utils::*,
    validation::academic_validation::*,
};

pub use app::{epcis::*, epcis_test_generator::*};

// Web service feature
#[cfg(feature = "web-service")]
pub mod web_service;
