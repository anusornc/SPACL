use owl2_reasoner::{
    Class, ClassAssertionAxiom, ClassExpression, DisjointClassesAxiom, EquivalentClassesAxiom,
    NamedIndividual, ObjectProperty, ObjectPropertyExpression, Ontology, OwlError, SimpleReasoner,
    SubClassOfAxiom,
};
use smallvec::smallvec;

fn class(iri: &str) -> Class {
    Class::new(format!("http://example.org/{iri}"))
}

fn individual(iri: &str) -> NamedIndividual {
    NamedIndividual::new(format!("http://example.org/{iri}"))
}

fn expr(class: &Class) -> ClassExpression {
    ClassExpression::Class(class.clone())
}

fn not(expression: ClassExpression) -> ClassExpression {
    ClassExpression::ObjectComplementOf(Box::new(expression))
}

fn and(expressions: Vec<ClassExpression>) -> ClassExpression {
    ClassExpression::ObjectIntersectionOf(expressions.into_iter().map(Box::new).collect())
}

fn or(expressions: Vec<ClassExpression>) -> ClassExpression {
    ClassExpression::ObjectUnionOf(expressions.into_iter().map(Box::new).collect())
}

fn object_property(iri: &str) -> ObjectPropertyExpression {
    ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(format!(
        "http://example.org/{iri}"
    ))))
}

#[test]
fn four_clause_boolean_tbox_can_be_unsatisfiable_without_abox() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let b = class("B");

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing")),
            and(vec![
                or(vec![expr(&a), expr(&b)]),
                or(vec![expr(&a), not(expr(&b))]),
                or(vec![not(expr(&a)), expr(&b)]),
                or(vec![not(expr(&a)), not(expr(&b))]),
            ]),
        ))
        .unwrap();

    assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn satisfiable_three_sat_style_boolean_tbox_remains_consistent() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let b = class("B");
    let c = class("C");

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing")),
            and(vec![
                or(vec![expr(&a), expr(&b), expr(&c)]),
                or(vec![not(expr(&a)), expr(&b), expr(&c)]),
                or(vec![expr(&a), not(expr(&b)), expr(&c)]),
                or(vec![expr(&a), expr(&b), not(expr(&c))]),
            ]),
        ))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn vacuous_boolean_tbox_still_requires_a_nonempty_domain_model() {
    let mut ontology = Ontology::new();
    let a = class("A");

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            and(vec![expr(&a), not(expr(&a))]),
            ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Nothing")),
        ))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn disjoint_classes_are_checked_per_individual_independently() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let b = class("B");
    let x = individual("x");
    let y = individual("y");

    ontology
        .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            a.iri().clone(),
            b.iri().clone(),
        ]))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&a)))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(y.iri().clone(), expr(&b)))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn direct_complement_assertion_contradiction_is_inconsistent() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let x = individual("x");

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&a)))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), not(expr(&a))))
        .unwrap();

    assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn named_equivalence_propagates_into_disjoint_contradictions() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let b = class("B");
    let c = class("C");
    let x = individual("x");

    ontology
        .add_equivalent_classes_axiom(EquivalentClassesAxiom::new(vec![
            a.iri().clone(),
            b.iri().clone(),
        ]))
        .unwrap();
    ontology
        .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            b.iri().clone(),
            c.iri().clone(),
        ]))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&a)))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&c)))
        .unwrap();

    assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn top_bottom_and_empty_boolean_expressions_follow_boolean_identities() {
    let mut ontology = Ontology::new();
    let x = individual("x");

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing")),
        ))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            ClassExpression::ObjectIntersectionOf(smallvec![]),
        ))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            not(ClassExpression::ObjectUnionOf(smallvec![])),
        ))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());

    let mut inconsistent = Ontology::new();
    inconsistent
        .add_class_assertion(ClassAssertionAxiom::new(
            individual("y").iri().clone(),
            ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Nothing")),
        ))
        .unwrap();

    assert!(!SimpleReasoner::new(inconsistent).is_consistent().unwrap());
}

#[test]
fn anonymous_subclass_lhs_implication_is_enforced() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let b = class("B");
    let c = class("C");
    let x = individual("x");

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            and(vec![expr(&a), expr(&b)]),
            expr(&c),
        ))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            and(vec![expr(&a), expr(&b), not(expr(&c))]),
        ))
        .unwrap();

    assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn unsupported_mixed_expression_does_not_create_boolean_certificate() {
    let mut ontology = Ontology::new();
    let a = class("A");
    let filler = class("Filler");
    let x = individual("x");

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expr(&a)))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            ClassExpression::ObjectSomeValuesFrom(
                Box::new(object_property("p")),
                Box::new(expr(&filler)),
            ),
        ))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn unsupported_mixed_fragment_is_detected_before_boolean_size_limits() {
    let mut ontology = Ontology::new();
    let x = individual("x");
    let filler = class("Filler");
    let many_names = (0..2_100)
        .map(|index| expr(&class(&format!("A{index}"))))
        .collect::<Vec<_>>();

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), or(many_names)))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            x.iri().clone(),
            ClassExpression::ObjectSomeValuesFrom(
                Box::new(object_property("p")),
                Box::new(expr(&filler)),
            ),
        ))
        .unwrap();

    assert!(SimpleReasoner::new(ontology).is_consistent().unwrap());
}

#[test]
fn exact_deep_boolean_resource_exhaustion_is_not_legacy_sat() {
    let mut ontology = Ontology::new();
    let x = individual("x");
    let mut expression = expr(&class("A"));
    for _ in 0..257 {
        expression = not(expression);
    }

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), expression))
        .unwrap();

    let result = SimpleReasoner::new(ontology).is_consistent();
    assert!(matches!(
        result,
        Err(OwlError::ResourceLimitExceeded { resource_type, .. })
            if resource_type == "boolean consistency expression depth"
    ));
}

#[test]
fn boolean_resource_exhaustion_is_an_error_not_a_sat_certificate() {
    let mut ontology = Ontology::new();
    let x = individual("x");
    let many_names = (0..2_100)
        .map(|index| expr(&class(&format!("A{index}"))))
        .collect::<Vec<_>>();

    ontology
        .add_class_assertion(ClassAssertionAxiom::new(x.iri().clone(), or(many_names)))
        .unwrap();

    let result = SimpleReasoner::new(ontology).is_consistent();
    assert!(matches!(
        result,
        Err(OwlError::ResourceLimitExceeded { resource_type, .. }) if resource_type == "boolean consistency variables"
    ));
}
