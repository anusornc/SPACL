#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test Hierarchical Classification on LUBM

use owl2_reasoner::{HierarchicalClassificationEngine, Ontology, ParserFactory, SimpleReasoner};
use std::path::Path;
use std::time::Instant;

fn main() {
    println!("========================================");
    println!("  Hierarchical Classification Test");
    println!("========================================\n");

    let path = Path::new("tests/data/univ-bench.owl");
    if !path.exists() {
        eprintln!("LUBM ontology not found!");
        std::process::exit(1);
    }

    println!("Loading LUBM ontology...");
    let content = std::fs::read_to_string(path).expect("Failed to read file");
    let parser = ParserFactory::auto_detect(&content).expect("Failed to detect format");
    let ontology = parser.parse_str(&content).expect("Failed to parse");

    let class_count = ontology.classes().len();
    println!("Loaded: {} classes\n", class_count);

    // Test can_handle
    let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
    println!(
        "Can use hierarchical: {}",
        if can_handle { "✓ Yes" } else { "✗ No" }
    );

    if can_handle {
        println!("\n--- Hierarchical Classification ---");
        let start = Instant::now();
        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
        let result = engine.classify().expect("Classification failed");
        let elapsed = start.elapsed();

        println!("Time: {:?}", elapsed);
        println!("Classes processed: {}", result.stats.classes_processed);
        println!(
            "Relationships discovered: {}",
            result.stats.relationships_discovered
        );

        // Test hierarchy queries
        println!("\n--- Testing Hierarchy Queries ---");

        // Get all classes
        let classes: Vec<_> = ontology.classes().iter().cloned().collect();
        if let Some(first_class) = classes.first() {
            let supers = result.hierarchy.get_all_superclasses(first_class.iri());
            println!(
                "Class {} has {} superclasses",
                first_class.iri(),
                supers.len()
            );
        }

        println!("\n✓ Hierarchical classification SUCCESS!");
    }

    // Compare with SimpleReasoner
    println!("\n--- Simple Reasoner Comparison ---");
    let start = Instant::now();
    let mut reasoner = SimpleReasoner::new(ontology.clone());
    let _ = reasoner.is_consistent();
    let elapsed = start.elapsed();

    println!("SimpleReasoner time: {:?}", elapsed);

    println!("\n========================================");
    println!("  Test Complete!");
    println!("========================================");
}
