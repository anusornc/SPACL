//! Benchmark showing adaptive threshold with unions
//!
//! This benchmark creates ontologies with actual disjunctions (unions)
//! to demonstrate when adaptive parallelism helps.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};

/// Create ontology with disjunctions: A ⊑ B ⊔ C ⊔ D ⊔ ...
fn create_union_ontology(num_unions: usize, operands_per_union: usize) -> Ontology {
    let mut ontology = Ontology::new();

    for i in 0..num_unions {
        let subclass = Class::new(format!("http://example.org/A{}", i));
        ontology.add_class(subclass.clone()).unwrap();

        // Create union: B1 ⊔ B2 ⊔ B3 ⊔ ...
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

fn bench_adaptive_with_unions(c: &mut Criterion) {
    let mut group = c.benchmark_group("adaptive_with_unions");
    group.sample_size(20);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(3));

    // Test: few unions with many operands (high branching factor)
    for &num_unions in &[1, 2, 4] {
        let ontology = create_union_ontology(num_unions, 10);

        // Sequential baseline
        group.bench_with_input(
            BenchmarkId::new("sequential", num_unions),
            &ontology,
            |b, o| {
                b.iter(|| {
                    let mut r = SimpleReasoner::new(o.clone());
                    r.is_consistent().unwrap_or(false)
                })
            },
        );

        // SPACL adaptive - should decide based on cost
        group.bench_with_input(
            BenchmarkId::new("spacl_adaptive", num_unions),
            &ontology,
            |b, o| {
                b.iter(|| {
                    let mut config = SpeculativeConfig::default();
                    config.parallel_threshold = 100; // High threshold = prefer sequential
                    let mut r = SpeculativeTableauxReasoner::with_config(o.clone(), config);
                    r.is_consistent().unwrap_or(false)
                })
            },
        );

        // SPACL forced parallel - always use parallelism
        group.bench_with_input(
            BenchmarkId::new("spacl_parallel", num_unions),
            &ontology,
            |b, o| {
                b.iter(|| {
                    let mut config = SpeculativeConfig::default();
                    config.parallel_threshold = 1; // Low threshold = always parallel
                    config.adaptive_tuning = false;
                    let mut r = SpeculativeTableauxReasoner::with_config(o.clone(), config);
                    r.is_consistent().unwrap_or(false)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_adaptive_with_unions);
criterion_main!(benches);
