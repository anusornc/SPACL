//! Quick benchmark comparing sequential vs SPACL
//! This version runs faster with fewer samples

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use owl2_reasoner::{Class, ClassExpression, Ontology, SimpleReasoner, SubClassOfAxiom};
use std::time::Duration;

/// Create a simple class hierarchy ontology
fn create_hierarchy_ontology(size: usize) -> Ontology {
    let mut ontology = Ontology::new();

    // Create classes: A0 ⊑ A1 ⊑ A2 ⊑ ... ⊑ An
    for i in 0..size {
        let subclass = Class::new(format!("http://example.org/A{}", i));
        let superclass = Class::new(format!("http://example.org/A{}", i + 1));

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

/// Sequential consistency check
fn sequential_consistency(ontology: &Ontology) -> bool {
    let reasoner = SimpleReasoner::new(ontology.clone());
    reasoner.is_consistent().unwrap_or(false)
}

fn bench_quick_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("quick_comparison");
    group.sample_size(50); // Fewer samples for faster runs
    group.warm_up_time(Duration::from_secs(1));

    // Test different sizes
    for size in [10, 50, 100] {
        let ontology = create_hierarchy_ontology(size);

        group.bench_with_input(BenchmarkId::new("sequential", size), &ontology, |b, o| {
            b.iter(|| sequential_consistency(black_box(o)))
        });
    }

    group.finish();
}

fn bench_simple_reasoner(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_reasoner_quick");
    group.sample_size(50);

    // Create a small family-like ontology
    let mut ontology = Ontology::new();

    let classes = ["Person", "Parent", "Child", "Grandparent"];
    for i in 0..classes.len() - 1 {
        let subclass = Class::new(format!("http://example.org/family#{}", classes[i]));
        let superclass = Class::new(format!("http://example.org/family#{}", classes[i + 1]));

        ontology.add_class(subclass.clone()).unwrap();
        ontology.add_class(superclass.clone()).unwrap();

        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(subclass),
                ClassExpression::Class(superclass),
            ))
            .unwrap();
    }

    group.bench_function("family_ontology", |b| {
        b.iter(|| sequential_consistency(black_box(&ontology)))
    });

    group.finish();
}

criterion_group!(benches, bench_quick_comparison, bench_simple_reasoner);
criterion_main!(benches);
