//! Application-specific modules
//!
//! Domain-specific implementations and examples.
//! Currently includes EPCIS (GS1 supply chain) support.

pub mod epcis;
pub mod epcis_parser;
pub mod epcis_test_generator;

// Re-exports
pub use epcis::*;
pub use epcis_test_generator::*;
