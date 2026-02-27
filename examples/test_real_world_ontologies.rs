#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test Real-World Ontologies with Adaptive Strategy
//!
//! This tests all real-world ontologies one by one with progress reporting.

use owl2_reasoner::{
    HierarchicalClassificationEngine, Ontology, OntologyCharacteristics, ParserFactory,
    SimpleReasoner,
};
use std::path::Path;
use std::time::Instant;

fn main() {
    println!("========================================");
    println!("  Real-World Ontology Test");
    println!("  With Adaptive Strategy Selection");
    println!("========================================\n");

    let test_cases = vec![
        ("LUBM", "tests/data/univ-bench.owl"),
        ("PATO", "benchmarks/ontologies/other/pato.owl"),
        ("DOID", "benchmarks/ontologies/other/doid.owl"),
        ("UBERON", "benchmarks/ontologies/other/uberon.owl"),
        ("GO_Basic", "benchmarks/ontologies/other/go-basic.owl"),
        // Skip ChEBI for now - it's 773MB and may take very long
        // ("ChEBI", "benchmarks/ontologies/other/chebi.owl"),
    ];

    let mut results = Vec::new();

    for (name, path) in test_cases {
        println!("\n========================================");
        println!("Testing: {}", name);
        println!("========================================");

        let path = Path::new(path);
        if !path.exists() {
            println!("❌ File not found: {:?}", path);
            continue;
        }

        // Load ontology
        print!("Loading... ");
        let load_start = Instant::now();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                println!("❌ Failed to read: {}", e);
                continue;
            }
        };

        let parser = match ParserFactory::auto_detect(&content) {
            Some(p) => p,
            None => {
                println!("❌ Failed to detect format");
                continue;
            }
        };

        let ontology = match parser.parse_str(&content) {
            Ok(o) => o,
            Err(e) => {
                println!("❌ Failed to parse: {:?}", e);
                continue;
            }
        };
        let load_time = load_start.elapsed();

        let class_count = ontology.classes().len();
        let axiom_count = ontology.axioms().len();
        println!("✓ Loaded in {:?}", load_time);
        println!("  Classes: {}", class_count);
        println!("  Axioms: {}", axiom_count);

        // Analyze characteristics
        print!("Analyzing... ");
        let analysis_start = Instant::now();
        let chars = OntologyCharacteristics::analyze(&ontology);
        let analysis_time = analysis_start.elapsed();
        println!("✓ ({:?})", analysis_time);
        println!("  Complexity Score: {:.2}", chars.complexity_score);
        println!("  Disjunctions: {}", chars.disjunction_count);

        // Determine strategy
        let can_use_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);
        let strategy = if can_use_hierarchical {
            "Hierarchical"
        } else {
            "Simple"
        };
        println!("  Strategy: {}", strategy);

        // Run classification
        if can_use_hierarchical {
            println!("\nRunning Hierarchical Classification...");
            let bench_start = Instant::now();

            // Run 3 times for average
            let mut times = Vec::new();
            for i in 1..=3 {
                let run_start = Instant::now();
                let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
                match engine.classify() {
                    Ok(result) => {
                        let elapsed = run_start.elapsed();
                        times.push(elapsed);
                        if i == 1 {
                            println!("  Relationships: {}", result.stats.relationships_discovered);
                        }
                    }
                    Err(e) => {
                        println!("  ❌ Classification failed: {:?}", e);
                        break;
                    }
                }
            }

            let total_bench_time = bench_start.elapsed();

            if !times.is_empty() {
                let avg_time = times.iter().sum::<std::time::Duration>() / times.len() as u32;
                let min_time = times.iter().min().unwrap();

                println!("\n✓ Results:");
                println!("  Average: {:?}", avg_time);
                println!("  Best: {:?}", min_time);
                println!("  Total benchmark time: {:?}", total_bench_time);

                // Calculate throughput
                let throughput = class_count as f64 / min_time.as_secs_f64();
                println!("  Throughput: {:.0} classes/second", throughput);

                results.push((
                    name.to_string(),
                    class_count,
                    min_time.as_millis() as f64,
                    true,
                ));
            }
        } else {
            println!("\nRunning Simple Reasoner...");
            let run_start = Instant::now();
            let mut reasoner = SimpleReasoner::new(ontology.clone());
            let _ = reasoner.is_consistent();
            let elapsed = run_start.elapsed();

            println!("\n✓ Results:");
            println!("  Time: {:?}", elapsed);

            results.push((
                name.to_string(),
                class_count,
                elapsed.as_millis() as f64,
                false,
            ));
        }

        println!("\n✓ {} complete!", name);
    }

    // Summary
    println!("\n\n========================================");
    println!("  SUMMARY");
    println!("========================================\n");

    println!(
        "{:<15} {:>10} {:>15} {:>12}",
        "Ontology", "Classes", "Time (ms)", "Strategy"
    );
    println!("{}", "-".repeat(60));

    for (name, classes, time_ms, used_hier) in &results {
        let strategy = if *used_hier { "Hierarchical" } else { "Simple" };
        println!(
            "{:<15} {:>10} {:>15.2} {:>12}",
            name, classes, time_ms, strategy
        );
    }

    println!("\n✓ All tests complete!");
}
