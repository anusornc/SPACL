use std::sync::Arc;

use owl2_reasoner::{
    evaluate_consistency_with_config, Axiom, Class, ClassAssertionAxiom, ClassExpression,
    DifferentIndividualsAxiom, IrreflexivePropertyAxiom, NamedIndividual,
    NegativeObjectPropertyAssertionAxiom, ObjectProperty, ObjectPropertyDomainAxiom,
    ObjectPropertyExpression, Ontology, OwlError, PropertyAssertionAxiom, SameIndividualAxiom,
    SchedulingMode, SemanticResult, SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner,
    SubClassOfAxiom,
};
use smallvec::smallvec;

fn class(name: &str) -> Class {
    Class::new(format!("http://test.org/{name}"))
}

fn named(name: &str) -> ClassExpression {
    ClassExpression::Class(class(name))
}

fn nothing() -> ClassExpression {
    ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Nothing"))
}

fn thing() -> ClassExpression {
    ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing"))
}

fn and(expressions: Vec<ClassExpression>) -> ClassExpression {
    ClassExpression::ObjectIntersectionOf(expressions.into_iter().map(Box::new).collect())
}

fn individual(name: &str) -> NamedIndividual {
    NamedIndividual::new(format!("http://test.org/{name}"))
}

fn property(name: &str) -> ObjectProperty {
    ObjectProperty::new(format!("http://test.org/{name}"))
}

fn property_expr(name: &str) -> Box<ObjectPropertyExpression> {
    Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
        property(name),
    )))
}

fn property_expr_iri(iri: &str) -> Box<ObjectPropertyExpression> {
    Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
        ObjectProperty::new(iri),
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

fn assert_edge(
    ontology: &mut Ontology,
    subject: &NamedIndividual,
    role: &ObjectProperty,
    object: &NamedIndividual,
) {
    ontology
        .add_property_assertion(PropertyAssertionAxiom::new(
            subject.iri().clone(),
            role.iri().clone(),
            object.iri().clone(),
        ))
        .unwrap();
}

fn assert_negative_edge(
    ontology: &mut Ontology,
    subject: &NamedIndividual,
    role: &ObjectProperty,
    object: &NamedIndividual,
) {
    ontology
        .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
            NegativeObjectPropertyAssertionAxiom::new(
                subject.iri().clone(),
                role.iri().clone(),
                object.iri().clone(),
            ),
        )))
        .unwrap();
}

fn always_parallel_config() -> SpeculativeConfig {
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = SchedulingMode::AlwaysParallel;
    config.num_workers = 4;
    config.parallel_threshold = 1;
    config
}

