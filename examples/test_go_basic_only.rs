#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test GO_Basic specifically - focused benchmark

use owl2_reasoner::{HierarchicalClassificationEngine, ParserFactory, SimpleReasoner};
use std::time::Instant;

fn main() {
    println!("=== GO_Basic Focused Benchmark ===\n");

    let path = "benchmarks/ontologies/other/go-basic.owl";

    // Step 1: Load
    println!("[1/3] Loading GO_Basic...");
    let start = Instant::now();
    let content = std::fs::read_to_string(path).expect("Failed to read file");
    let parser = ParserFactory::auto_detect(&content).expect("No parser found");
    let ontology = parser.parse_str(&content).expect("Failed to parse");
    let load_time = start.elapsed();

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    println!("  Loaded: {} classes, {} axioms", class_count, axiom_count);
    println!("  Load time: {:.2?}", load_time);

    // Step 2: Check strategy
    println!("\n[2/3] Analyzing...");
    let can_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);
    println!("  Can use hierarchical: {}", can_hierarchical);

    // Count simple axioms
    let mut simple_count = 0;
    let mut total_subclass = 0;
    for axiom in ontology.subclass_axioms() {
        total_subclass += 1;
        match (axiom.sub_class(), axiom.super_class()) {
            (
                owl2_reasoner::ClassExpression::Class(_),
                owl2_reasoner::ClassExpression::Class(_),
            ) => {
                simple_count += 1;
            }
            _ => {}
        }
    }
    let simple_ratio = if total_subclass > 0 {
        simple_count as f64 / total_subclass as f64 * 100.0
    } else {
        0.0
    };
    println!(
        "  Simple subclass axioms: {}/{} ({:.1}%)",
        simple_count, total_subclass, simple_ratio
    );

    // Step 3: Benchmark hierarchical (single run)
    if can_hierarchical {
        println!("\n[3/3] Running hierarchical classification (3 runs)...");

        let mut times = Vec::new();
        for i in 1..=3 {
            print!("  Run {}...", i);
            let _ = std::io::Write::flush(&mut std::io::stdout());

            let start = Instant::now();
            let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
            let _ = engine.classify();
            let elapsed = start.elapsed();

            println!(" {:?}", elapsed);
            times.push(elapsed);
        }

        let avg = times.iter().sum::<std::time::Duration>() / times.len() as u32;
        println!("\n  Average hierarchical: {:?}", avg);

        // Also test simple reasoner once for comparison
        println!("\n  Testing simple reasoner (1 run)...");
        let start = Instant::now();
        let mut reasoner = SimpleReasoner::new(ontology.clone());
        let _ = reasoner.is_consistent();
        let simple_time = start.elapsed();
        println!("  Simple reasoner: {:?}", simple_time);

        let speedup = simple_time.as_secs_f64() / avg.as_secs_f64();
        println!("\n========================================");
        println!("  RESULTS FOR GO_Basic:");
        println!("  - Hierarchical: {:?}", avg);
        println!("  - Simple: {:?}", simple_time);
        println!("  - Speedup: {:.1}x", speedup);
        println!("========================================");
    }
}
