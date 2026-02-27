#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Benchmark: Compact Hierarchy vs Original Implementation
//!
//! This benchmark compares the memory-efficient compact hierarchy
//! representation against the original HashMap-based approach.

use owl2_reasoner::{
    core::entities::Class,
    core::iri::IRI,
    core::ontology::Ontology,
    logic::axioms::{Axiom, ClassExpression, SubClassOfAxiom},
    reasoner::compact_hierarchy::CompactClassificationEngine,
    reasoner::hierarchical_classification::HierarchicalClassificationEngine,
};
use std::time::Instant;

fn main() {
    println!("=== Compact Hierarchy Benchmark ===\n");

    let sizes = vec![100, 1000, 5000, 10000];

    for size in sizes {
        println!("\n{}", "=".repeat(60));
        println!("Testing with {} classes", size);
        println!("{}", "=".repeat(60));

        // Generate tree ontology
        let ontology = generate_tree_ontology(size);
        println!("Generated ontology: {} classes", ontology.classes().len());

        // Test Original Implementation
        println!("\n--- Original Implementation ---");
        let orig_times = benchmark_original(&ontology, 5);
        let orig_avg = average_time(&orig_times);
        println!("Average time: {:?}", orig_avg);

        // Test Compact Implementation
        println!("\n--- Compact Implementation ---");
        let compact_times = benchmark_compact(&ontology, 5);
        let compact_avg = average_time(&compact_times);
        println!("Average time: {:?}", compact_avg);

        // Compare
        if compact_avg.as_nanos() > 0 {
            let speedup = orig_avg.as_secs_f64() / compact_avg.as_secs_f64();
            println!(
                "\n>>> Compact is {:.1}x {}",
                if speedup > 1.0 {
                    speedup
                } else {
                    1.0 / speedup
                },
                if speedup > 1.0 { "faster" } else { "slower" }
            );
        }

        // Memory estimate
        let mem_per_class = size * ((size + 63) / 64) * 8 / 1024; // KB
        println!(
            "Estimated compact memory: ~{} KB for ancestors",
            mem_per_class
        );
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

fn benchmark_compact(ontology: &Ontology, iterations: usize) -> Vec<std::time::Duration> {
    let mut times = Vec::new();

    for i in 0..iterations {
        let start = Instant::now();
        match CompactClassificationEngine::new(ontology.clone()) {
            Ok(mut engine) => match engine.classify() {
                Ok(_) => {}
                Err(e) => println!("  Classification error: {:?}", e),
            },
            Err(e) => println!("  Engine creation error: {:?}", e),
        }
        let elapsed = start.elapsed();
        times.push(elapsed);

        if i == 0 {
            println!("  First run: {:?}", elapsed);
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
