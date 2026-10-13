use std::sync::Arc;

use owl2_reasoner::core::entities::{Class, NamedIndividual};
use owl2_reasoner::logic::axioms::ClassAssertionAxiom;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::{ClassExpression, IRI, Ontology};

const RDF_LIST_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#List";

fn premise_with_class() -> Ontology {
    let mut ontology = Ontology::new();
    ontology
        .add_class(Class::new("http://example.org/a"))
        .unwrap();
    ontology
}

/// The conclusion document's explicit `<rdf:List>` element parses as a
/// class assertion of the list node itself — serialization scaffolding,
/// not a conclusion condition (W3C I5.5-005's singleton unionOf).
fn list_typed_conclusion(subject_iri: &str) -> Ontology {
    let mut ontology = Ontology::new();
    let individual = Arc::new(IRI::new(subject_iri).unwrap());
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            individual,
            ClassExpression::Class(Class::new(RDF_LIST_IRI)),
        ))
        .unwrap();
    ontology
}

#[test]
fn anonymous_rdf_list_conclusion_is_vacuously_entailed() {
    let premise = Arc::new(premise_with_class());
    let conclusion = Arc::new(list_typed_conclusion("_:riog00000002"));
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "the list scaffolding is not a conclusion the premise could fail"
    );
}

#[test]
fn named_rdf_list_individual_conclusion_stays_reduced() {
    let premise = Arc::new(premise_with_class());
    let conclusion = Arc::new(list_typed_conclusion("http://example.org/a-node"));
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "a named individual typed rdf:List is a real assertion the premise does not make"
    );
}
