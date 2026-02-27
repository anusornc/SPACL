//! Simple Real-World Ontology Benchmark
//!
//! Run with: cargo bench --bench ontology_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use owl2_reasoner::{
    serializer::BinaryOntologyFormat, util::profiling::configure_iri_cache_for_large_ontology,
    Ontology, OntologyParser, SimpleReasoner, SpeculativeTableauxReasoner,
};

/// Benchmark ontologies
const LUBM_PATH: &str = "tests/data/univ-bench.owl";
const GO_PATH: &str = "benchmarks/ontologies/other/go-basic.owl";

fn load_ontology(path: &str) -> Option<Ontology> {
    use owl2_reasoner::parser::{OntologyParser, RdfXmlParser};

    let path = Path::new(path);
    if !path.exists() {
        println!("File not found: {}", path.display());
        return None;
    }

    // Prefer binary format if available to avoid costly parsing for large files.
    let bin_path = if path.extension().map(|e| e == "owlbin").unwrap_or(false) {
        path.to_path_buf()
    } else {
        path.with_extension("owlbin")
    };
    if bin_path.exists() {
        println!("Loading binary {}...", bin_path.display());
        let start = std::time::Instant::now();
        let mut file = std::fs::File::open(&bin_path).ok()?;
        let ontology = BinaryOntologyFormat::deserialize(&mut file).ok()?;
        println!(
            "Loaded in {:?}: {} classes",
            start.elapsed(),
            ontology.classes().len()
        );
        return Some(ontology);
    }

    // Pre-configure IRI cache based on file size to reduce allocations.
    if let Ok(metadata) = std::fs::metadata(path) {
        let file_size = metadata.len();
        let estimated_classes = (file_size / 50) as usize;
        if estimated_classes > 10_000 {
            configure_iri_cache_for_large_ontology(estimated_classes);
        }
    }

    // Try RDF/XML parser first (most .owl files are RDF/XML)
    println!("Loading {}...", path.display());
    let start = std::time::Instant::now();

    let parser = RdfXmlParser::new();
    let result = parser.parse_file(path);
    let elapsed = start.elapsed();

    match result {
        Ok(ontology) => {
            println!(
                "Loaded in {:?}: {} classes",
                elapsed,
                ontology.classes().len()
            );
            Some(ontology)
        }
        Err(e) => {
            println!("Error loading: {:?}", e);
            None
        }
    }
}

fn bench_lubm(c: &mut Criterion) {
    let Some(ontology) = load_ontology(LUBM_PATH) else {
        println!("LUBM ontology not found, skipping benchmark");
        return;
    };
    let ontology = Arc::new(ontology);

    let mut group = c.benchmark_group("lubm_consistency");
    group.measurement_time(Duration::from_secs(10));

    // Sequential benchmark
    group.bench_function("sequential", |b| {
        b.iter_batched(
            || Arc::clone(&ontology),
            |ontology| {
                let reasoner = SimpleReasoner::from_arc(ontology);
                let _ = reasoner.is_consistent();
            },
            BatchSize::SmallInput,
        );
    });

    // SPACL benchmark
    group.bench_function("spacl", |b| {
        b.iter_batched(
            || Arc::clone(&ontology),
            |ontology| {
                let mut reasoner = SpeculativeTableauxReasoner::from_arc(ontology);
                let _ = reasoner.is_consistent();
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_go(c: &mut Criterion) {
    let Some(ontology) = load_ontology(GO_PATH) else {
        println!("GO ontology not found, skipping benchmark");
        return;
    };
    let ontology = Arc::new(ontology);

    println!("Running GO benchmark (this may take a while)...");

    let mut group = c.benchmark_group("go_consistency");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(5); // Fewer samples for large ontology

    // Sequential benchmark
    group.bench_function("sequential", |b| {
        b.iter_batched(
            || Arc::clone(&ontology),
            |ontology| {
                let reasoner = SimpleReasoner::from_arc(ontology);
                let _ = reasoner.is_consistent();
            },
            BatchSize::SmallInput,
        );
    });

    // SPACL benchmark
    group.bench_function("spacl", |b| {
        b.iter_batched(
            || Arc::clone(&ontology),
            |ontology| {
                let mut reasoner = SpeculativeTableauxReasoner::from_arc(ontology);
                let _ = reasoner.is_consistent();
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_lubm, bench_go);
criterion_main!(benches);
