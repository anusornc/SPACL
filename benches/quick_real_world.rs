//! Quick Real-World Ontology Benchmark

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::time::Duration;

use owl2_reasoner::{Ontology, ParserFactory, SimpleReasoner};

const ONTOLOGIES: &[(&str, &str)] = &[
    ("LUBM", "tests/data/univ-bench.owl"),
    ("PATO", "benchmarks/ontologies/other/pato.owl"),
    ("DOID", "benchmarks/ontologies/other/doid.owl"),
];

fn load_ontology(path: &str) -> Option<Ontology> {
    let path = Path::new(path);
    if !path.exists() {
        eprintln!("File not found: {}", path.display());
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

fn bench_real_world(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    for (name, path) in ONTOLOGIES {
        if let Some(ontology) = load_ontology(path) {
            let class_count = ontology.classes().len();
            group.bench_with_input(
                BenchmarkId::new("consistency", *name),
                &ontology,
                |b, ontology| {
                    b.iter(|| {
                        let mut reasoner = SimpleReasoner::new(ontology.clone());
                        let _ = reasoner.is_consistent();
                        black_box(reasoner);
                    });
                },
            );
            println!("Benchmarked {} ({} classes)", name, class_count);
        }
    }
    group.finish();
}

criterion_group!(benches, bench_real_world);
criterion_main!(benches);
