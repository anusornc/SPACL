use owl2_reasoner::{
    Class, ClassExpression, ObjectProperty, ObjectPropertyExpression, Ontology, SubClassOfAxiom,
};

#[test]
fn qualified_cardinality_exposes_its_filler_through_public_expression_visitors() {
    let filler = Class::new("http://example.org/Doctor");
    let property = ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(
        "http://example.org/hasFriend",
    )));
    let expression = ClassExpression::ObjectMinCardinality(
        2,
        Box::new(property),
        Some(Box::new(ClassExpression::Class(filler.clone()))),
    );
    assert!(expression.contains_class(filler.iri()));
    assert_eq!(expression.collect_subexpressions().len(), 2);
}

#[test]
fn cardinality_validation_visits_nested_qualifiers_without_revisiting_the_root() {
    let property = ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(
        "http://example.org/p",
    )));
    let nested = ClassExpression::ObjectMinCardinality(
        1,
        Box::new(property.clone()),
        Some(Box::new(ClassExpression::ObjectMaxCardinality(
            1_000_001,
            Box::new(property),
            None,
        ))),
    );
    let mut ontology = Ontology::new();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(Class::new("http://example.org/A")),
            nested,
        ))
        .unwrap();
    let errors = ontology.validate_cardinality_constraints().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("1000001"));
}

#[test]
fn tableaux_public_consistency_does_not_ignore_qualified_zero_bounds() {
    use owl2_reasoner::reasoner::tableaux::TableauxReasoner;
    use owl2_reasoner::{ClassAssertionAxiom, NamedIndividual, PropertyAssertionAxiom};

    let subject = NamedIndividual::new("http://example.org/subject");
    let target = NamedIndividual::new("http://example.org/target");
    let property = ObjectProperty::new("http://example.org/p");
    let mut ontology = Ontology::new();
    ontology
        .add_property_assertion(PropertyAssertionAxiom::new(
            subject.iri().clone(),
            property.iri().clone(),
            target.iri().clone(),
        ))
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            subject.iri().clone(),
            ClassExpression::ObjectMaxCardinality(
                0,
                Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(property))),
                Some(Box::new(ClassExpression::Class(Class::new(
                    "http://www.w3.org/2002/07/owl#Thing",
                )))),
            ),
        ))
        .unwrap();
    assert!(!TableauxReasoner::new(ontology).is_consistent().unwrap());
}
