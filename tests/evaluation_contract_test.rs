use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use owl2_reasoner::{
    evaluate_consistency, evaluate_entailment, evaluate_inconsistency, Class, ClassAssertionAxiom,
    ClassExpression, DisjointClassesAxiom, EquivalentClassesAxiom, NamedIndividual, ObjectProperty,
    ObjectPropertyExpression, Ontology, SubClassOfAxiom,
};

fn subclass_ontology(subclass: &Class, superclass: &Class) -> Ontology {
    let mut ontology = Ontology::new();
    ontology.add_class(subclass.clone()).unwrap();
    ontology.add_class(superclass.clone()).unwrap();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(subclass.clone()),
            ClassExpression::Class(superclass.clone()),
        ))
        .unwrap();
    ontology
}

#[test]
fn consistency_evaluation_returns_machine_readable_semantic_result() {
    let response = evaluate_consistency(Arc::new(Ontology::new()), "empty.owl");

    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.operation.as_str(), "consistency");
    assert_eq!(response.semantic_result.as_str(), "consistent");
    assert_eq!(response.input_ontology, "empty.owl");
    assert!(serde_json::to_value(&response).unwrap()["reason_time_ms"].is_number());
    assert!(serde_json::to_value(&response).unwrap()["runtime_metadata"].is_object());
    assert_eq!(response.runtime_metadata.flags.len(), 12);
    assert_eq!(
        response.runtime_metadata.supported_restrictions,
        ["owl2_consistency"]
    );
    assert!(!response.runtime_metadata.effective_config.is_empty());
}

#[test]
fn inconsistency_evaluation_returns_boolean_semantic_result() {
    let response = evaluate_inconsistency(Arc::new(Ontology::new()), "empty.owl");

    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.operation.as_str(), "inconsistency");
    assert_eq!(response.semantic_result.as_str(), "false");
}

#[test]
fn inconsistency_evaluation_detects_irreflexive_self_assertion() {
    let root = env!("CARGO_MANIFEST_DIR");
    let ontology = format!("{root}/tests/data/inconsistent_irreflexive_property.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "inconsistency", &ontology])
        .output()
        .expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["execution_status"], "success");
    assert_eq!(response["semantic_result"], "true");
}

#[test]
fn consistency_evaluation_allows_mutual_subclass_relationships() {
    let root = env!("CARGO_MANIFEST_DIR");
    let ontology = format!("{root}/tests/data/consistent_subclass_cycle.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "consistency", &ontology])
        .output()
        .expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["execution_status"], "success");
    assert_eq!(response["semantic_result"], "consistent");
}

#[test]
fn inconsistency_evaluation_detects_empty_some_values_expression() {
    let individual = NamedIndividual::new("http://example.org/a");
    let property = ObjectProperty::new("http://example.org/op");
    let nothing = Class::new("http://www.w3.org/2002/07/owl#Nothing");
    let mut ontology = Ontology::new();
    ontology.add_named_individual(individual.clone()).unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            individual.iri().clone(),
            ClassExpression::ObjectSomeValuesFrom(
                Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(property))),
                Box::new(ClassExpression::Class(nothing)),
            ),
        ))
        .unwrap();

    let response = evaluate_inconsistency(Arc::new(ontology), "nothing-restriction.owl");
    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn rdf_xml_functional_datatype_property_keeps_data_property_kind() {
    let root = env!("CARGO_MANIFEST_DIR");
    let ontology_path = format!("{root}/tests/data/functional_data_property.owl");
    let ontology =
        owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&ontology_path))
            .expect("functional datatype property fixture should parse");

    assert_eq!(ontology.functional_data_property_axioms().len(), 1);
    assert!(ontology.functional_property_axioms().is_empty());
}

