use owl2_reasoner::{
    Class, ClassAssertionAxiom, ClassExpression, NamedIndividual, Ontology, SchedulingMode,
    SpeculativeConfig, SpeculativeTableauxReasoner, SubClassOfAxiom,
};
use smallvec::smallvec;

fn named(name: &str) -> ClassExpression {
    ClassExpression::Class(Class::new(format!("http://test.org/{name}")))
}

fn clauses(mask: u8, asserted: bool) -> Ontology {
    let mut ontology = Ontology::new();
    for polarity in 0..4 {
        if mask & (1 << polarity) == 0 {
            continue;
        }
        let literal = |name, bit| {
            if polarity & bit == 0 {
                named(name)
            } else {
                ClassExpression::ObjectComplementOf(Box::new(named(name)))
            }
        };
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                named("Trigger"),
                ClassExpression::ObjectUnionOf(smallvec![
                    Box::new(literal("A", 1)),
                    Box::new(literal("B", 2)),
                ]),
            ))
            .unwrap();
    }
    if asserted {
        let witness = NamedIndividual::new("http://test.org/witness");
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                witness.iri().clone(),
                named("Trigger"),
            ))
            .unwrap();
    }
    ontology
}

#[test]
fn all_two_variable_clause_subsets_match_independent_truth_table() {
    for mask in 0_u8..16 {
        // Each polarity encodes one of A|B, !A|B, A|!B, !A|!B.
        let satisfiable = (0..4).any(|assignment| {
            (0..4).all(|polarity| {
                mask & (1 << polarity) == 0
                    || ((assignment & 1 != 0) != (polarity & 1 != 0))
                    || ((assignment & 2 != 0) != (polarity & 2 != 0))
            })
        });
        for asserted in [false, true] {
            let expected = !asserted || satisfiable;
            for mode in [SchedulingMode::Sequential, SchedulingMode::AlwaysParallel] {
                for workers in [1, 2] {
                    for learning in [false, true] {
                        let mut config = SpeculativeConfig::default();
                        config.scheduling_mode = mode;
                        config.num_workers = workers;
                        config.enable_learning = learning;
                        let mut reasoner = SpeculativeTableauxReasoner::with_config(
                            clauses(mask, asserted),
                            config,
                        );
                        for _ in 0..2 {
                            assert_eq!(
                                reasoner.is_consistent().unwrap(),
                                expected,
                                "mask={mask}, asserted={asserted}, mode={mode:?}, workers={workers}, learning={learning}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn an_empty_named_class_does_not_require_an_artificial_branch_witness() {
    let mut ontology = Ontology::new();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            named("Empty"),
            ClassExpression::ObjectUnionOf(smallvec![
                Box::new(ClassExpression::Class(Class::new(
                    "http://www.w3.org/2002/07/owl#Nothing",
                ))),
                Box::new(ClassExpression::Class(Class::new(
                    "http://www.w3.org/2002/07/owl#Nothing",
                ))),
            ]),
        ))
        .unwrap();
    let mut config = SpeculativeConfig::default();
    config.scheduling_mode = SchedulingMode::AlwaysParallel;
    config.num_workers = 2;
    let mut reasoner = SpeculativeTableauxReasoner::with_config(ontology, config);
    assert!(reasoner.is_consistent().unwrap());
    let stats = reasoner.get_stats();
    assert_eq!(stats.boolean_certificate, Some(true));
    assert!(!stats.used_parallel);
    assert_eq!(stats.workers_started, 0);
    assert_eq!(stats.branches_created, 0);
}
