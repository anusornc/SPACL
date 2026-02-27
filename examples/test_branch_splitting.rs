#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test branch splitting for disjunctions
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Instant;

fn main() {
    println!("=== Testing SPACL Branch Splitting ===\n");

    // Test with univ-bench (no disjunctions - should have 1 branch)
    let content = std::fs::read_to_string("tests/data/univ-bench.owl").unwrap();
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content).unwrap();
    let ontology = parser.parse_str(&content).unwrap();

    println!("Univ-bench: {} axioms", ontology.axioms().len());

    // Force parallel mode to see branch creation
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 5;

    let start = Instant::now();
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let result = spacl.is_consistent().unwrap();
    let time = start.elapsed();
    let stats = spacl.get_stats();

    println!("Result: {} in {:?}", result, time);
    println!("Branches created: {}", stats.branches_created);
    println!("Branches pruned: {}", stats.branches_pruned);

    // Verify with sequential
    let seq = SimpleReasoner::new(ontology);
    let seq_result = seq.is_consistent().unwrap();

    if result == seq_result {
        println!("✓ Results match!");
    } else {
        println!("✗ MISMATCH!");
    }
}