#[test]
fn qualified_max_counts_only_known_distinct_qualifying_targets() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, named("Target"));
    assert_class(&mut ontology, &right, named("Target"));
    ontology
        .add_axiom(Axiom::DifferentIndividuals(Box::new(
            DifferentIndividualsAxiom::new(vec![left.iri().clone(), right.iri().clone()]),
        )))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn qualified_max_does_not_use_unique_name_assumption() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, named("Target"));
    assert_class(&mut ontology, &right, named("Target"));
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn disjoint_class_memberships_make_fillers_distinct_for_qualified_max() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");
    let a = class("A");
    let b = class("B");

    ontology
        .add_disjoint_classes_axiom(owl2_reasoner::DisjointClassesAxiom::new(vec![
            a.iri().clone(),
            b.iri().clone(),
        ]))
        .unwrap();
    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, ClassExpression::Class(a));
    assert_class(&mut ontology, &right, ClassExpression::Class(b));
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(thing())),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn non_clique_distinctness_graph_is_not_mistaken_for_two_mergeable_targets() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let fillers = [
        individual("f0"),
        individual("f1"),
        individual("f2"),
        individual("f3"),
        individual("f4"),
    ];
    let relation = property("relation");

    for filler in &fillers {
        assert_edge(&mut ontology, &owner, &relation, filler);
    }
    for (left, right) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)] {
        ontology
            .add_axiom(Axiom::DifferentIndividuals(Box::new(
                DifferentIndividualsAxiom::new(vec![
                    fillers[left].iri().clone(),
                    fillers[right].iri().clone(),
                ]),
            )))
            .unwrap();
    }
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            2,
            property_expr("relation"),
            Some(Box::new(thing())),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn qualified_min_missing_edges_is_open_world_consistent() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");

    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            2,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn qualified_exact_zero_rejects_known_qualifying_edge_but_ignores_nonqualifying_edge() {
    let mut consistent = Ontology::new();
    let owner = individual("owner");
    let filler = individual("filler");
    let relation = property("relation");
    assert_edge(&mut consistent, &owner, &relation, &filler);
    assert_class(
        &mut consistent,
        &owner,
        ClassExpression::ObjectExactCardinality(
            0,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );
    assert_eq!(
        SimpleReasoner::new(consistent).is_consistent().unwrap(),
        true
    );

    let mut inconsistent = Ontology::new();
    assert_edge(&mut inconsistent, &owner, &relation, &filler);
    assert_class(&mut inconsistent, &filler, named("Target"));
    assert_class(
        &mut inconsistent,
        &owner,
        ClassExpression::ObjectExactCardinality(
            0,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );
    assert_eq!(
        SimpleReasoner::new(inconsistent).is_consistent().unwrap(),
        false
    );
}

#[test]
fn duplicate_edges_to_the_same_target_are_counted_once() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let filler = individual("filler");
    let relation = property("relation");

    assert_edge(&mut ontology, &owner, &relation, &filler);
    assert_edge(&mut ontology, &owner, &relation, &filler);
    assert_class(&mut ontology, &filler, named("Target"));
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn bottom_qualifier_makes_positive_minimum_unsatisfiable() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(nothing())),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn conjunctive_named_qualifier_requires_all_explicit_memberships() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, named("Target"));
    assert_class(&mut ontology, &left, named("Enabled"));
    assert_class(&mut ontology, &right, named("Target"));
    ontology
        .add_axiom(Axiom::DifferentIndividuals(Box::new(
            DifferentIndividualsAxiom::new(vec![left.iri().clone(), right.iri().clone()]),
        )))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(and(vec![named("Target"), named("Enabled")]))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn has_self_conflicts_with_irreflexive_role() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let relation = property("relation");

    ontology
        .add_axiom(Axiom::IrreflexiveProperty(Box::new(
            IrreflexivePropertyAxiom::new(relation.iri().clone()),
        )))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectHasSelf(property_expr("relation")),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn existing_named_fillers_do_not_block_fresh_minimum_witness_model() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let controller = individual("controller");
    let relation = property("relation");
    let side = property("side");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, named("C"));
    assert_class(&mut ontology, &right, named("C"));
    assert_edge(&mut ontology, &controller, &side, &left);
    assert_edge(&mut ontology, &controller, &side, &right);
    assert_class(
        &mut ontology,
        &controller,
        ClassExpression::ObjectMaxCardinality(1, property_expr("side"), Some(Box::new(thing()))),
    );
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            2,
            property_expr("relation"),
            Some(Box::new(named("C"))),
        ),
    );

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn implicit_thing_makes_disjoint_thing_qualified_minimum_unsat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let c = class("C");
    ontology
        .add_disjoint_classes_axiom(owl2_reasoner::DisjointClassesAxiom::new(vec![
            Class::new("http://www.w3.org/2002/07/owl#Thing")
                .iri()
                .clone(),
            c.iri().clone(),
        ]))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(ClassExpression::Class(c))),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn huge_minimum_cardinality_hits_resource_bound_before_witness_loop() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            u32::MAX,
            property_expr("relation"),
            Some(Box::new(named("C"))),
        ),
    );

    // Budget exhaustion of the bounded checker still surfaces (#113
    // review): it guards the checker's own correctness.
    assert!(matches!(
        SimpleReasoner::new(ontology).is_consistent(),
        Err(OwlError::ResourceLimitExceeded { .. })
    ));
}

