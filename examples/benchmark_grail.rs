#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Benchmark: GRAIL Index vs Original Implementation
//!
//! Tests the O(n) space approach with randomized interval labeling

use owl2_reasoner::{
    core::entities::Class,
    core::iri::IRI,
    core::ontology::Ontology,
    logic::axioms::{Axiom, ClassExpression, SubClassOfAxiom},
    reasoner::grail_hierarchy::GrailClassificationEngine,
    reasoner::hierarchical_classification::HierarchicalClassificationEngine,
};
use std::time::Instant;

fn main() {
    println!("=== GRAIL Hierarchy Benchmark ===\n");
    println!("Testing O(n) space approach with randomized interval labeling\n");

    let sizes = vec![100, 1000, 5000, 10000, 50000];

    for size in sizes {
        println!("\n{}", "=".repeat(70));
        println!("Testing with {} classes", size);
        println!("{}", "=".repeat(70));

        // Generate tree ontology
        let ontology = generate_tree_ontology(size);
        println!("Generated ontology: {} classes", ontology.classes().len());

        // Test Original Implementation
        println!("\n--- Original (O(n^2) materialized) ---");
        let orig_times = benchmark_original(&ontology, 3);
        let orig_avg = average_time(&orig_times);
        println!("Average build time: {:?}", orig_avg);

        // Test GRAIL Implementation
        println!("\n--- GRAIL (O(n) space, O(1) query) ---");
        match benchmark_grail(&ontology, 3) {
            Ok((build_times, query_times)) => {
                let build_avg = average_time(&build_times);
                let query_avg = average_time(&query_times);
                println!("Average build time: {:?}", build_avg);
                println!("Average query time: {:?}", query_avg);

                // Compare
                if build_avg.as_nanos() > 0 {
                    let speedup = orig_avg.as_secs_f64() / build_avg.as_secs_f64();
                    println!(
                        "\n>>> GRAIL build is {:.1}x {}",
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
            Err(e) => println!("GRAIL error: {:?}", e),
        }
    }

    println!("\n{}", "=".repeat(70));
    println!("Benchmark Complete");
    println!("{}", "=".repeat(70));
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

fn benchmark_grail(
    ontology: &Ontology,
    iterations: usize,
) -> Result<(Vec<std::time::Duration>, Vec<std::time::Duration>), Box<dyn std::error::Error>> {
    let mut build_times = Vec::new();
    let mut query_times = Vec::new();

    for i in 0..iterations {
        let start = Instant::now();
        let mut engine = GrailClassificationEngine::new(ontology.clone())?;
        let result = engine.classify()?;
        let build_elapsed = start.elapsed();
        build_times.push(build_elapsed);

        // Test some queries
        let query_start = Instant::now();
        let classes: Vec<_> = ontology.classes().iter().cloned().collect();

        // Query 100 random subclass relationships
        for j in 0..100.min(classes.len()) {
            let sub = classes[j].iri();
            let sup = classes[0].iri(); // Thing
            let _ = engine.is_subclass_of(sub, sup);
        }
        let query_elapsed = query_start.elapsed();
        query_times.push(query_elapsed);

        if i == 0 {
            println!("  First build: {:?}", build_elapsed);
            println!(
                "  Classes: {}, Edges: {}",
                result.stats.classes_processed, result.stats.relationships_discovered
            );
            println!("  Query time (100 checks): {:?}", query_elapsed);
        }
    }

    Ok((build_times, query_times))
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
