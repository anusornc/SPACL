//! Logic and axiom definitions for OWL2
//!
//! This module contains:
//! - Axiom types (subclass, equivalence, disjointness, etc.)
//! - Class expressions (intersection, union, restriction, etc.)
//! - Property expressions
//! - Datatype definitions

pub mod axioms;
pub mod datatypes;

// Re-exports
pub use axioms::*;