#[test]
fn rdf_xml_nested_bottom_property_restrictions_become_empty_class_assertions() {
    let root = env!("CARGO_MANIFEST_DIR");
    let cases = [
        (
            "tests/data/w3c_bottom_data_property_restriction.owl",
            "http://www.w3.org/2002/07/owl#bottomDataProperty",
            "data",
        ),
        (
            "tests/data/w3c_bottom_object_property_restriction.owl",
            "http://www.w3.org/2002/07/owl#bottomObjectProperty",
            "object",
        ),
    ];

    for (relative_path, property_iri, kind) in cases {
        let path = format!("{root}/{relative_path}");
        let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
            .expect("W3C nested restriction fixture should parse");
        let assertions: Vec<_> = ontology
            .class_assertions()
            .into_iter()
            .filter(|assertion| assertion.individual().as_str() == "http://example.org/i")
            .collect();
        assert_eq!(
            assertions.len(),
            1,
            "{kind} restriction should be materialized"
        );

        match (kind, assertions[0].class_expr()) {
            ("data", ClassExpression::DataSomeValuesFrom(property, _)) => {
                assert_eq!(
                    property
                        .as_named()
                        .expect("named data property")
                        .iri()
                        .as_str(),
                    property_iri
                );
            }
            ("object", ClassExpression::ObjectSomeValuesFrom(property, _)) => {
                assert_eq!(
                    property
                        .as_named()
                        .expect("named object property")
                        .iri()
                        .as_str(),
                    property_iri
                );
            }
            (expected_kind, expression) => {
                panic!("expected {expected_kind} bottom restriction, got {expression:?}")
            }
        }

        let response = evaluate_inconsistency(Arc::new(ontology), &path);
        assert_eq!(
            response.semantic_result.as_str(),
            "true",
            "{kind} restriction"
        );
    }
}

#[test]
fn rdf_xml_negative_property_assertions_conflict_with_positive_assertions() {
    let root = env!("CARGO_MANIFEST_DIR");
    for relative_path in [
        "tests/data/negative_data_property_assertion.owl",
        "tests/data/negative_object_property_assertion.owl",
    ] {
        let path = format!("{root}/{relative_path}");
        let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
            .expect("negative property fixture should parse");
        let response = evaluate_inconsistency(Arc::new(ontology), &path);
        assert_eq!(response.semantic_result.as_str(), "true", "{relative_path}");
    }
}

#[test]
fn rdf_xml_disjoint_data_properties_conflict_on_same_value() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/disjoint_data_properties.owl");
    let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
        .expect("disjoint data property fixture should parse");
    assert_eq!(ontology.disjoint_data_properties_axioms().len(), 1);

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn rdf_xml_functional_data_property_rejects_distinct_values() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/functional_data_property_conflict.owl");
    let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
        .expect("functional data property fixture should parse");
    assert_eq!(ontology.functional_data_property_axioms().len(), 1);

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn rdf_xml_complement_of_top_object_existential_is_empty() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/top_object_property_restriction.owl");
    let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
        .expect("top object property fixture should parse");

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn rdf_xml_max_cardinality_rejects_known_distinct_fillers() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/max_cardinality_conflict.owl");
    let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
        .expect("max cardinality fixture should parse");

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn rdf_xml_datatype_restrictions_feed_consistency_checks() {
    let root = env!("CARGO_MANIFEST_DIR");
    for relative_path in [
        "tests/data/empty_float_interval.owl",
        "tests/data/rational_one_of_min_cardinality.owl",
    ] {
        let path = format!("{root}/{relative_path}");
        let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
            .expect("datatype restriction fixture should parse");
        let response = evaluate_inconsistency(Arc::new(ontology), &path);
        assert_eq!(response.semantic_result.as_str(), "true", "{relative_path}");
    }
}

#[test]
fn rdf_xml_key_equal_values_conflict_with_different_individuals() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/has_key_different_individuals.owl");
    let ontology = owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
        .expect("hasKey fixture should parse");
    assert_eq!(ontology.has_key_axioms().len(), 1);

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn entailment_evaluation_supports_named_class_subsumption() {
    let subclass = Class::new("http://example.org/A");
    let superclass = Class::new("http://example.org/B");
    let premise = Arc::new(subclass_ontology(&subclass, &superclass));
    let conclusion = Arc::new(subclass_ontology(&subclass, &superclass));

    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");

    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.operation.as_str(), "entailment");
    assert_eq!(response.semantic_result.as_str(), "true");
    assert_eq!(
        response.supported_scope.as_deref(),
        Some("owl2_entailment_consistency_reduction")
    );
}

#[test]
fn entailment_evaluation_reports_false_for_unentailed_subsumption() {
    let subclass = Class::new("http://example.org/A");
    let superclass = Class::new("http://example.org/B");
    let premise = Arc::new(subclass_ontology(&subclass, &superclass));
    let conclusion = Arc::new(subclass_ontology(&superclass, &subclass));

    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");

    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "false");
}

