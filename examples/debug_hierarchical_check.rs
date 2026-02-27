#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Debug why ontologies fail hierarchical check

use owl2_reasoner::{HierarchicalClassificationEngine, ParserFactory};
use std::path::Path;

fn main() {
    println!("========================================");
    println!("  Debug Hierarchical Check");
    println!("========================================\n");

    let test_cases = vec![
        ("LUBM", "tests/data/univ-bench.owl"),
        ("PATO", "benchmarks/ontologies/other/pato.owl"),
        ("DOID", "benchmarks/ontologies/other/doid.owl"),
    ];

    for (name, path) in test_cases {
        println!("\n========================================");
        println!("Analyzing: {}", name);
        println!("========================================");

        let path = Path::new(path);
        if !path.exists() {
            println!("❌ File not found");
            continue;
        }

        // Load ontology
        let content = std::fs::read_to_string(path).expect("Failed to read");
        let parser = ParserFactory::auto_detect(&content).expect("Failed to detect");
        let ontology = parser.parse_str(&content).expect("Failed to parse");

        println!("Classes: {}", ontology.classes().len());
        println!("Total axioms: {}", ontology.axioms().len());

        // Check each condition
        println!("\n--- Checking conditions for HierarchicalEngine ---");

        // 1. Check subclass axioms
        let subclass_axioms = ontology.subclass_axioms().len();
        println!("Subclass axioms: {}", subclass_axioms);

        let mut simple_subclass = 0;
        let mut complex_subclass = 0;
        for axiom in ontology.subclass_axioms() {
            use owl2_reasoner::logic::axioms::ClassExpression;
            match (axiom.sub_class(), axiom.super_class()) {
                (ClassExpression::Class(_), ClassExpression::Class(_)) => {
                    simple_subclass += 1;
                }
                _ => {
                    complex_subclass += 1;
                    if complex_subclass <= 3 {
                        println!(
                            "  Complex subclass: {:?} ⊑ {:?}",
                            axiom.sub_class(),
                            axiom.super_class()
                        );
                    }
                }
            }
        }
        println!(
            "  Simple: {}, Complex: {}",
            simple_subclass, complex_subclass
        );

        // 2. Check disjoint classes axioms
        let disjoint_count = ontology.disjoint_classes_axioms().len();
        println!("\nDisjoint classes axioms: {}", disjoint_count);
        if disjoint_count > 0 {
            for (i, axiom) in ontology
                .disjoint_classes_axioms()
                .iter()
                .take(3)
                .enumerate()
            {
                println!("  Disjoint #{}: {} classes", i, axiom.classes().len());
            }
        }

        // 3. Check equivalent classes axioms
        let equiv_count = ontology.equivalent_classes_axioms().len();
        println!("\nEquivalent classes axioms: {}", equiv_count);

        // 4. Check for disjunctions in axioms
        let mut disjunction_count = 0;
        for axiom in ontology.axioms() {
            if let owl2_reasoner::logic::axioms::Axiom::SubClassOf(sub) = axiom.as_ref() {
                disjunction_count += count_disjunctions(sub.super_class());
            }
        }
        println!("\nDisjunctions (ObjectUnionOf): {}", disjunction_count);

        // Final check
        let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
        println!(
            "\n✓ HierarchicalClassificationEngine::can_handle() = {}",
            can_handle
        );

        if !can_handle {
            println!("\n⚠️  REASONS WHY IT FAILED:");
            if complex_subclass > 0 {
                println!("  - Has {} complex subclass axioms", complex_subclass);
            }
            if disjoint_count > 0 {
                println!("  - Has {} disjoint classes axioms", disjoint_count);
            }
            if disjunction_count > 0 {
                println!("  - Has {} disjunctions", disjunction_count);
            }
        }
    }

    println!("\n========================================");
    println!("Debug complete!");
    println!("========================================");
}

fn count_disjunctions(expr: &owl2_reasoner::logic::axioms::ClassExpression) -> usize {
    use owl2_reasoner::logic::axioms::ClassExpression;
    match expr {
        ClassExpression::ObjectUnionOf(operands) => {
            1 + operands
                .iter()
                .map(|op| count_disjunctions(op))
                .sum::<usize>()
        }
        ClassExpression::ObjectIntersectionOf(operands) => operands
            .iter()
            .map(|op| count_disjunctions(op))
            .sum::<usize>(),
        ClassExpression::ObjectComplementOf(inner) => count_disjunctions(inner),
        ClassExpression::ObjectSomeValuesFrom(_, inner) => count_disjunctions(inner),
        ClassExpression::ObjectAllValuesFrom(_, inner) => count_disjunctions(inner),
        _ => 0,
    }
}
