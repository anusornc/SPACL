//! OWL2 Profile validation
//!
//! Validates ontologies against OWL2 profiles:
//! - EL (EL++): Polynomial-time reasoning
//! - QL: Efficient query answering
//! - RL: Rule-based reasoning

pub mod cache;
pub mod common;
pub mod el;
pub mod ql;
pub mod rl;

// Re-exports
pub use cache::{CachePriority, ProfileCacheConfig, ProfileValidationCache};
pub use common::*;
