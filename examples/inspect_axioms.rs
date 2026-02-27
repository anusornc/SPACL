#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
use owl2_reasoner::{Ontology, ParserFactory};

fn main() {
    let content = std::fs::read_to_string("tests/data/disjunctive_simple.owl").unwrap();
    let parser = ParserFactory::auto_detect(&content).unwrap();
    let ontology: Ontology = parser.parse_str(&content).unwrap();

    println!("Classes: {}", ontology.classes().len());
    println!("Axioms: {}\n", ontology.axioms().len());

    // Check SubClassOf axioms
    let subclass_axioms = ontology.subclass_axioms();
    println!("SubClassOf axioms: {}", subclass_axioms.len());

    for (i, ax) in subclass_axioms.iter().enumerate() {
        println!("\nAxiom {}:", i);
        println!("  Sub: {:?}", ax.sub_class());
        println!("  Super: {:?}", ax.super_class());
    }
}
