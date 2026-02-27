//! Extreme-scale benchmark - Test up to 100K classes

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Duration;

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

fn bench_sequential(ontology: &Ontology) -> bool {
    let reasoner = SimpleReasoner::new(ontology.clone());
    reasoner.is_consistent().unwrap_or(false)
}

fn bench_spacl(ontology: &Ontology) -> bool {
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 100;

    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let result = reasoner.is_consistent().unwrap_or(false);
    let stats = reasoner.get_stats();
    println!(
        "\nStats: {} branches, {} pruned",
        stats.branches_created, stats.branches_pruned
    );
    result
}

fn extreme_scale_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("extreme_scale");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(10);

    // Test sizes from 10K to 100K
    for size in [10000, 50000, 100000] {
        println!("\nGenerating {}-class ontology...", size);
        let ontology = create_hierarchy_ontology(size);
        println!("Done! Running benchmarks...");

        // Sequential
        group.bench_with_input(BenchmarkId::new("sequential", size), &ontology, |b, o| {
            b.iter(|| bench_sequential(black_box(o)))
        });

        // SPACL
        group.bench_with_input(BenchmarkId::new("spacl", size), &ontology, |b, o| {
            b.iter(|| bench_spacl(black_box(o)))
        });
    }

    group.finish();
}

criterion_group!(benches, extreme_scale_benchmark);
criterion_main!(benches);
