//! Real-World Ontology Benchmark
//!
//! Tests SPACL on real BioPortal ontologies:
//! - GO (Gene Ontology)
//! - UBERON (Anatomy)
//! - DOID (Disease Ontology)
//! - PATO (Phenotype)
//! - ChEBI (Chemical Entities)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::parser::{OntologyParser, RdfXmlParser};
use owl2_reasoner::{Ontology, OwlReasoner, SimpleReasoner, SpeculativeTableauxReasoner};
use std::path::Path;
use std::time::{Duration, Instant};

/// Ontology test case
struct TestCase {
    name: &'static str,
    path: &'static str,
    description: &'static str,
}

const TEST_CASES: &[TestCase] = &[
    TestCase {
        name: "LUBM",
        path: "tests/data/univ-bench.owl",
        description: "Lehigh University Benchmark (~43 classes)",
    },
    TestCase {
        name: "PATO",
        path: "benchmarks/ontologies/other/pato.owl",
        description: "Phenotype And Trait Ontology (~3k classes)",
    },
    TestCase {
        name: "DOID",
        path: "benchmarks/ontologies/other/doid.owl",
        description: "Disease Ontology (~15k classes)",
    },
    TestCase {
        name: "UBERON",
        path: "benchmarks/ontologies/other/uberon.owl",
        description: "Uberon Anatomy Ontology (~15k classes)",
    },
    TestCase {
        name: "GO_Basic",
        path: "benchmarks/ontologies/other/go-basic.owl",
        description: "Gene Ontology Basic (~45k classes)",
    },
];

/// Load ontology from file
fn load_ontology(path: &str) -> Option<Ontology> {
    let path = Path::new(path);
    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;

    // Try to parse as RDF/XML
    use owl2_reasoner::parser::RdfXmlParser;
    let parser = RdfXmlParser::new();

    match parser.parse_str(&content) {
        Ok(ontology) => {
            let ont: Ontology = ontology;
            println!(
                "Loaded {}: {} classes, {} axioms",
                path.display(),
                ont.classes().len(),
                ont.axioms().len()
            );
            Some(ont)
        }
        Err(e) => {
            eprintln!("Parse error for {}: {:?}", path.display(), e);
            None
        }
    }
}

/// Benchmark sequential reasoning
fn bench_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world_sequential");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    for test in TEST_CASES {
        let Some(ontology) = load_ontology(test.path) else {
            println!("Skipping {} (not found)", test.name);
            continue;
        };

        let class_count = ontology.classes().len();

        group.bench_with_input(
            BenchmarkId::new(test.name, class_count),
            &ontology,
            |b, ont| {
                b.iter(|| {
                    let reasoner = SimpleReasoner::new(ont.clone());
                    let _ = reasoner.is_consistent();
                    black_box(&reasoner);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark SPACL reasoning
fn bench_spacl(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world_spacl");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    for test in TEST_CASES {
        let Some(ontology) = load_ontology(test.path) else {
            continue;
        };

        let class_count = ontology.classes().len();

        group.bench_with_input(
            BenchmarkId::new(test.name, class_count),
            &ontology,
            |b, ont| {
                b.iter(|| {
                    let mut reasoner = SpeculativeTableauxReasoner::new(ont.clone());
                    let _ = reasoner.is_consistent();
                    black_box(reasoner);
                });
            },
        );
    }

    group.finish();
}

/// Quick test - not a benchmark, just collects stats
#[test]
fn test_real_world_loading() {
    println!("\n=== Real-World Ontology Loading Test ===\n");

    for test in TEST_CASES {
        print!("{}: ", test.name);

        let start = Instant::now();
        match load_ontology(test.path) {
            Some(ont) => {
                let load_time = start.elapsed();
                println!(
                    "✓ Loaded {} classes, {} axioms in {:?}",
                    ont.classes().len(),
                    ont.axioms().len(),
                    load_time
                );
            }
            None => {
                println!("✗ Failed to load");
            }
        }
    }
}

criterion_group!(benches, bench_sequential, bench_spacl);
criterion_main!(benches);
