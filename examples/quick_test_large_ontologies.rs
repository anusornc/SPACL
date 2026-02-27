#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Quick Test for Large Ontologies with GRAIL

use owl2_reasoner::{HierarchicalClassificationEngine, ParserFactory};
use std::time::Instant;

fn test_ontology(name: &str, path: &str) {
    println!("\n=== {} ===", name);

    if !std::path::Path::new(path).exists() {
        println!("  File not found");
        return;
    }

    // Load
    println!("  Loading...");
    let start = Instant::now();
    let content = std::fs::read_to_string(path).expect("Failed to read");
    let parser = ParserFactory::auto_detect(&content).expect("No parser");
    let ontology = parser.parse_str(&content).expect("Parse failed");
    let load_time = start.elapsed();

    let class_count = ontology.classes().len();
    println!("  Loaded {} classes in {:?}", class_count, load_time);

    // Check hierarchical
    let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
    println!("  Can handle: {}", can_handle);

    if !can_handle {
        return;
    }

    // Classify with GRAIL
    println!("  Classifying...");
    let start = Instant::now();
    let mut engine = HierarchicalClassificationEngine::new(ontology);
    let result = engine.classify().expect("Classification failed");
    let classify_time = start.elapsed();

    println!("  ✓ Done in {:?}", classify_time);
    println!("  Relationships: {}", result.stats.relationships_discovered);

    let throughput = class_count as f64 / classify_time.as_secs_f64();
    println!("  Throughput: {:.0} classes/sec", throughput);
}

fn main() {
    println!("========================================");
    println!("Quick Large Ontology Test with GRAIL");
    println!("========================================");

    // Test PATO and DOID (fast)
    test_ontology("PATO", "benchmarks/ontologies/other/pato.owl");
    test_ontology("DOID", "benchmarks/ontologies/other/doid.owl");

    // Test UBERON and GO_Basic (slow loading but fast classification)
    println!("\n⚠️  Large ontologies ahead - loading may take 2-5 minutes each");
    test_ontology("UBERON", "benchmarks/ontologies/other/uberon.owl");
    test_ontology("GO_Basic", "benchmarks/ontologies/other/go-basic.owl");

    println!("\n========================================");
    println!("Test Complete");
    println!("========================================");
}
