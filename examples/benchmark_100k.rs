#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Quick benchmark with working ontologies
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Instant;

fn main() {
    // Use DOID (15K classes) - smaller but works
    let content = std::fs::read_to_string("benchmarks/ontologies/other/doid.owl").unwrap();
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content).unwrap();
    let ontology = parser.parse_str(&content).unwrap();

    println!("=== DOID ({} classes) ===", ontology.classes().len());

    // Sequential
    let start = Instant::now();
    let seq = SimpleReasoner::new(ontology.clone());
    let seq_result = seq.is_consistent().unwrap();
    let seq_time = start.elapsed();
    println!("Sequential: {} in {:?}", seq_result, seq_time);

    // SPACL - sequential mode (small ontology)
    let start = Instant::now();
    let mut spacl = SpeculativeTableauxReasoner::new(ontology.clone());
    let spacl_result = spacl.is_consistent().unwrap();
    let spacl_time = start.elapsed();
    println!("SPACL (seq mode): {} in {:?}", spacl_result, spacl_time);

    // SPACL - force parallel mode
    let start = Instant::now();
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 10; // Force parallel
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let spacl_result = spacl.is_consistent().unwrap();
    let spacl_time = start.elapsed();
    let stats = spacl.get_stats();
    println!("SPACL (parallel): {} in {:?}", spacl_result, spacl_time);
    println!("  Branches: {}", stats.branches_created);
    println!(
        "  Speedup: {:.2}x",
        if spacl_time.as_millis() > 0 {
            seq_time.as_millis() as f64 / spacl_time.as_millis() as f64
        } else {
            0.0
        }
    );
}
