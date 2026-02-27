#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Synthetic Hierarchy Benchmark
//!
//! Generates large tree-like ontologies to test hierarchical classification performance

use owl2_reasoner::{
    core::entities::Class,
    core::iri::IRI,
    core::ontology::Ontology,
    logic::axioms::{Axiom, ClassExpression, SubClassOfAxiom},
    reasoner::hierarchical_classification::HierarchicalClassificationEngine,
    reasoner::simple::SimpleReasoner,
};
use std::time::Instant;

fn main() {
    println!("=== Synthetic Hierarchy Benchmark ===\n");

    // Test with different sizes
    let sizes = vec![100, 1000, 5000, 10000, 50000];

    for size in sizes {
        println!("\n{}", "=".repeat(60));
        println!("Testing with {} classes", size);
        println!("{}", "=".repeat(60));

        // Generate tree ontology
        let ontology = generate_tree_ontology(size);

        let class_count = ontology.classes().len();
        let axiom_count = ontology.axioms().len();
        println!(
            "  Generated: {} classes, {} axioms",
            class_count, axiom_count
        );

        // Verify hierarchical can handle it
        let can_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);
        println!("  Can use hierarchical: {}", can_hierarchical);

        if !can_hierarchical {
            println!("  ERROR: Should be able to handle tree ontology!");
            continue;
        }

        // Benchmark hierarchical
        let hierarchical_iters = if size >= 10000 { 3 } else { 10 };
        let mut hier_times = Vec::new();

        println!(
            "  Running hierarchical ({} iterations)...",
            hierarchical_iters
        );
        for i in 0..hierarchical_iters {
            let start = Instant::now();
            let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
            let _ = engine.classify();
            let elapsed = start.elapsed();
            hier_times.push(elapsed);

            if i == 0 {
                println!("    First run: {:?}", elapsed);
            }
        }

        let avg_hier = hier_times.iter().sum::<std::time::Duration>() / hier_times.len() as u32;
        println!("    Average: {:?}", avg_hier);

        // Benchmark simple (only for smaller sizes)
        if size <= 5000 {
            let simple_iters = if size >= 1000 { 3 } else { 5 };
            let mut simple_times = Vec::new();

            println!("  Running simple reasoner ({} iterations)...", simple_iters);
            for i in 0..simple_iters {
                let start = Instant::now();
                let mut reasoner = SimpleReasoner::new(ontology.clone());
                let _ = reasoner.is_consistent();
                let elapsed = start.elapsed();
                simple_times.push(elapsed);

                if i == 0 {
                    println!("    First run: {:?}", elapsed);
                }
            }

            let avg_simple =
                simple_times.iter().sum::<std::time::Duration>() / simple_times.len() as u32;
            println!("    Average: {:?}", avg_simple);

            let speedup = avg_simple.as_secs_f64() / avg_hier.as_secs_f64();
            println!(
                "\n  RESULT: {:.1}x speedup (Hierarchical: {:?} vs Simple: {:?})",
                speedup, avg_hier, avg_simple
            );
        } else {
            // For large sizes, estimate based on O(n) vs O(n²) complexity
            println!(
                "  Skipping simple reasoner (would be too slow for {} classes)",
                size
            );
            println!("\n  RESULT: Hierarchical: {:?}", avg_hier);
        }

        // Show scaling
        if size > 100 {
            let time_per_class = avg_hier.as_secs_f64() * 1_000_000.0 / size as f64;
            println!("  Time per class: {:.2} µs", time_per_class);
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("BENCHMARK COMPLETE");
    println!("{}", "=".repeat(60));
}

/// Generate a tree-structured ontology with N classes
/// Structure: Thing > Class_0 > Class_1 > ... > Class_N-1
fn generate_tree_ontology(n: usize) -> Ontology {
    let mut ontology = Ontology::new();

    // Get Thing class
    let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing".to_string()).unwrap();
    let thing = Class::new(thing_iri);

    // Add Thing to ontology
    ontology.add_class(thing.clone()).unwrap();

    // Generate N classes with linear hierarchy
    // Thing > Class_0 > Class_1 > Class_2 > ... > Class_N-1
    let mut prev_class = thing.clone();

    for i in 0..n {
        let class_iri = IRI::new(format!("http://test.org/Class_{}", i)).unwrap();
        let class = Class::new(class_iri);

        // Add class to ontology
        ontology.add_class(class.clone()).unwrap();

        // Add subclass axiom: Class_i ⊑ prev_class
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
