//! Enhanced JSON-LD 1.1 Parser for OWL2 Ontologies
//!
//! This module provides comprehensive JSON-LD 1.1 standard compliance including:
//! - Full context processing with remote fetching
//! - Value expansion with typing and language tagging
//! - Container processing (@language, @index, @set, @list)
//! - JSON-LD expansion algorithm implementation
//! - @reverse processing and nested contexts
//! - Integration with OWL2 ontology structures

pub mod algorithm;
pub mod container;
pub mod context;
pub mod parser;
pub mod value;

// Re-export the main parser
pub use parser::JsonLdParser;

// Re-export other types for backward compatibility
pub use algorithm::{
    ExpandedNode, ExpandedValue, ExpansionConfig, JsonLdExpansionAlgorithm, Owl2Node, Owl2Value,
};
pub use container::{ContainerProcessor, ProcessedContainer, RdfObject, RdfTriple};
pub use context::{Container, Context, ContextManager, TermDefinition};
pub use value::ProcessedValue as JsonLdValue;
