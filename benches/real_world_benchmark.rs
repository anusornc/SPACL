//! Real-World Ontology Benchmark
//!
//! This benchmark evaluates performance on real-world biomedical ontologies
//! with adaptive strategy selection (hierarchical/simple/SPACL).

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use owl2_reasoner::reasoner::grail_hierarchy::GrailClassificationEngine;
use owl2_reasoner::{
    detect_profile, select_classification_reasoner, util::module_extraction::extract_tbox_module,
    util::ontology_io::load_ontology_with_env, ClassificationReasoner,
    HierarchicalClassificationEngine, Ontology, SimpleReasoner, SpeculativeTableauxReasoner,
};

/// Configuration for benchmark runs
const SAMPLE_SIZE: usize = 10;
const MEASUREMENT_TIME_SECS: u64 = 30;
const TBOX_MODULE_MIN_CLASSES: usize = 50_000;

/// Ontology benchmark configuration
struct OntologyBenchmark {
    name: &'static str,
    path: &'static str,
    _description: &'static str,
}

/// List of ontologies to benchmark
const ONTOLOGIES: &[OntologyBenchmark] = &[
    OntologyBenchmark {
        name: "LUBM",
        path: "tests/data/univ-bench.owl",
        _description: "Lehigh University Benchmark",
    },
    OntologyBenchmark {
        name: "PATO",
        path: "benchmarks/ontologies/other/pato.owl",
        _description: "Phenotype And Trait Ontology",
    },
    OntologyBenchmark {
        name: "DOID",
        path: "benchmarks/ontologies/other/doid.owl",
        _description: "Disease Ontology",
    },
    OntologyBenchmark {
        name: "UBERON",
        path: "benchmarks/ontologies/other/uberon.owl",
        _description: "Uberon Anatomy Ontology",
    },
    OntologyBenchmark {
        name: "GO_Basic",
        path: "benchmarks/ontologies/other/go-basic.owl",
        _description: "Gene Ontology (basic)",
    },
    OntologyBenchmark {
        name: "ChEBI",
        path: "benchmarks/ontologies/other/chebi.owl",
        _description: "Chemical Entities of Biological Interest",
    },
];

/// Load an ontology from file
fn load_ontology(path: &str) -> Option<Ontology> {
    let path = Path::new(path);

    if !path.exists() {
        eprintln!("Warning: Ontology file not found: {}", path.display());
        return None;
    }

    println!("Loading {}...", path.display());
    let start = std::time::Instant::now();
    let ontology = load_ontology_with_env(path).ok()?;
    println!(
        "Loaded in {:?}: {} classes",
        start.elapsed(),
        ontology.classes().len()
    );
    Some(ontology)
}

fn has_abox_axioms(ontology: &Ontology) -> bool {
    !ontology.class_assertions().is_empty()
        || !ontology.property_assertions().is_empty()
        || !ontology.data_property_assertions().is_empty()
        || !ontology.negative_object_property_assertions().is_empty()
        || !ontology.negative_data_property_assertions().is_empty()
        || !ontology.same_individual_axioms().is_empty()
        || !ontology.different_individuals_axioms().is_empty()
        || !ontology.named_individuals().is_empty()
        || !ontology.anonymous_individuals().is_empty()
}

fn maybe_extract_tbox_module(ontology: Ontology) -> Ontology {
    if ontology.classes().len() < TBOX_MODULE_MIN_CLASSES {
        return ontology;
    }

    if has_abox_axioms(&ontology) {
        return ontology;
    }

    let axiom_count = ontology.axioms().len();
    match extract_tbox_module(&ontology) {
        Ok(module) => {
            println!(
                "TBox module extracted: {} axioms -> {} axioms",
                axiom_count,
                module.axioms().len()
            );
            module
        }
        Err(err) => {
            eprintln!("TBox module extraction failed: {:?}", err);
            ontology
        }
    }
}

fn should_skip_ontology(ont: &OntologyBenchmark) -> bool {
    if ont.name == "ChEBI" && std::env::var("OWL2_REASONER_INCLUDE_CHEBI").is_err() {
        println!("Skipping ChEBI (set OWL2_REASONER_INCLUDE_CHEBI=1 to include)");
        return true;
    }
    false
}

