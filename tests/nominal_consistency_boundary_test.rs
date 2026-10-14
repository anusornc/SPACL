use std::sync::Arc;

use owl2_reasoner::reasoning::tableaux::TableauxReasoner;
use owl2_reasoner::{
    evaluate_consistency_with_config, Class, ClassAssertionAxiom, ClassExpression, Individual,
    NamedIndividual, ObjectProperty, ObjectPropertyExpression, Ontology, OwlError, SchedulingMode,
    SemanticResult, SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner,
};
use smallvec::smallvec;

fn individual(name: &str) -> NamedIndividual {
    NamedIndividual::new(format!("http://test.org/{name}"))
}

fn nominal(names: &[&str]) -> ClassExpression {
    ClassExpression::ObjectOneOf(Box::new(
        names
            .iter()
            .map(|name| Individual::Named(individual(name)))
            .collect(),
    ))
}

fn property_expr(name: &str) -> Box<ObjectPropertyExpression> {
    Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
        ObjectProperty::new(format!("http://test.org/{name}")),
    )))
}

fn assert_class(
    ontology: &mut Ontology,
    individual: &NamedIndividual,
    expression: ClassExpression,
) {
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            individual.iri().clone(),
            expression,
        ))
        .unwrap();
}

fn unsupported(result: Result<bool, OwlError>) {
    assert!(matches!(result, Err(OwlError::UnsupportedFeature(_))));
}

fn always_parallel_config() -> SpeculativeConfig {
    SpeculativeConfig {
        scheduling_mode: SchedulingMode::AlwaysParallel,
        num_workers: 4,
        parallel_threshold: 1,
        ..SpeculativeConfig::default()
    }
}

#[test]
fn public_consistency_entrypoints_decide_nonempty_object_one_of() {
    // #113: a nominal assertion used to abort every entrypoint as
    // unsupported; the sequential layers now decide, and a membership
    // in {a, b} is satisfiable.
    let mut ontology = Ontology::new();
    assert_class(&mut ontology, &individual("x"), nominal(&["a", "b"]));

    assert_eq!(SimpleReasoner::new(ontology.clone()).is_consistent().unwrap(), true);

    let mut tableaux = TableauxReasoner::new(ontology.clone());
    assert_eq!(tableaux.check_consistency().unwrap(), true);

    let mut spacl =
        SpeculativeTableauxReasoner::with_config(ontology.clone(), always_parallel_config());
    assert_eq!(spacl.is_consistent().unwrap(), true);

    let response =
        evaluate_consistency_with_config(Arc::new(ontology), "memory", always_parallel_config());
    assert_eq!(response.semantic_result, SemanticResult::Consistent);
    assert!(response.unsupported_fragment.is_none());
}

#[test]
fn nested_nonempty_object_one_of_qualifier_is_decided_consistent() {
    // #113: satisfiable (an "allowed" successor exists), now decided
    // instead of aborting as unsupported.
    let mut ontology = Ontology::new();
    assert_class(
        &mut ontology,
        &individual("owner"),
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(ClassExpression::ObjectIntersectionOf(smallvec![
                Box::new(ClassExpression::Class(Class::new("http://test.org/Target"))),
                Box::new(nominal(&["allowed"])),
            ]))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology.clone()).is_consistent().unwrap(), true);

    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology, always_parallel_config());
    assert_eq!(spacl.is_consistent().unwrap(), true);
}

#[test]
fn empty_object_one_of_keeps_programmatic_empty_class_behavior() {
    let mut ontology = Ontology::new();
    assert_class(
        &mut ontology,
        &individual("x"),
        ClassExpression::ObjectOneOf(Box::new(smallvec![])),
    );

    assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
}