#[test]
fn top_object_property_negative_assertion_is_unsupported_in_targeted_fragment() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let top = ObjectProperty::new("http://www.w3.org/2002/07/owl#topObjectProperty");

    assert_negative_edge(&mut ontology, &left, &top, &right);
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("C"))),
        ),
    );

    // #113: satisfiable (no edge exists to contradict), decided below.
    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn same_as_in_targeted_fragment_is_explicitly_unsupported() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let alias = individual("alias");
    ontology
        .add_axiom(Axiom::SameIndividual(Box::new(SameIndividualAxiom::new(
            vec![owner.iri().clone(), alias.iri().clone()],
        ))))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    // #113: the bounded checker declines, the sequential layers decide.
    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn role_tbox_in_targeted_fragment_is_explicitly_unsupported() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    ontology
        .add_axiom(Axiom::ObjectPropertyDomain(Box::new(
            ObjectPropertyDomainAxiom::new(
                property("relation").iri().clone(),
                class("Owner").iri().clone(),
            ),
        )))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    // #113: the bounded checker declines, the sequential layers decide.
    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn unsupported_qualifier_reports_unknown_through_evaluator() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(ClassExpression::ObjectUnionOf(smallvec![
                Box::new(named("Left")),
                Box::new(named("Right")),
            ]))),
        ),
    );

    let response = evaluate_consistency_with_config(
        Arc::new(ontology),
        "memory",
        SpeculativeConfig::default(),
    );

    // #113: the union qualifier makes the bounded checker decline; the
    // sequential layers answer (satisfiable).
    assert_eq!(response.semantic_result, SemanticResult::Consistent);
    assert!(response.unsupported_fragment.is_none());
}

#[test]
fn qualified_certificate_bypasses_artificial_spacl_workers_in_scheduling_modes() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    for mode in [SchedulingMode::Sequential, SchedulingMode::AlwaysParallel] {
        let mut config = always_parallel_config();
        config.scheduling_mode = mode;
        let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);

        assert_eq!(reasoner.is_consistent().unwrap(), true);
        let stats = reasoner.get_stats();
        assert_eq!(stats.qualified_consistency_certificate, Some(true));
        assert_eq!(stats.workers_started, 0);
        assert!(!stats.used_parallel);
    }
}

#[test]
fn class_tbox_targeted_cardinality_is_not_default_sat() {
    let mut ontology = Ontology::new();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            named("Owner"),
            ClassExpression::ObjectMinCardinality(
                1,
                property_expr("relation"),
                Some(Box::new(named("Target"))),
            ),
        ))
        .unwrap();

    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn ordinary_subclass_tbox_mixed_with_targeted_assertion_is_unsupported() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(named("A"), named("B")))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    // #113: satisfiable; the mixed fragment falls through.
    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]

#[test]
fn critic_disjoint_named_fillers_count_against_top_qualified_max() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");
    let c = class("C");
    let d = class("D");

    ontology
        .add_disjoint_classes_axiom(owl2_reasoner::DisjointClassesAxiom::new(vec![
            c.iri().clone(),
            d.iri().clone(),
        ]))
        .unwrap();
    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_class(&mut ontology, &left, ClassExpression::Class(c));
    assert_class(&mut ontology, &right, ClassExpression::Class(d));
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(thing())),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}

#[test]
fn critic_odd_cycle_distinctness_cannot_be_merged_into_two_targets() {
    non_clique_distinctness_graph_is_not_mistaken_for_two_mergeable_targets();
}

#[test]
fn critic_top_object_property_zero_max_is_not_reported_sat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            0,
            property_expr_iri("http://www.w3.org/2002/07/owl#topObjectProperty"),
            Some(Box::new(thing())),
        ),
    );

    // #113: satisfiable (owner has no edge at all).
    assert_eq!(SimpleReasoner::new(ontology).is_consistent().unwrap(), true);
}

#[test]
fn critic_minimum_with_disjoint_conjunctive_qualifier_is_unsat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let c = class("C");
    let d = class("D");
    ontology
        .add_disjoint_classes_axiom(owl2_reasoner::DisjointClassesAxiom::new(vec![
            c.iri().clone(),
            d.iri().clone(),
        ]))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(and(vec![
                ClassExpression::Class(c),
                ClassExpression::Class(d),
            ]))),
        ),
    );

    assert_eq!(
        SimpleReasoner::new(ontology).is_consistent().unwrap(),
        false
    );
}


