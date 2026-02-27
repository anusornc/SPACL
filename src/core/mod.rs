//! Core data structures for OWL2 reasoning
//!
//! This module contains the fundamental types used throughout the reasoner:
//! - Error handling
//! - IRI management
//! - OWL entities (classes, properties, individuals)
//! - Ontology container

pub mod entities;
pub mod error;
pub mod iri;
pub mod ontology;

// Re-exports for convenience
pub use entities::*;
pub use error::{OwlError, OwlResult};
pub use iri::IRI;
pub use ontology::Ontology;
