#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Instant;

fn main() {
    let content = std::fs::read_to_string("tests/data/univ-bench.owl").unwrap();
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content).unwrap();
    let ontology = parser.parse_str(&content).unwrap();

    println!(
        "Testing threshold tuning ({} axioms)",
        ontology.axioms().len()
    );

    // Test with different thresholds
    for threshold in [5, 50, 100, 500, 1000] {
        let start = Instant::now();
        let mut config = SpeculativeConfig::default();
        config.parallel_threshold = threshold;
        let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
        let result = spacl.is_consistent().unwrap();
        let time = start.elapsed();
        let stats = spacl.get_stats();

        let mode = if stats.branches_created > 0 {
            "PARALLEL"
        } else {
            "sequential"
        };
        println!(
            "threshold={:4}: {:?} ({}) - branches={}",
            threshold, time, mode, stats.branches_created
        );
    }
}
