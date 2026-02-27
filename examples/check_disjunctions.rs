#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Check what disjunctions are found in the ontology

fn main() {
    let content = std::fs::read_to_string("tests/data/disjunctive_test.owl").unwrap();
    let parser = owl2_reasoner::ParserFactory::auto_detect(&content).unwrap();
    let ontology = parser.parse_str(&content).unwrap();

    println!(
        "Ontology loaded: {} classes, {} axioms",
        ontology.classes().len(),
        ontology.axioms().len()
    );

    // Check for equivalent classes axioms
    let eq_axioms = ontology.equivalent_classes_axioms();
    println!("\nEquivalent classes axioms: {}", eq_axioms.len());

    for (i, ax) in eq_axioms.iter().enumerate() {
        println!(
            "  Axiom {}: classes involved: {:?}",
            i,
            ax.classes()
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
        );
    }

    // Check subclass axioms for disjunctions
    let sub_axioms = ontology.subclass_axioms();
    println!("\nSubclass axioms: {}", sub_axioms.len());

    for (i, ax) in sub_axioms.iter().enumerate() {
        println!(
            "  Axiom {}: {:?} ⊑ {:?}",
            i,
            ax.sub_class(),
            ax.super_class()
        );
    }
}

fn check_expression(expr: &owl2_reasoner::ClassExpression, indent: usize) {
    let prefix = "  ".repeat(indent);
    match expr {
        owl2_reasoner::ClassExpression::ObjectUnionOf(ops) => {
            println!("{}Found Union! ({} operands)", prefix, ops.len());
            for op in ops {
                check_expression(op, indent + 1);
            }
        }
        owl2_reasoner::ClassExpression::ObjectIntersectionOf(ops) => {
            println!("{}Found Intersection ({} operands)", prefix, ops.len());
            for op in ops {
                check_expression(op, indent + 1);
            }
        }
        owl2_reasoner::ClassExpression::Class(c) => {
            println!("{}Class: {}", prefix, c.iri());
        }
        _ => {
            println!("{}Other expression type", prefix);
        }
    }
}
