//! Benchmark to measure nogood learning effectiveness
//!
//! This benchmark tests:
//! 1. Nogood hit rates at different scales
//! 2. Pruning effectiveness (% of branches pruned)
//! 3. Local vs global cache hit distribution
//! 4. Performance with/without nogood learning

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{
    Class, ClassExpression, DisjointClassesAxiom, Ontology, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Duration;

/// Create an ontology with disjunctions to test nogood learning
fn create_disjunctive_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();

    // Create a hierarchy
    for i in 0..size {
        let class = Class::new(format!("http://example.org/C{}", i));
        ontology.add_class(class).unwrap();
    }

    // Add disjointness axioms (creates opportunities for nogood learning)
    // Make every 5th class disjoint from the next one
    for i in (0..size).step_by(5) {
        if i + 1 < size {
            let c1 = Class::new(format!("http://example.org/C{}", i));
            let c2 = Class::new(format!("http://example.org/C{}", i + 1));

            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    ClassExpression::Class(c1),
                    ClassExpression::Class(c2),
                ]))
                .unwrap();
        }
    }

    ontology
}

/// Run reasoning with nogood learning enabled
fn reason_with_learning(ontology: &Ontology) -> (bool, String) {
    let mut config = SpeculativeConfig::default();
    config.enable_learning = true;
    config.parallel_threshold = 50; // Use parallel for this test

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);

    let result = reasoner.is_consistent().unwrap_or(false);
    let stats = reasoner.get_stats();
    let report = stats.report();

    (result, report)
}

/// Run reasoning with nogood learning disabled
fn reason_without_learning(ontology: &Ontology) -> bool {
    let mut config = SpeculativeConfig::default();
    config.enable_learning = false;
    config.parallel_threshold = 50;

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);

    reasoner.is_consistent().unwrap_or(false)
}

fn bench_nogood_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("nogood_effectiveness");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    // Test with different sizes
    for size in [100, 500, 1000] {
        let ontology = create_disjunctive_ontology(size);

        // Benchmark with learning
        group.bench_with_input(
            BenchmarkId::new("with_learning", size),
            &ontology,
            |b, o| {
                b.iter(|| {
                    let (result, report) = reason_with_learning(black_box(o));
                    // Print stats every iteration for analysis
                    println!("\nSize {}: {}", size, report);
                    result
                })
            },
        );

        // Benchmark without learning
        group.bench_with_input(
            BenchmarkId::new("without_learning", size),
            &ontology,
            |b, o| b.iter(|| reason_without_learning(black_box(o))),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_nogood_effectiveness);
criterion_main!(benches);