#[test]
fn entailment_evaluation_decides_disjointness_conclusions() {
    let left = Class::new("http://example.org/A");
    let right = Class::new("http://example.org/B");
    let premise = Arc::new(Ontology::new());
    let mut conclusion_ontology = Ontology::new();
    conclusion_ontology.add_class(left.clone()).unwrap();
    conclusion_ontology.add_class(right.clone()).unwrap();
    conclusion_ontology
        .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            left.iri().clone(),
            right.iri().clone(),
        ]))
        .unwrap();

    let response = evaluate_entailment(
        premise,
        Arc::new(conclusion_ontology),
        "premise.owl",
        "conclusion.owl",
    );

    // An empty premise does not entail disjointness: the C ⊓ D witness
    // stays consistent, so this is a genuine non-entailment.
    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "false");
}

#[test]
fn entailment_evaluation_relaxes_the_premise_gate() {
    let left = Class::new("http://example.org/A");
    let right = Class::new("http://example.org/B");
    let mut premise_ontology = subclass_ontology(&left, &right);
    premise_ontology
        .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
            left.iri().clone(),
            right.iri().clone(),
        ]))
        .unwrap();
    let conclusion = Arc::new(subclass_ontology(&left, &right));

    let response = evaluate_entailment(
        Arc::new(premise_ontology),
        conclusion,
        "premise.owl",
        "conclusion.owl",
    );

    // The premise gate is relaxed (#112): the consistency lane decides,
    // and a conclusion the premise states verbatim is entailed.
    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn entailment_evaluation_decides_equivalence_transitivity() {
    let a = Class::new("http://example.org/A");
    let b = Class::new("http://example.org/B");
    let c = Class::new("http://example.org/C");
    let mut premise_ontology = Ontology::new();
    premise_ontology.add_class(a.clone()).unwrap();
    premise_ontology.add_class(b.clone()).unwrap();
    premise_ontology.add_class(c.clone()).unwrap();
    premise_ontology
        .add_equivalent_classes_axiom(EquivalentClassesAxiom::new(vec![
            a.iri().clone(),
            b.iri().clone(),
        ]))
        .unwrap();
    premise_ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(b.clone()),
            ClassExpression::Class(c.clone()),
        ))
        .unwrap();

    let conclusion = Arc::new(subclass_ontology(&a, &c));
    let response = evaluate_entailment(
        Arc::new(premise_ontology),
        conclusion,
        "premise.owl",
        "conclusion.owl",
    );

    // The witness reduction walks A ⊑ B ⊑ C onto the A ⊓ ¬C witness,
    // so the transitive subsumption is now decidable (#112).
    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "true");
}

#[test]
fn evaluate_cli_emits_machine_readable_entailment_json() {
    let root = env!("CARGO_MANIFEST_DIR");
    let premise = format!("{root}/tests/data/evaluation_premise.owl");
    let conclusion = format!("{root}/tests/data/evaluation_conclusion.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "entailment", &premise, &conclusion])
        .output()
        .expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["status"], "not_evaluated");
    assert_eq!(response["operation"], "entailment");
    assert_eq!(response["semantic_result"], "true");
    assert_eq!(response["supported_scope"], "owl2_entailment_consistency_reduction");
}

#[test]
fn evaluate_cli_applies_and_reports_effective_consistency_config() {
    let root = env!("CARGO_MANIFEST_DIR");
    let input = format!("{root}/tests/data/evaluation_premise.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "consistency", &input])
        .env("SPACL_SCHED_MODE", "sequential")
        .output()
        .expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["status"], "not_evaluated");
    assert_eq!(response["execution_status"], "success");
    assert_eq!(
        response["runtime_metadata"]["effective_config"]["scheduling_mode"],
        "Sequential"
    );
}

#[test]
fn evaluate_cli_rejects_unknown_operation_before_loading_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "unknown", "/path/that/does/not/exist.owl"])
        .output()
        .expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["status"], "unsupported");
    assert_eq!(response["semantic_result"], "unknown");
}

