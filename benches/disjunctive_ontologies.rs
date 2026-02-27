//! Benchmark for Disjunctive Ontologies
//!
//! Tests SPACL on synthetic ontologies with disjunctions (A ⊔ B)
//! where parallel reasoning should provide benefits.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::parser::{OntologyParser, RdfXmlParser};
use owl2_reasoner::{Ontology, OwlReasoner, SimpleReasoner, SpeculativeTableauxReasoner};
use std::path::Path;
use std::time::Duration;

const TEST_CASES: &[(&str, &str, usize)] = &[
    (
        "Disjunctive_10K",
        "benchmarks/ontologies/disjunctive/disjunctive_10k.owl",
        10000,
    ),
    (
        "Disjunctive_30K",
        "benchmarks/ontologies/disjunctive/disjunctive_30k.owl",
        30000,
    ),
    (
        "Disjunctive_50K",
        "benchmarks/ontologies/disjunctive/disjunctive_50k.owl",
        50000,
    ),
];

fn load_ontology(path: &str) -> Option<Ontology> {
    let path = Path::new(path);
    if !path.exists() {
        eprintln!("File not found: {}", path.display());
        return None;
    }

    let content = std::fs::read_to_string(path).ok()?;
    let parser = RdfXmlParser::new();

    parser.parse_str(&content).ok()
}

fn bench_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("disjunctive_sequential");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(300)); // 5 min max

    for (name, path, _) in TEST_CASES {
        let Some(ont) = load_ontology(path) else {
            println!("Skipping {} (not found)", name);
            continue;
        };

        let class_count = ont.classes().len();
        let disjunctions = count_disjunctions(&ont);

        println!(
            "Testing {}: {} classes, ~{} disjunctions",
            name, class_count, disjunctions
        );

        group.bench_with_input(BenchmarkId::new(name, class_count), &ont, |b, ont| {
            b.iter(|| {
                let reasoner = SimpleReasoner::new(ont.clone());
                let _ = reasoner.is_consistent();
                black_box(&reasoner);
            });
        });
    }

    group.finish();
}

fn bench_spacl(c: &mut Criterion) {
    let mut group = c.benchmark_group("disjunctive_spacl");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(300));

    for (name, path, _) in TEST_CASES {
        let Some(ont) = load_ontology(path) else {
            continue;
        };

        let class_count = ont.classes().len();

        group.bench_with_input(BenchmarkId::new(name, class_count), &ont, |b, ont| {
            b.iter(|| {
                let mut reasoner = SpeculativeTableauxReasoner::new(ont.clone());
                let _ = reasoner.is_consistent();
                black_box(&reasoner);
            });
        });
    }

    group.finish();
}

fn count_disjunctions(ont: &Ontology) -> usize {
    // Count axioms that involve unions
    use owl2_reasoner::logic::axioms::Axiom;

    ont.axioms()
        .iter()
        .filter(|ax| {
            if let Axiom::SubClassOf(sub) = ax.as_ref() {
                format!("{:?}", sub.super_class()).contains("Union")
            } else {
                false
            }
        })
        .count()
}

criterion_group!(benches, bench_sequential, bench_spacl);
criterion_main!(benches);
