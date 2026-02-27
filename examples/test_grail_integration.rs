#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test GRAIL integration with real ontologies

use owl2_reasoner::{HierarchicalClassificationEngine, Ontology, OwlReasoner, ParserFactory};
use std::time::Instant;

fn main() {
    println!("=== GRAIL Integration Test ===\n");

    // Test ontologies
    let ontologies = vec![
        ("LUBM", "tests/data/univ-bench.owl"),
        (
            "Hierarchy 100",
            "benchmarks/competitors/ontologies/hierarchy_100.owl",
        ),
        (
            "Hierarchy 1000",
            "benchmarks/competitors/ontologies/hierarchy_1000.owl",
        ),
        (
            "Hierarchy 10000",
            "benchmarks/competitors/ontologies/hierarchy_10000.owl",
        ),
    ];

    for (name, path) in ontologies {
        println!("\n{}", "=".repeat(60));
        println!("Testing: {}", name);
        println!("{}", "=".repeat(60));

        // Load ontology
        let start = Instant::now();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                println!("  Error loading: {:?}", e);
                continue;
            }
        };

        let parser = match ParserFactory::auto_detect(&content) {
            Some(p) => p,
            None => {
                println!("  No parser found");
                continue;
            }
        };

        let ontology = match parser.parse_str(&content) {
            Ok(o) => o,
            Err(e) => {
                println!("  Parse error: {:?}", e);
                continue;
            }
        };
        let load_time = start.elapsed();

        let class_count = ontology.classes().len();
        println!("  Loaded: {} classes in {:?}", class_count, load_time);

        // Check if hierarchical can handle it
        let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
        println!("  Can use hierarchical: {}", can_handle);

        if !can_handle {
            println!("  Skipping (not hierarchical)");
            continue;
        }

        // Classify with GRAIL
        println!("  Classifying with GRAIL...");
        let start = Instant::now();
        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
        let result = match engine.classify() {
            Ok(r) => r,
            Err(e) => {
                println!("  Classification error: {:?}", e);
                continue;
            }
        };
        let classify_time = start.elapsed();

        println!("  ✓ Classified in {:?}", classify_time);
        println!("  Classes: {}", result.stats.classes_processed);
        println!("  Relationships: {}", result.stats.relationships_discovered);

        // Test queries
        if class_count > 0 {
            let start = Instant::now();
            let classes: Vec<_> = ontology.classes().iter().cloned().collect();
            let thing_iri = owl2_reasoner::core::iri::IRI::new(
                "http://www.w3.org/2002/07/owl#Thing".to_string(),
            )
            .unwrap();

            // Query 100 subclass relationships
            let query_count = 100.min(classes.len());
            for i in 0..query_count {
                let _ = engine.is_subclass_of(classes[i].iri(), &thing_iri);
            }
            let query_time = start.elapsed();

            println!("  Query time ({} checks): {:?}", query_count, query_time);
            println!("  Avg per query: {:?}", query_time / query_count as u32);
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("Integration Test Complete");
    println!("{}", "=".repeat(60));
}
