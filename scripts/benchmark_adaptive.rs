#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
// Quick benchmark comparing sequential, adaptive, and parallel modes
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Instant;

/// Create ontology with unions (disjunctions)
fn create_union_ontology(num_unions: usize, operands_per_union: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..num_unions {
        let subclass = Class::new(format!("http://example.org/A{}", i));
        ontology.add_class(subclass.clone()).unwrap();

        let union_operands: Vec<_> = (0..operands_per_union)
            .map(|j| {
                let op_class = Class::new(format!("http://example.org/A{}B{}", i, j));
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
    }

    ontology
}

fn benchmark_sequential(ontology: &Ontology, iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        let mut reasoner = SimpleReasoner::new(ontology.clone());
        let _ = reasoner.is_consistent();
    }
    start.elapsed().as_secs_f64() / iterations as f64 * 1_000_000.0 // Convert to µs
}

fn benchmark_adaptive(ontology: &Ontology, iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        let mut config = SpeculativeConfig::default();
        config.parallel_threshold = 100;
        let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
        let _ = reasoner.is_consistent();
    }
    start.elapsed().as_secs_f64() / iterations as f64 * 1_000_000.0 // Convert to µs
}

fn benchmark_parallel(ontology: &Ontology, iterations: usize) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        let mut config = SpeculativeConfig::default();
        config.parallel_threshold = 1;
        config.adaptive_tuning = false;
        let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
        let _ = reasoner.is_consistent();
    }
    start.elapsed().as_secs_f64() / iterations as f64 * 1_000_000.0 // Convert to µs
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Adaptive Threshold Benchmark - SPACL Performance          ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let test_cases = vec![
        (
            "Simple (1 union, 5 ops)",
            create_union_ontology(1, 5),
            10000,
        ),
        ("Small (2 unions, 5 ops)", create_union_ontology(2, 5), 5000),
        (
            "Medium (5 unions, 5 ops)",
            create_union_ontology(5, 5),
            2000,
        ),
    ];

    println!(
        "{:<25} {:>12} {:>12} {:>12} {:>10}",
        "Test Case", "Sequential", "Adaptive", "Parallel", "Speedup"
    );
    println!("{}", "─".repeat(75));

    for (name, ontology, iterations) in test_cases {
        let seq_time = benchmark_sequential(&ontology, iterations);
        let adaptive_time = benchmark_adaptive(&ontology, iterations);
        let parallel_time = benchmark_parallel(&ontology, iterations.min(100));

        let speedup = parallel_time / adaptive_time;

        println!(
            "{:<25} {:>10.1}µs {:>10.1}µs {:>10.1}µs {:>9.1}x",
            name, seq_time, adaptive_time, parallel_time, speedup
        );
    }

    println!("\n{}", "─".repeat(75));
    println!("\nKey Insights:");
    println!("  • Sequential:  SimpleReasoner baseline (no overhead)");
    println!("  • Adaptive:    SPACL with threshold=100 (avoids parallel when cheap)");
    println!("  • Parallel:    SPACL with threshold=1 (always uses parallelism)");
    println!("  • Speedup:     How much faster adaptive is vs always-parallel");
}
