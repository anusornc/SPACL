use std::sync::Arc;

use owl2_reasoner::core::entities::{Class, NamedIndividual};
use owl2_reasoner::logic::axioms::ClassAssertionAxiom;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::{ClassExpression, IRI, Ontology};

const OWL_ONTOLOGY_IRI: &str = "http://www.w3.org/2002/07/owl#Ontology";

fn premise_with_individual() -> Ontology {
    let mut ontology = Ontology::new();
    ontology
        .add_named_individual(NamedIndividual::new("http://example.org/a"))
        .unwrap();
    ontology
}

/// The conclusion document's `<owl:Ontology/>` header parses as a
/// class assertion of an anonymous individual typed owl:Ontology —
/// serialization, not a conclusion condition (W3C
/// description-logic-661/665/667's oiled headers).
fn header_conclusion(subject_iri: &str) -> Ontology {
    let mut ontology = Ontology::new();
    let individual = Arc::new(IRI::new(subject_iri).unwrap());
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            individual,
            ClassExpression::Class(Class::new(OWL_ONTOLOGY_IRI)),
        ))
        .unwrap();
    ontology
}

#[test]
fn anonymous_ontology_header_conclusion_is_vacuously_entailed() {
    let premise = Arc::new(premise_with_individual());
    let conclusion = Arc::new(header_conclusion("_:riog00000001"));
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "the document header is not a conclusion the premise could fail"
    );
}

#[test]
fn named_ontology_individual_conclusion_stays_reduced() {
    let premise = Arc::new(premise_with_individual());
    let conclusion = Arc::new(header_conclusion("http://example.org/a"));
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "a named individual typed owl:Ontology is a real assertion the \
         premise does not make — genuine non-entailment"
    );
}
