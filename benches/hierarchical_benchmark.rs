//! Hierarchical Classification Benchmark
//!
//! This benchmark demonstrates the performance improvement from using
//! hierarchical classification for tree-like ontologies.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::time::Duration;

use owl2_reasoner::{
    HierarchicalClassificationEngine, Ontology, OwlReasoner, ParserFactory, SimpleReasoner,
    SpeculativeTableauxReasoner,
};

/// Configuration for benchmark runs
const SAMPLE_SIZE: usize = 10;
const MEASUREMENT_TIME_SECS: u64 = 30;

/// Ontology benchmark configuration
struct OntologyBenchmark {
    name: &'static str,
    path: &'static str,
    description: &'static str,
    expected_classes: usize,
}

/// List of ontologies to benchmark
const ONTOLOGIES: &[OntologyBenchmark] = &[
    OntologyBenchmark {
        name: "LUBM",
        path: "tests/data/univ-bench.owl",
        description: "Lehigh University Benchmark (43 classes)",
        expected_classes: 43,
    },
    OntologyBenchmark {
        name: "GO_Basic",
        path: "benchmarks/ontologies/other/go-basic.owl",
        description: "Gene Ontology basic (51k+ classes)",
        expected_classes: 51897,
    },
];

/// Load an ontology from file
fn load_ontology(path: &str) -> Option<Ontology> {
    let path = Path::new(path);

    if !path.exists() {
        eprintln!("Warning: Ontology file not found: {}", path.display());
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;
    let parser = ParserFactory::auto_detect(&content)?;

    match parser.parse_str(&content) {
        Ok(ontology) => {
            println!(
                "Loaded {}: {} classes",
                path.display(),
                ontology.classes().len()
            );
            Some(ontology)
        }
        Err(e) => {
            eprintln!("Error parsing {}: {:?}", path.display(), e);
            None
        }
    }
}

/// Benchmark with automatic strategy selection
fn bench_adaptive_classification(c: &mut Criterion) {
    let mut group = c.benchmark_group("adaptive_classification");
    group.sample_size(SAMPLE_SIZE);
    group.measurement_time(Duration::from_secs(MEASUREMENT_TIME_SECS));

    for ont_config in ONTOLOGIES {
        let Some(ontology) = load_ontology(ont_config.path) else {
            println!("Skipping {} (file not found)", ont_config.name);
            continue;
        };

        let class_count = ontology.classes().len();

        // Check if we can use hierarchical classification
        let can_use_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);

        if can_use_hierarchical {
            println!(
                "{}: Using HIERARCHICAL classification (fast path)",
                ont_config.name
            );

            group.bench_with_input(
                BenchmarkId::new("hierarchical", ont_config.name),
                &ontology,
                |b, ontology| {
                    b.iter(|| {
                        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
                        let _ = engine.classify();
                        black_box(engine);
                    });
                },
            );
        } else {
            println!("{}: Using SIMPLE reasoner (fallback)", ont_config.name);

            group.bench_with_input(
                BenchmarkId::new("simple", ont_config.name),
                &ontology,
                |b, ontology| {
                    b.iter(|| {
                        let mut reasoner = SimpleReasoner::new(ontology.clone());
                        let _ = reasoner.is_consistent();
                        black_box(reasoner);
                    });
                },
            );
        }

        println!("Benchmarked {} ({} classes)", ont_config.name, class_count);
    }

    group.finish();
}

/// Compare hierarchical vs simple for LUBM (small test)
fn bench_hierarchical_vs_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("hierarchical_vs_simple");
    group.sample_size(SAMPLE_SIZE);
    group.measurement_time(Duration::from_secs(MEASUREMENT_TIME_SECS));

    // Only test with LUBM for comparison
    let ont_config = &ONTOLOGIES[0]; // LUBM

    let Some(ontology) = load_ontology(ont_config.path) else {
        println!("Skipping {} (file not found)", ont_config.name);
        return;
    };

    // Benchmark hierarchical classification
    group.bench_with_input(
        BenchmarkId::new("hierarchical", ont_config.name),
        &ontology,
        |b, ontology| {
            b.iter(|| {
                let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
                let _ = engine.classify();
                black_box(engine);
            });
        },
    );

    // Benchmark simple reasoner
    group.bench_with_input(
        BenchmarkId::new("simple", ont_config.name),
        &ontology,
        |b, ontology| {
            b.iter(|| {
                let mut reasoner = SimpleReasoner::new(ontology.clone());
                let _ = reasoner.is_consistent();
                black_box(reasoner);
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_adaptive_classification,
    bench_hierarchical_vs_simple
);
criterion_main!(benches);
