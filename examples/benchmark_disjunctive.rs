#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Benchmark SPACL on disjunctive ontology
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Instant;

fn main() {
    println!("========================================");
    println!("SPACL Disjunctive Ontology Benchmark");
    println!("========================================\n");

    let mut ontology = Ontology::new();
    let top = Class::new("http://example.org/Top");
    let a = Class::new("http://example.org/A");
    let b = Class::new("http://example.org/B");
    let c = Class::new("http://example.org/C");
    let d = Class::new("http://example.org/D");
    let e = Class::new("http://example.org/E");
    ontology.add_class(top.clone()).unwrap();
    ontology.add_class(a.clone()).unwrap();
    ontology.add_class(b.clone()).unwrap();
    ontology.add_class(c.clone()).unwrap();
    ontology.add_class(d.clone()).unwrap();
    ontology.add_class(e.clone()).unwrap();

    let first_union = ClassExpression::ObjectUnionOf(
        vec![
            Box::new(ClassExpression::Class(a.clone())),
            Box::new(ClassExpression::Class(b.clone())),
        ]
        .into_iter()
        .collect(),
    );
    let second_union = ClassExpression::ObjectUnionOf(
        vec![
            Box::new(ClassExpression::Class(c.clone())),
            Box::new(ClassExpression::Class(d.clone())),
        ]
        .into_iter()
        .collect(),
    );
    let conjunction = ClassExpression::ObjectIntersectionOf(
        vec![
            Box::new(first_union),
            Box::new(second_union),
            Box::new(ClassExpression::Class(e.clone())),
        ]
        .into_iter()
        .collect(),
    );
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(top),
            conjunction,
        ))
        .unwrap();

    for i in 0..50 {
        let leaf = Class::new(format!("http://example.org/E{}", i));
        ontology.add_class(leaf.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(leaf),
                ClassExpression::Class(e.clone()),
            ))
            .unwrap();
    }

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    println!("Ontology: synthetic disjunctive");
    println!("Classes: {}", class_count);
    println!("Axioms: {}", axiom_count);
    println!("Structure: (A ⊔ B) ⊓ (C ⊔ D) ⊓ E + 50 subclasses of E");
    println!();

    // Sequential benchmark
    println!("--- Sequential ---");
    let start = Instant::now();
    let seq = SimpleReasoner::new(ontology.clone());
    let seq_result = seq.is_consistent().unwrap();
    let seq_time = start.elapsed();
    println!("Result: {} in {:?}", seq_result, seq_time);

    // SPACL benchmark - force parallel mode
    println!("\n--- SPACL (parallel mode) ---");
    let start = Instant::now();
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 1; // Force parallel
    config.adaptive_tuning = false; // Ensure parallel_threshold gate is used
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let spacl_result = spacl.is_consistent().unwrap();
    let spacl_time = start.elapsed();
    let stats = spacl.get_stats();

    println!("Result: {} in {:?}", spacl_result, spacl_time);
    println!("Branches created: {}", stats.branches_created);
    println!("Branches pruned: {}", stats.branches_pruned);
    println!("Nogoods learned: {}", stats.nogoods_learned);

    // Verify correctness
    if seq_result == spacl_result {
        println!("\n✓ Results match!");
    } else {
        println!("\n✗ MISMATCH!");
    }

    // Calculate performance
    let seq_us = seq_time.as_micros() as f64;
    let spacl_us = spacl_time.as_micros() as f64;

    if seq_us > 0.0 && spacl_us > 0.0 {
        let speedup = seq_us / spacl_us;
        if speedup > 1.0 {
            println!("\n🚀 Speedup: {:.2}x", speedup);
        } else {
            println!("\n⚠️  Overhead: {:.2}x", 1.0 / speedup);
        }
    }

    println!("\n========================================");
}