/// Benchmark adaptive strategy selection
fn bench_adaptive_classification(c: &mut Criterion) {
    let mut group = c.benchmark_group("adaptive_classification");
    group.sample_size(SAMPLE_SIZE);
    group.measurement_time(Duration::from_secs(MEASUREMENT_TIME_SECS));

    for ont_config in ONTOLOGIES {
        if should_skip_ontology(ont_config) {
            continue;
        }
        let Some(ontology) = load_ontology(ont_config.path) else {
            println!("Skipping {} (file not found)", ont_config.name);
            continue;
        };
        let ontology = maybe_extract_tbox_module(ontology);
        let ontology = Arc::new(ontology);

        let profile = detect_profile(&ontology);
        if let Some(profile) = profile {
            println!("{}: Profile {:?}", ont_config.name, profile);
        }

        let decision = select_classification_reasoner(&ontology, profile);
        println!(
            "{}: Using {} ({})",
            ont_config.name,
            decision.reasoner.as_str(),
            decision.rationale
        );

        match decision.reasoner {
            ClassificationReasoner::Hierarchical => {
                let class_count = ontology.classes().len();
                if class_count > 10_000 {
                    group.bench_with_input(
                        BenchmarkId::new("hierarchical_grail", ont_config.name),
                        &ontology,
                        |b, ontology| {
                            b.iter_batched(
                                || Arc::clone(ontology),
                                |ontology| {
                                    let mut engine = GrailClassificationEngine::from_arc(ontology)
                                        .expect("Failed to create GRAIL engine");
                                    let _ = engine.classify();
                                    black_box(engine);
                                },
                                BatchSize::SmallInput,
                            );
                        },
                    );
                } else {
                    group.bench_with_input(
                        BenchmarkId::new("hierarchical", ont_config.name),
                        &ontology,
                        |b, ontology| {
                            b.iter_batched(
                                || Arc::clone(ontology),
                                |ontology| {
                                    let mut engine =
                                        HierarchicalClassificationEngine::from_arc(ontology);
                                    let _ = engine.classify();
                                    black_box(engine);
                                },
                                BatchSize::SmallInput,
                            );
                        },
                    );
                }
            }
            ClassificationReasoner::Simple => {
                group.bench_with_input(
                    BenchmarkId::new("simple", ont_config.name),
                    &ontology,
                    |b, ontology| {
                        b.iter_batched(
                            || Arc::clone(ontology),
                            |ontology| {
                                let reasoner = SimpleReasoner::from_arc(ontology);
                                let _ = reasoner.is_consistent();
                                black_box(reasoner);
                            },
                            BatchSize::SmallInput,
                        );
                    },
                );
            }
            ClassificationReasoner::Speculative => {
                group.bench_with_input(
                    BenchmarkId::new("spacl", ont_config.name),
                    &ontology,
                    |b, ontology| {
                        b.iter_batched(
                            || Arc::clone(ontology),
                            |ontology| {
                                let mut reasoner = SpeculativeTableauxReasoner::from_arc(ontology);
                                let _ = reasoner.is_consistent();
                                black_box(reasoner);
                            },
                            BatchSize::SmallInput,
                        );
                    },
                );
            }
        }
    }

    group.finish();
}

/// Sequential baseline using SimpleReasoner
fn bench_sequential_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_baseline");
    group.sample_size(SAMPLE_SIZE);
    group.measurement_time(Duration::from_secs(MEASUREMENT_TIME_SECS));

    for ont_config in ONTOLOGIES {
        if should_skip_ontology(ont_config) {
            continue;
        }
        let Some(ontology) = load_ontology(ont_config.path) else {
            continue;
        };
        let ontology = maybe_extract_tbox_module(ontology);
        let ontology = Arc::new(ontology);

        group.bench_with_input(
            BenchmarkId::new("sequential", ont_config.name),
            &ontology,
            |b, ontology| {
                b.iter_batched(
                    || Arc::clone(ontology),
                    |ontology| {
                        let reasoner = SimpleReasoner::from_arc(ontology);
                        let _ = reasoner.is_consistent();
                        black_box(reasoner);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_adaptive_classification,
    bench_sequential_baseline
);
criterion_main!(benches);
