#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Verify SPACL produces correct results

use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};

fn main() {
    let ontologies = vec![
        ("univ-bench", "tests/data/univ-bench.owl"),
        ("PATO", "benchmarks/ontologies/other/pato.owl"),
    ];

    for (name, path) in ontologies {
        println!("\n=== Testing {} ===", name);

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                println!("  Skipping (file not found)");
                continue;
            }
        };

        let parser = match owl2_reasoner::ParserFactory::auto_detect(&content) {
            Some(p) => p,
            None => {
                println!("  Skipping (parser not found)");
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

        println!("  Classes: {}", ontology.classes().len());

        // Sequential
        let seq = SimpleReasoner::new(ontology.clone());
        let seq_result = seq.is_consistent().unwrap();
        println!("  Sequential: {}", seq_result);

        // SPACL
        let mut config = SpeculativeConfig::default();
        config.parallel_threshold = 10; // Lower for testing
        let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
        let spacl_result = spacl.is_consistent().unwrap();
        let stats = spacl.get_stats();
        println!(
            "  SPACL: {} (branches: {})",
            spacl_result, stats.branches_created
        );

        // Verify match
        if seq_result == spacl_result {
            println!("  ✅ Results match!");
        } else {
            println!("  ❌ MISMATCH!");
        }
    }
}
