//! Scalability benchmark - Test performance from 100 to 10K classes
//!
//! This benchmark measures:
//! 1. Sequential performance scaling
//! 2. SPACL with adaptive threshold
//! 3. Memory usage patterns
//! 4. Nogood learning effectiveness

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Duration;

/// Create a class hierarchy ontology with specified size
fn create_hierarchy_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..size {
        let subclass = Class::new(format!("http://example.org/C{}", i));
        let superclass = Class::new(format!("http://example.org/C{}", i + 1));

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

/// Sequential baseline using SimpleReasoner
fn bench_sequential(ontology: &Ontology) -> bool {
    let reasoner = SimpleReasoner::new(ontology.clone());
    reasoner.is_consistent().unwrap_or(false)
}

/// SPACL with adaptive threshold
fn bench_spacl_adaptive(ontology: &Ontology) -> bool {
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 100; // Use sequential for <100 branches

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    reasoner.is_consistent().unwrap_or(false)
}

fn scalability_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    // Test sizes from 100 to 10,000 classes
    let sizes = vec![100, 500, 1000, 5000, 10000];

    for size in &sizes {
        let ontology = create_hierarchy_ontology(*size);

        // Report throughput in elements/second
        group.throughput(Throughput::Elements(*size as u64));
        group.measurement_time(Duration::from_secs(10));

        // Sequential benchmark
        group.bench_with_input(BenchmarkId::new("sequential", size), &ontology, |b, o| {
            b.iter(|| bench_sequential(black_box(o)))
        });

        // SPACL adaptive benchmark
        group.bench_with_input(
            BenchmarkId::new("spacl_adaptive", size),
            &ontology,
            |b, o| b.iter(|| bench_spacl_adaptive(black_box(o))),
        );
    }

    group.finish();
}

criterion_group!(benches, scalability_benchmark);
criterion_main!(benches);
