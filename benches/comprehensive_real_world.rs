//! Comprehensive real-world ontology benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::time::Duration;

fn load_ontology(path: &str) -> Option<owl2_reasoner::Ontology> {
    let content = std::fs::read_to_string(path).ok()?;
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content)?;
    parser.parse_str(&content).ok()
}

fn bench_real_world_ontologies(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world_comprehensive");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    let ontologies = vec![
        ("univ-bench", "tests/data/univ-bench.owl", 8),
        ("PATO", "benchmarks/ontologies/other/pato.owl", 13291),
        ("DOID", "benchmarks/ontologies/other/doid.owl", 15660),
        ("UBERON", "benchmarks/ontologies/other/uberon.owl", 15000),
    ];

    for (name, path, expected_classes) in ontologies {
        let Some(ontology) = load_ontology(path) else {
            println!("Skipping {} (not found)", name);
            continue;
        };

        let class_count = ontology.classes().len();
        let axiom_count = ontology.axioms().len();

        println!(
            "\n=== {} ({} classes, {} axioms) ===",
            name, class_count, axiom_count
        );

        // Determine threshold based on size
        let threshold = if axiom_count < 100 {
            1000
        } else if axiom_count < 1000 {
            500
        } else {
            100
        };

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
        group.bench_with_input(
            BenchmarkId::new(format!("spacl_t{}", threshold), name),
            &ontology,
            |b, ontology| {
                b.iter(|| {
                    let mut config = SpeculativeConfig::default();
                    config.parallel_threshold = threshold;
                    let mut reasoner =
                        SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
                    let _ = reasoner.is_consistent();
                    black_box(reasoner);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_real_world_ontologies);
criterion_main!(benches);
