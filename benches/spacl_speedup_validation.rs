//! Validate SPACL speedup claims with large ontologies

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Duration;

fn load_ontology(path: &str) -> Option<owl2_reasoner::Ontology> {
    let content = std::fs::read_to_string(path).ok()?;
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content)?;
    parser.parse_str(&content).ok()
}

fn bench_speedup(c: &mut Criterion) {
    let mut group = c.benchmark_group("spacl_speedup");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    let test_cases = vec![
        ("1K", "tests/data/hierarchy_1000.owl"),
        ("10K", "tests/data/hierarchy_10000.owl"),
        ("100K", "tests/data/hierarchy_100000.owl"),
    ];

    for (name, path) in test_cases {
        let Some(ontology) = load_ontology(path) else {
            println!("Skipping {} (not found)", name);
            continue;
        };

        let class_count = ontology.classes().len();
        println!("\n=== Testing {} ({} classes) ===", name, class_count);

        // Sequential benchmark
        group.bench_with_input(
            BenchmarkId::new("sequential", name),
            &ontology,
            |b, ontology| {
                b.iter(|| {
                    let reasoner = SimpleReasoner::new(ontology.clone());
                    let _ = reasoner.is_consistent();
                    black_box(reasoner);
                });
            },
        );

        // SPACL benchmark
        group.bench_with_input(BenchmarkId::new("spacl", name), &ontology, |b, ontology| {
            b.iter(|| {
                let mut config = SpeculativeConfig::default();
                config.parallel_threshold = 100;
                let mut reasoner =
                    SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
                let _ = reasoner.is_consistent();
                black_box(reasoner);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_speedup);
criterion_main!(benches);