#[test]
fn evaluate_cli_rejects_invalid_entailment_arity_before_loading_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .args(["evaluate", "entailment", "/path/that/does/not/exist.owl"])
        .output()
        .expect("evaluate CLI should start");

    // #174: a structured failure now also exits nonzero — status alone
    // can detect it; the response still explains why.
    assert!(!output.status.success(), "CLI should fail: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    assert_eq!(response["status"], "failed");
    assert!(response["error"]
        .as_str()
        .unwrap_or_default()
        .contains("entailment expects 2 argument(s)"));
}

mod entailment_kernel {
    //! #112: entailment as consistency reduction — premise ∪ {negated
    //! conclusion} inconsistent ⇔ entailed. Anchors: W3C bnode2somevaluesfrom,
    //! WebOnt-cardinality-001, rdfbased-sem-eqdis-sameas-*, WebOnt-I5.24-004.

    use super::*;
    use owl2_reasoner::{
        DifferentIndividualsAxiom, NamedIndividual, ObjectProperty, PropertyAssertionAxiom,
        SameIndividualAxiom, SubClassOfAxiom,
    };

    fn named(suffix: &str) -> String {
        format!("http://example.org/{suffix}")
    }

    fn individual(suffix: &str) -> NamedIndividual {
        NamedIndividual::new(named(suffix))
    }

    #[test]
    fn class_assertion_entailment_via_negated_assertion() {
        // premise: a ∈ C ⊓ D; conclusion: a ∈ C — premise ∪ {a: ¬C} clashes.
        let mut premise = Ontology::new();
        let a = individual("a");
        premise.add_named_individual(a.clone()).unwrap();
        premise
            .add_class_assertion(owl2_reasoner::ClassAssertionAxiom::new(
                a.iri().clone(),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::Class(Class::new(named("C")))),
                    Box::new(ClassExpression::Class(Class::new(named("D")))),
                ]),
            ))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion.add_named_individual(a.clone()).unwrap();
        conclusion
            .add_class_assertion(owl2_reasoner::ClassAssertionAxiom::new(
                a.iri().clone(),
                ClassExpression::Class(Class::new(named("C"))),
            ))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }

    #[test]
    fn unentailed_class_assertion_answers_false() {
        // premise: a ∈ C; conclusion: a ∈ D — the negated conclusion stays
        // consistent, so the entailment is genuinely absent.
        let mut premise = Ontology::new();
        let a = individual("a");
        premise.add_named_individual(a.clone()).unwrap();
        premise
            .add_class_assertion(owl2_reasoner::ClassAssertionAxiom::new(
                a.iri().clone(),
                ClassExpression::Class(Class::new(named("C"))),
            ))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion.add_named_individual(a.clone()).unwrap();
        conclusion
            .add_class_assertion(owl2_reasoner::ClassAssertionAxiom::new(
                a.iri().clone(),
                ClassExpression::Class(Class::new(named("D"))),
            ))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "false");
    }

    #[test]
    fn subclass_entailment_via_witness_negation() {
        // premise ⊑ D; conclusion C ⊑ D — witness w: C ⊓ ¬D clashes.
        let mut premise = Ontology::new();
        premise
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(Class::new(named("C"))),
                ClassExpression::Class(Class::new(named("D"))),
            ))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(Class::new(named("C"))),
                ClassExpression::Class(Class::new(named("D"))),
            ))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }

    #[test]
    fn same_individual_entailment_via_different_from_negation() {
        let mut premise = Ontology::new();
        let a = individual("a");
        let b = individual("b");
        premise.add_named_individual(a.clone()).unwrap();
        premise.add_named_individual(b.clone()).unwrap();
        premise
            .add_axiom(owl2_reasoner::Axiom::SameIndividual(Box::new(
                SameIndividualAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
            )))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion
            .add_axiom(owl2_reasoner::Axiom::SameIndividual(Box::new(
                SameIndividualAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
            )))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }

    #[test]
    fn different_individuals_entailment_via_same_individual_negation() {
        let mut premise = Ontology::new();
        let a = individual("a");
        let b = individual("b");
        premise.add_named_individual(a.clone()).unwrap();
        premise.add_named_individual(b.clone()).unwrap();
        premise
            .add_axiom(owl2_reasoner::Axiom::DifferentIndividuals(Box::new(
                DifferentIndividualsAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
            )))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion
            .add_axiom(owl2_reasoner::Axiom::DifferentIndividuals(Box::new(
                DifferentIndividualsAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
            )))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }

    #[test]
    fn property_assertion_entailment_via_negated_assertion() {
        let mut premise = Ontology::new();
        let a = individual("a");
        let b = individual("b");
        premise.add_named_individual(a.clone()).unwrap();
        premise.add_named_individual(b.clone()).unwrap();
        let p = ObjectProperty::new(named("p"));
        premise
            .add_property_assertion(PropertyAssertionAxiom::new(
                a.iri().clone(),
                p.iri().clone(),
                b.iri().clone(),
            ))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion
            .add_property_assertion(PropertyAssertionAxiom::new(
                a.iri().clone(),
                p.iri().clone(),
                b.iri().clone(),
            ))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }

    #[test]
    fn unentailed_property_assertion_answers_false() {
        let mut premise = Ontology::new();
        let a = individual("a");
        let b = individual("b");
        premise.add_named_individual(a.clone()).unwrap();
        premise.add_named_individual(b.clone()).unwrap();
        let mut conclusion = Ontology::new();
        conclusion.add_named_individual(a.clone()).unwrap();
        conclusion.add_named_individual(b.clone()).unwrap();
        let p = ObjectProperty::new(named("p"));
        conclusion
            .add_property_assertion(PropertyAssertionAxiom::new(
                a.iri().clone(),
                p.iri().clone(),
                b.iri().clone(),
            ))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "false");
    }

    #[test]
    fn disjoint_classes_entailment_via_intersection_witness() {
        // premise: C disjoint D; conclusion: same — witness w: C ⊓ D clashes.
        let mut premise = Ontology::new();
        premise
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("C")).unwrap()),
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("D")).unwrap()),
            ]))
            .unwrap();
        let mut conclusion = Ontology::new();
        conclusion
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("C")).unwrap()),
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("D")).unwrap()),
            ]))
            .unwrap();

        let response = evaluate_entailment(
            Arc::new(premise),
            Arc::new(conclusion),
            "premise.owl",
            "conclusion.owl",
        );
        assert_eq!(response.status.as_str(), "not_evaluated");
        assert_eq!(response.semantic_result.as_str(), "true");
    }
}

