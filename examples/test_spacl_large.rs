#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test SPACL vs Sequential performance with larger ontology

use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Instant;

fn main() {
    // Load PATO ontology (larger)
    let content = std::fs::read_to_string("benchmarks/ontologies/other/pato.owl").unwrap();
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content).unwrap();
    let ontology = parser.parse_str(&content).unwrap();

    println!(
        "Loaded PATO ontology with {} classes",
        ontology.classes().len()
    );

    // Test sequential
    println!("\n=== Sequential ===");
    let start = Instant::now();
    let seq = SimpleReasoner::new(ontology.clone());
    let seq_result = seq.is_consistent().unwrap();
    let seq_time = start.elapsed();
    println!("Sequential: {} in {:?}", seq_result, seq_time);

    // Test SPACL with lower threshold
    println!("\n=== SPACL ===");
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 100; // Lower threshold
    config.num_workers = 4;

    let start = Instant::now();
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let spacl_result = spacl.is_consistent().unwrap();
    let spacl_time = start.elapsed();
    let stats = spacl.get_stats();
    println!("SPACL: {} in {:?}", spacl_result, spacl_time);
    println!("SPACL stats: {:#?}", stats);

    if spacl_time.as_millis() > 0 && seq_time.as_millis() > 0 {
        let speedup = seq_time.as_millis() as f64 / spacl_time.as_millis() as f64;
        println!("\nSpeedup: {:.2}x", speedup);
    }
}
