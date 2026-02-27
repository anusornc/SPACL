//! Benchmark for adaptive parallelism threshold
//!
//! This benchmark verifies that:
//! 1. Small ontologies use sequential processing (fast)
//! 2. Large ontologies use parallel processing when beneficial

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::sync::Arc;

/// Create a simple class hierarchy ontology
fn create_hierarchy_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();

    // Create classes: A0 ⊑ A1 ⊑ A2 ⊑ ... ⊑ An
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

/// Sequential reasoner baseline
fn sequential_check(ontology: &Ontology) -> bool {
    let reasoner = SimpleReasoner::new(ontology.clone());
    reasoner.is_consistent().unwrap_or(false)
}

/// SPACL with low threshold (always parallel)
fn spacl_always_parallel(ontology: &Ontology) -> bool {
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 1; // Always use parallel

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    reasoner.is_consistent().unwrap_or(false)
}

/// SPACL with adaptive threshold (sequential for small)
fn spacl_adaptive(ontology: &Ontology) -> bool {
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 100; // Sequential if <100 branches

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    reasoner.is_consistent().unwrap_or(false)
}

fn bench_adaptive_threshold(c: &mut Criterion) {
    let mut group = c.benchmark_group("adaptive_threshold");
    group.sample_size(30);
    group.warm_up_time(std::time::Duration::from_secs(1));

    // Test different sizes
    for size in [10, 50, 100] {
        let ontology = create_hierarchy_ontology(size);

        // Sequential baseline
        group.bench_with_input(BenchmarkId::new("sequential", size), &ontology, |b, o| {
            b.iter(|| sequential_check(black_box(o)))
        });

        // SPACL always parallel (high overhead for small)
        group.bench_with_input(
            BenchmarkId::new("spacl_always_parallel", size),
            &ontology,
            |b, o| b.iter(|| spacl_always_parallel(black_box(o))),
        );

        // SPACL adaptive (should use sequential for small)
        group.bench_with_input(
            BenchmarkId::new("spacl_adaptive", size),
            &ontology,
            |b, o| b.iter(|| spacl_adaptive(black_box(o))),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_adaptive_threshold);
criterion_main!(benches);
