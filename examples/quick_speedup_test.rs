#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
use owl2_reasoner::{
    Class, ClassExpression, Ontology, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use std::time::Instant;

fn main() {
    // Build a synthetic disjunctive ontology directly to ensure ObjectUnionOf is present.
    let mut ontology = Ontology::new();
    let top = Class::new("http://example.org/Top");
    let a = Class::new("http://example.org/A");
    let b = Class::new("http://example.org/B");
    ontology.add_class(top.clone()).unwrap();
    ontology.add_class(a.clone()).unwrap();
    ontology.add_class(b.clone()).unwrap();

    let union = ClassExpression::ObjectUnionOf(
        vec![
            Box::new(ClassExpression::Class(a.clone())),
            Box::new(ClassExpression::Class(b.clone())),
        ]
        .into_iter()
        .collect(),
    );
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(top.clone()),
            union,
        ))
        .unwrap();

    // Add extra hierarchy so benchmark is not completely trivial.
    for i in 0..100 {
        let leaf = Class::new(format!("http://example.org/B{}", i));
        ontology.add_class(leaf.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(leaf),
                ClassExpression::Class(b.clone()),
            ))
            .unwrap();
    }

    println!(
        "Synthetic disjunctive: {} classes, {} axioms",
        ontology.classes().len(),
        ontology.axioms().len()
    );

    // Sequential
    let start = Instant::now();
    let seq = SimpleReasoner::new(ontology.clone());
    let seq_result = seq.is_consistent().unwrap();
    let seq_time = start.elapsed();
    println!("Sequential: {} in {:?}", seq_result, seq_time);

    // SPACL with low threshold to force parallel
    let start = Instant::now();
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = 1; // Force parallel with a single detected disjunction
    config.adaptive_tuning = false; // Ensure parallel_threshold gate is used
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let spacl_result = spacl.is_consistent().unwrap();
    let spacl_time = start.elapsed();
    let stats = spacl.get_stats();

    println!("SPACL: {} in {:?}", spacl_result, spacl_time);
    println!("  Branches created: {}", stats.branches_created);
    println!("  Branches pruned: {}", stats.branches_pruned);
    println!("  Nogoods learned: {}", stats.nogoods_learned);

    let speedup = seq_time.as_micros() as f64 / spacl_time.as_micros() as f64;
    println!("  Speedup: {:.2}x", speedup);
}
