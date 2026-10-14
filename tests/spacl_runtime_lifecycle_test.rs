use owl2_reasoner::{
    Axiom, Class, ClassExpression, IrreflexivePropertyAxiom, NamedIndividual, ObjectProperty,
    Ontology, PropertyAssertionAxiom, SchedulingMode, SimpleReasoner, SpeculativeConfig,
    SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use smallvec::smallvec;

fn class_expr(iri: &str) -> ClassExpression {
    ClassExpression::Class(Class::new(iri))
}

fn forced_parallel_config() -> SpeculativeConfig {
    let mut config = SpeculativeConfig::default();
    config.num_workers = 1;
    config.scheduling_mode = SchedulingMode::AlwaysParallel;
    config.speculative_timeout_ms = 5_000;
    config
}

fn globally_contradictory_ontology_with_unrelated_union() -> Ontology {
    let mut ontology = Ontology::new();
    add_role_edge(&mut ontology, true);

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            class_expr("http://test.org/Trigger"),
            ClassExpression::ObjectUnionOf(smallvec![
                Box::new(class_expr("http://test.org/Left")),
                Box::new(class_expr("http://test.org/Right")),
            ]),
        ))
        .unwrap();

    ontology
}

fn add_role_edge(ontology: &mut Ontology, self_edge: bool) {
    // This mixed fragment intentionally exercises real workers, not the exact
    // Boolean shortcut. An irreflexive self-edge is the contradiction control.
    let subject = NamedIndividual::new("http://test.org/subject");
    let object = NamedIndividual::new("http://test.org/object");
    let property = ObjectProperty::new("http://test.org/relation")
        .iri()
        .clone();
    ontology
        .add_axiom(Axiom::IrreflexiveProperty(Box::new(
            IrreflexivePropertyAxiom::new(property.clone()),
        )))
        .unwrap();
    ontology
        .add_property_assertion(PropertyAssertionAxiom::new(
            subject.iri().clone(),
            property,
            if self_edge {
                subject.iri().clone()
            } else {
                object.iri().clone()
            },
        ))
        .unwrap();
}

#[test]
fn disabled_learning_does_not_verify_add_or_count_nogoods() {
    let mut config = forced_parallel_config();
    config.enable_learning = false;

    let mut reasoner = SpeculativeTableauxReasoner::with_config(
        globally_contradictory_ontology_with_unrelated_union().clone(),
        config,
    );
    let baseline_result =
        SimpleReasoner::new(globally_contradictory_ontology_with_unrelated_union())
            .is_consistent()
            .unwrap();

    let result = reasoner.is_consistent();

    assert!(result.is_ok());
    assert!(
        !baseline_result,
        "an irreflexive role self-edge is inconsistent"
    );
    assert_eq!(
        result.unwrap(),
        baseline_result,
        "speculative path must preserve the baseline verdict"
    );
    assert!(reasoner.get_stats().used_parallel);
    assert_eq!(
        reasoner.get_nogood_stats().0,
        0,
        "disabled learning must not add nogoods"
    );
    assert_eq!(
        reasoner.get_stats().nogoods_learned,
        0,
        "disabled learning must not count learned nogoods"
    );
    assert_eq!(
        reasoner.get_stats().nogood_checks,
        0,
        "disabled learning must not perform nogood checks"
    );
}

#[test]
fn repeated_consistency_calls_finish_every_spawned_worker() {
    let mut ontology = Ontology::new();
    add_role_edge(&mut ontology, false);
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            class_expr("http://test.org/Trigger"),
            ClassExpression::ObjectUnionOf(smallvec![
                Box::new(class_expr("http://test.org/Left")),
                Box::new(class_expr("http://test.org/Right")),
            ]),
        ))
        .unwrap();

    let mut config = forced_parallel_config();
    config.num_workers = 128;
    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology, config.clone());

    for _ in 0..3 {
        let before = reasoner.get_stats();
        assert_eq!(reasoner.is_consistent().unwrap(), true);
        let stats = reasoner.get_stats();
        assert!(stats.used_parallel);
        assert!(
            stats.report().contains(&format!(
                "Workers: {} started, {} completed",
                stats.workers_started, stats.workers_completed
            )),
            "stats report must expose worker completion counters"
        );
        assert_eq!(
            stats.workers_started - before.workers_started,
            config.num_workers,
            "is_consistent must start every configured worker before returning"
        );
        assert_eq!(
            stats.workers_completed - before.workers_completed,
            config.num_workers,
            "is_consistent must not return before spawned workers finish"
        );
    }
}

#[test]
fn parallel_success_verdict_stays_true_under_bounded_stress() {
    let mut ontology = Ontology::new();
    add_role_edge(&mut ontology, false);
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            class_expr("http://test.org/Trigger"),
            ClassExpression::ObjectUnionOf(smallvec![
                Box::new(class_expr("http://test.org/Left")),
                Box::new(class_expr("http://test.org/Right")),
            ]),
        ))
        .unwrap();

    let mut config = forced_parallel_config();
    config.num_workers = 32;

    for _ in 0..64 {
        let mut reasoner =
            SpeculativeTableauxReasoner::with_config(ontology.clone(), config.clone());
        assert_eq!(
            reasoner.is_consistent().unwrap(),
            true,
            "parallel success must not be lost when outstanding work reaches zero"
        );
        let stats = reasoner.get_stats();
        assert!(stats.used_parallel);
        assert_eq!(stats.workers_started, config.num_workers);
        assert_eq!(stats.workers_completed, config.num_workers);
    }
}
