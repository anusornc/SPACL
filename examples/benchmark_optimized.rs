#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Benchmark: Optimized Hierarchy vs Original Implementation
//!
//! Tests the linear-time algorithm with SCC detection and topological sort

use owl2_reasoner::{
    core::entities::Class,
    core::iri::IRI,
    core::ontology::Ontology,
    logic::axioms::{Axiom, ClassExpression, SubClassOfAxiom},
    reasoner::hierarchical_classification::HierarchicalClassificationEngine,
    reasoner::optimized_hierarchy::OptimizedClassificationEngine,
};
use std::time::Instant;

fn main() {
    println!("=== Optimized Hierarchy Benchmark ===\n");
    println!("Testing linear-time algorithm with SCC detection\n");

    let sizes = vec![100, 1000, 5000, 10000];

    for size in sizes {
        println!("\n{}", "=".repeat(60));
        println!("Testing with {} classes", size);
        println!("{}", "=".repeat(60));

        // Generate tree ontology
        let ontology = generate_tree_ontology(size);
        println!("Generated ontology: {} classes", ontology.classes().len());

        // Test Original Implementation
        println!("\n--- Original (O(n^2) BFS) ---");
        let orig_times = benchmark_original(&ontology, 3);
        let orig_avg = average_time(&orig_times);
        println!("Average time: {:?}", orig_avg);

        // Test Optimized Implementation
        println!("\n--- Optimized (Linear toposort + DP) ---");
        let opt_times = benchmark_optimized(&ontology, 3);
        let opt_avg = average_time(&opt_times);
        println!("Average time: {:?}", opt_avg);

        // Compare
        if opt_avg.as_nanos() > 0 {
            let speedup = orig_avg.as_secs_f64() / opt_avg.as_secs_f64();
            println!(
                "\n>>> Optimized is {:.1}x {}",
                if speedup > 1.0 {
                    speedup
                } else {
                    1.0 / speedup
                },
                if speedup > 1.0 {
                    "faster ✅"
                } else {
                    "slower ❌"
                }
            );
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("Benchmark Complete");
    println!("{}", "=".repeat(60));
}

fn benchmark_original(ontology: &Ontology, iterations: usize) -> Vec<std::time::Duration> {
    let mut times = Vec::new();

    for i in 0..iterations {
        let start = Instant::now();
        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
        let _ = engine.classify();
        let elapsed = start.elapsed();
        times.push(elapsed);

        if i == 0 {
            println!("  First run: {:?}", elapsed);
        }
    }

    times
}

fn benchmark_optimized(ontology: &Ontology, iterations: usize) -> Vec<std::time::Duration> {
    let mut times = Vec::new();

    for i in 0..iterations {
        let start = Instant::now();
        match OptimizedClassificationEngine::new(ontology.clone()) {
            Ok(mut engine) => match engine.classify() {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    times.push(elapsed);
                    if i == 0 {
                        println!("  First run: {:?}", elapsed);
                        println!("  Classes processed: {}", result.stats.classes_processed);
                        println!("  Relationships: {}", result.stats.relationships_discovered);
                    }
                }
                Err(e) => println!("  Classification error: {:?}", e),
            },
            Err(e) => println!("  Engine creation error: {:?}", e),
        }
    }

    times
}

fn average_time(times: &[std::time::Duration]) -> std::time::Duration {
    if times.is_empty() {
        return std::time::Duration::from_secs(0);
    }
    let sum: std::time::Duration = times.iter().sum();
    sum / times.len() as u32
}

/// Generate a tree-structured ontology
fn generate_tree_ontology(n: usize) -> Ontology {
    let mut ontology = Ontology::new();

    // Get Thing class
    let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing".to_string()).unwrap();
    let thing = Class::new(thing_iri);
    ontology.add_class(thing.clone()).unwrap();

    // Build chain: Thing -> Class_0 -> Class_1 -> ... -> Class_N-1
    let mut prev_class = thing;

    for i in 0..n {
        let class_iri = IRI::new(format!("http://test.org/Class_{}", i)).unwrap();
        let class = Class::new(class_iri);
        ontology.add_class(class.clone()).unwrap();

        // Add subclass axiom
        let axiom = SubClassOfAxiom::new(
            ClassExpression::Class(class.clone()),
            ClassExpression::Class(prev_class.clone()),
        );
        ontology
            .add_axiom(Axiom::SubClassOf(Box::new(axiom)))
            .unwrap();

        prev_class = class;
    }

    ontology
}
