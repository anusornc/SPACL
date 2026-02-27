#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Verify hierarchical classification performance

use owl2_reasoner::{
    HierarchicalClassificationEngine, Ontology, OwlReasoner, ParserFactory, SimpleReasoner,
};
use std::time::Instant;

fn main() {
    println!("=== Hierarchical Classification Verification ===\n");

    // Test with LUBM first (small, fast)
    test_ontology("LUBM", "tests/data/univ-bench.owl");

    // Test with GO_Basic (large, should use hierarchical) - skip loading if takes too long
    println!("Testing GO_Basic (this may take a while to load)...");
    test_ontology("GO_Basic", "benchmarks/ontologies/other/go-basic.owl");
}

fn test_ontology(name: &str, path: &str) {
    println!("Testing {}...", name);

    // Load ontology
    let start = Instant::now();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("  Error loading: {:?}\n", e);
            return;
        }
    };

    let parser = match ParserFactory::auto_detect(&content) {
        Some(p) => p,
        None => {
            println!("  No parser found\n");
            return;
        }
    };

    let ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            println!("  Parse error: {:?}\n", e);
            return;
        }
    };
    let load_time = start.elapsed();

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    println!(
        "  Loaded: {} classes, {} axioms in {:?}",
        class_count, axiom_count, load_time
    );

    // Check if hierarchical can handle it
    let can_use_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);
    println!("  Can use hierarchical: {}", can_use_hierarchical);

    if can_use_hierarchical {
        // Time hierarchical classification
        let start = Instant::now();
        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
        let _ = engine.classify();
        let hierarchical_time = start.elapsed();

        // Time simple reasoner for comparison
        let start = Instant::now();
        let mut reasoner = SimpleReasoner::new(ontology.clone());
        let _ = reasoner.is_consistent();
        let simple_time = start.elapsed();

        let speedup = simple_time.as_secs_f64() / hierarchical_time.as_secs_f64();

        println!("  Hierarchical: {:?}", hierarchical_time);
        println!("  Simple: {:?}", simple_time);
        println!("  Speedup: {:.1}x", speedup);
    } else {
        // Just time simple reasoner
        let start = Instant::now();
        let mut reasoner = SimpleReasoner::new(ontology.clone());
        let _ = reasoner.is_consistent();
        let simple_time = start.elapsed();
        println!("  Simple reasoner: {:?}", simple_time);
    }

    println!();
}
