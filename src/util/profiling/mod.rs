//! Profiling and optimization utilities

pub mod memory;

pub use memory::{
    MemoryProfiler, 
    get_resident_memory, 
    configure_iri_cache_for_large_ontology,
    GLOBAL_ALLOCATOR,
};
