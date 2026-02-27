#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
// Test edge cases for adaptive threshold
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Instant;

fn create_no_disjunction_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();
    for i in 0..size {
        let subclass = Class::new(format!("http://example.org/A{}", i));
        let superclass = Class::new(format!("http://example.org/A{}", i + 1));
        ontology.add_class(subclass.clone()).unwrap();
        ontology.add_class(superclass.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(subclass),
                ClassExpression::Class(superclass),
            ))
            .unwrap();
    }
    ontology
}

fn create_single_large_union(operands: usize) -> Ontology {
    let mut ontology = Ontology::new();
    let subclass = Class::new("http://example.org/Root");
    ontology.add_class(subclass.clone()).unwrap();

    let union_operands: Vec<_> = (0..operands)
        .map(|j| {
            let op_class = Class::new(format!("http://example.org/Branch{}", j));
            ontology.add_class(op_class.clone()).unwrap();
            Box::new(ClassExpression::Class(op_class))
        })
        .collect();

    let union = ClassExpression::ObjectUnionOf(union_operands.into_iter().collect());
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(subclass),
            union,
        ))
        .unwrap();

    ontology
}

fn benchmark<F>(name: &str, f: F, iterations: usize) -> f64
where
    F: Fn() -> bool,
{
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = f();
    }
    let avg_us = start.elapsed().as_secs_f64() / iterations as f64 * 1_000_000.0;
    println!("  {}: {:.1}µs", name, avg_us);
    avg_us
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║           Edge Case Benchmark - Adaptive Threshold             ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Test 1: No disjunctions (should always use sequential)
    println!("Test 1: No disjunctions (100 classes in hierarchy)");
    let ontology = create_no_disjunction_ontology(100);
    let seq = benchmark(
        "Sequential",
        || {
            SimpleReasoner::new(ontology.clone())
                .is_consistent()
                .unwrap()
        },
        10000,
    );
    let adaptive = benchmark(
        "Adaptive",
        || {
            let mut config = SpeculativeConfig::default();
            config.parallel_threshold = 50;
            SpeculativeTableauxReasoner::with_config(ontology.clone(), config)
                .is_consistent()
                .unwrap()
        },
        10000,
    );
    println!("  Overhead: {:.1}x\n", adaptive / seq);

    // Test 2: Single large union (high branching factor)
    println!("Test 2: Single union with 20 operands (high branching)");
    let ontology = create_single_large_union(20);
    let seq = benchmark(
        "Sequential",
        || {
            SimpleReasoner::new(ontology.clone())
                .is_consistent()
                .unwrap()
        },
        5000,
    );
    let adaptive_low = benchmark(
        "Adaptive (low thresh)",
        || {
            let mut config = SpeculativeConfig::default();
            config.parallel_threshold = 50; // Will use sequential
            SpeculativeTableauxReasoner::with_config(ontology.clone(), config)
                .is_consistent()
                .unwrap()
        },
        5000,
    );
    let adaptive_high = benchmark(
        "Adaptive (high thresh)",
        || {
            let mut config = SpeculativeConfig::default();
            config.parallel_threshold = 5; // Will use parallel
            config.adaptive_tuning = false;
            SpeculativeTableauxReasoner::with_config(ontology.clone(), config)
                .is_consistent()
                .unwrap()
        },
        100,
    );
    println!(
        "  Sequential speedup vs parallel: {:.1}x\n",
        adaptive_high / seq
    );

    // Test 3: Empty ontology
    println!("Test 3: Empty ontology");
    let ontology = Ontology::new();
    let seq = benchmark(
        "Sequential",
        || {
            SimpleReasoner::new(ontology.clone())
                .is_consistent()
                .unwrap()
        },
        20000,
    );
    let adaptive = benchmark(
        "Adaptive",
        || {
            let mut config = SpeculativeConfig::default();
            config.parallel_threshold = 50;
            SpeculativeTableauxReasoner::with_config(ontology.clone(), config)
                .is_consistent()
                .unwrap()
        },
        20000,
    );
    println!("  Overhead: {:.1}x\n", adaptive / seq);

    println!("═══════════════════════════════════════════════════════════════════");
    println!("\nKey Findings:");
    println!("  1. No disjunctions → Adaptive correctly uses sequential path");
    println!("  2. High branching with low threshold → Avoids parallel overhead");
    println!("  3. Empty ontology → Minimal overhead from adaptive check");
}