mod entailment_kernel_soundness {
    //! Regression guards for the two soundness holes the #112 review
    //! found (verified unsound before the fix).

    use super::*;
    use owl2_reasoner::{
        Axiom, Class, ClassAssertionAxiom, InverseObjectPropertiesAxiom, NamedIndividual,
        NegativeObjectPropertyAssertionAxiom, ObjectProperty, PropertyAssertionAxiom,
        SubClassOfAxiom, SubObjectPropertyOfAxiom,
    };

    fn named(suffix: &str) -> String {
        format!("http://example.org/{suffix}")
    }

    #[test]
    fn inverse_partner_edge_keeps_forward_negated_existential_consistent() {
        // a in not-(exists p.C) and exists partner.C, with p inverseOf
        // partner: the partner-successor is a p-predecessor, which the
        // negated forward existential does not constrain.
        let mut ontology = Ontology::new();
        let a = NamedIndividual::new(named("a"));
        ontology.add_named_individual(a.clone()).unwrap();
        ontology
            .add_axiom(Axiom::InverseObjectProperties(Box::new(
                InverseObjectPropertiesAxiom::new(
                    std::sync::Arc::new(owl2_reasoner::IRI::new(named("p")).unwrap()),
                    std::sync::Arc::new(owl2_reasoner::IRI::new(named("partner")).unwrap()),
                ),
            )))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                a.iri().clone(),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                                owl2_reasoner::ObjectProperty::new(named("p")),
                            ))),
                            Box::new(ClassExpression::Class(Class::new(named("C")))),
                        ),
                    ))),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                            owl2_reasoner::ObjectProperty::new(named("partner")),
                        ))),
                        Box::new(ClassExpression::Class(Class::new(named("C")))),
                    )),
                ]),
            ))
            .unwrap();

        let response = evaluate_consistency(Arc::new(ontology), "soundness.owl");
        assert_eq!(response.semantic_result.as_str(), "consistent");
    }

    #[test]
    fn subproperty_positive_contradicts_negated_super_only_in_sub_direction() {
        // p ⊑ q: p(a,b) contradicts ¬q(a,b) ...
        let mut ontology = Ontology::new();
        let a = NamedIndividual::new(named("a"));
        let b = NamedIndividual::new(named("b"));
        ontology.add_named_individual(a.clone()).unwrap();
        ontology.add_named_individual(b.clone()).unwrap();
        ontology
            .add_axiom(Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("p")).unwrap()),
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("q")).unwrap()),
            ))))
            .unwrap();
        let p = ObjectProperty::new(named("p"));
        let q = ObjectProperty::new(named("q"));
        ontology
            .add_property_assertion(PropertyAssertionAxiom::new(
                a.iri().clone(),
                p.iri().clone(),
                b.iri().clone(),
            ))
            .unwrap();
        ontology
            .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                NegativeObjectPropertyAssertionAxiom::new(
                    a.iri().clone(),
                    q.iri().clone(),
                    b.iri().clone(),
                ),
            )))
            .unwrap();

        let response = evaluate_inconsistency(Arc::new(ontology), "sub-direction.owl");
        assert_eq!(response.semantic_result.as_str(), "true");

        // ... but q(a,b) does NOT contradict ¬p(a,b).
        let mut ontology = Ontology::new();
        ontology.add_named_individual(a.clone()).unwrap();
        ontology.add_named_individual(b.clone()).unwrap();
        ontology
            .add_axiom(Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("p")).unwrap()),
                std::sync::Arc::new(owl2_reasoner::IRI::new(named("q")).unwrap()),
            ))))
            .unwrap();
        ontology
            .add_property_assertion(PropertyAssertionAxiom::new(
                a.iri().clone(),
                q.iri().clone(),
                b.iri().clone(),
            ))
            .unwrap();
        ontology
            .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                NegativeObjectPropertyAssertionAxiom::new(
                    a.iri().clone(),
                    p.iri().clone(),
                    b.iri().clone(),
                ),
            )))
            .unwrap();

        let response = evaluate_inconsistency(Arc::new(ontology), "super-direction.owl");
        assert_eq!(response.semantic_result.as_str(), "false");
    }
}

