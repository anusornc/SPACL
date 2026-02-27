//! Benchmark comparing SPACL (Speculative Parallel Tableaux) vs Sequential Tableaux
//!
//! This benchmark evaluates the performance improvement of the novel SPACL algorithm
//! compared to traditional sequential tableaux reasoning.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Duration;

/// Creates a simple family ontology for testing
fn create_family_ontology() -> Ontology {
    let mut ontology = Ontology::new();

    let person = Class::new("http://example.org/Person");
    let parent = Class::new("http://example.org/Parent");
    let child = Class::new("http://example.org/Child");
    let grandparent = Class::new("http://example.org/Grandparent");

    ontology.add_class(person.clone()).unwrap();
    ontology.add_class(parent.clone()).unwrap();
    ontology.add_class(child.clone()).unwrap();
    ontology.add_class(grandparent.clone()).unwrap();

    // Parent ⊑ Person
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(parent.clone()),
            ClassExpression::Class(person.clone()),
        ))
        .unwrap();

    // Child ⊑ Person
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(child.clone()),
            ClassExpression::Class(person.clone()),
        ))
        .unwrap();

    // Grandparent ⊑ Person
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(grandparent.clone()),
            ClassExpression::Class(person.clone()),
        ))
        .unwrap();

    ontology
}

/// Creates a larger hierarchical ontology with n classes
fn create_hierarchy_ontology(n: usize) -> Ontology {
    let mut ontology = Ontology::new();
    let base = Class::new("http://example.org/Base");
    ontology.add_class(base.clone()).unwrap();

    let mut prev_class = base;
    for i in 0..n {
        let class = Class::new(format!("http://example.org/Class{}", i));
        ontology.add_class(class.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(class.clone()),
                ClassExpression::Class(prev_class.clone()),
            ))
            .unwrap();
        prev_class = class;
    }

    ontology
}

/// Creates an ontology with branching (disjunctions) to test SPACL's parallel advantage
fn create_branching_ontology(branches: usize) -> Ontology {
    let mut ontology = Ontology::new();
    let root = Class::new("http://example.org/Root");
    ontology.add_class(root.clone()).unwrap();

    for i in 0..branches {
        let branch = Class::new(format!("http://example.org/Branch{}", i));
        ontology.add_class(branch.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(branch),
                ClassExpression::Class(root.clone()),
            ))
            .unwrap();
    }

    ontology
}

fn bench_simple_reasoner(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_reasoner");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("family_ontology", |b| {
        b.iter(|| {
            let ontology = create_family_ontology();
            let reasoner = SimpleReasoner::new(ontology);
            black_box(reasoner.is_consistent().unwrap())
        })
    });

    group.finish();
}

fn bench_hierarchy_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("hierarchy_scaling");
    group.measurement_time(Duration::from_secs(20));

    for size in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("sequential", size), size, |b, &size| {
            b.iter(|| {
                let ontology = create_hierarchy_ontology(size);
                let reasoner = SimpleReasoner::new(ontology);
                black_box(reasoner.is_consistent().unwrap())
            })
        });
    }

    group.finish();
}

fn bench_speculative_vs_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("spacl_vs_sequential");
    group.measurement_time(Duration::from_secs(30));

    // Test with different branching factors
    for branches in [4, 8, 16, 32].iter() {
        group.throughput(Throughput::Elements(*branches as u64));

        // Sequential baseline
        group.bench_with_input(
            BenchmarkId::new("sequential", branches),
            branches,
            |b, &branches| {
                b.iter(|| {
                    let ontology = create_branching_ontology(branches);
                    let reasoner = SimpleReasoner::new(ontology);
                    black_box(reasoner.is_consistent().unwrap())
                })
            },
        );

        // SPACL with default config
        group.bench_with_input(
            BenchmarkId::new("spacl_default", branches),
            branches,
            |b, &branches| {
                b.iter(|| {
                    let ontology = create_branching_ontology(branches);
                    let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
                    black_box(reasoner.is_consistent().unwrap())
                })
            },
        );

        // SPACL with learning disabled (pure parallelism)
        group.bench_with_input(
            BenchmarkId::new("spacl_no_learning", branches),
            branches,
            |b, &branches| {
                b.iter(|| {
                    let ontology = create_branching_ontology(branches);
                    let config = SpeculativeConfig {
                        enable_learning: false,
                        ..Default::default()
                    };
                    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology, config);
                    black_box(reasoner.is_consistent().unwrap())
                })
            },
        );

        // SPACL with single worker (sequential but with overhead)
        group.bench_with_input(
            BenchmarkId::new("spacl_single_worker", branches),
            branches,
            |b, &branches| {
                b.iter(|| {
                    let ontology = create_branching_ontology(branches);
                    let config = SpeculativeConfig {
                        num_workers: 1,
                        ..Default::default()
                    };
                    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology, config);
                    black_box(reasoner.is_consistent().unwrap())
                })
            },
        );
    }

    group.finish();
}

fn bench_spacl_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("spacl_statistics");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("collect_stats", |b| {
        b.iter(|| {
            let ontology = create_branching_ontology(16);
            let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
            let _ = reasoner.is_consistent().unwrap();
            let stats = reasoner.get_stats();
            black_box(stats)
        })
    });

    group.bench_function("nogood_stats", |b| {
        b.iter(|| {
            let ontology = create_branching_ontology(16);
            let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
            let _ = reasoner.is_consistent().unwrap();
            let (nogoods, hits) = reasoner.get_nogood_stats();
            black_box((nogoods, hits))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_reasoner,
    bench_hierarchy_scaling,
    bench_speculative_vs_sequential,
    bench_spacl_stats
);
criterion_main!(benches);