#[test]
fn mixed_targeted_fragment_falls_through_instead_of_unsupported() {
    // #113: an ontology mixing a targeted construct (qualified
    // cardinality) with an ordinary TBox used to abort the whole
    // evaluation as unsupported; the bounded checker declining its
    // fragment must fall through to the next consistency layer
    // (W3C 0007/0098/0400/0475/0487/0488/0497 drifted this way).
    let mut ontology = Ontology::new();
    let a = Class::new("http://example.org/A");
    let b = Class::new("http://example.org/B");
    ontology.add_class(a.clone()).unwrap();
    ontology.add_class(b.clone()).unwrap();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(a),
            ClassExpression::Class(b),
        ))
        .unwrap();
    let individual = owl2_reasoner::NamedIndividual::new("http://example.org/x");
    ontology
        .add_named_individual(individual.clone())
        .unwrap();
    ontology
        .add_class_assertion(ClassAssertionAxiom::new(
            individual.iri().clone(),
            ClassExpression::ObjectMaxCardinality(
                1,
                Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                    owl2_reasoner::ObjectProperty::new("http://example.org/p"),
                ))),
                Some(Box::new(ClassExpression::Class(Class::new(
                    "http://example.org/B",
                )))),
            ),
        ))
        .unwrap();

    let response = evaluate_consistency(Arc::new(ontology), "mixed.owl");
    assert_ne!(response.status.as_str(), "unsupported");
    assert_eq!(response.semantic_result.as_str(), "consistent");
}

#[test]
fn rdf_rest_bare_namespace_terminates_the_list() {
    // W3C New-Feature-Rational-002/003 spell rdf:nil's IRI without the
    // local name; the r13 parser tolerated it, and the intended
    // semantics (0.5 = 1/2, so the oneOf holds one value under min 2)
    // makes the premise inconsistent (#113).
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/w3c_rdf_rest_bare_namespace_terminates_list.owl");
    let ontology =
        owl2_reasoner::util::ontology_io::load_ontology_with_env(Path::new(&path))
            .expect("bare-namespace rdf:rest fixture should parse");

    let response = evaluate_inconsistency(Arc::new(ontology), &path);
    assert_eq!(response.semantic_result.as_str(), "true");
}
