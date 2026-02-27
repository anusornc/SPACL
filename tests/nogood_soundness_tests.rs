//! Nogood Learning Soundness Tests
//!
//! These tests verify that the nogood learning in speculative reasoning
//! is SOUND (no false positives) even if not MINIMAL.

use owl2_reasoner::core::entities::{Class, NamedIndividual};
use owl2_reasoner::core::iri::IRI;
use owl2_reasoner::core::ontology::Ontology;
use owl2_reasoner::logic::axioms::{ClassAssertionAxiom, ClassExpression, SubClassOfAxiom};
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::reasoner::speculative::SpeculativeTableauxReasoner;
use smallvec::smallvec;

/// Helper: Create a simple class with IRI
fn class(iri: &str) -> Class {
    Class::new(IRI::new(iri).unwrap())
}

/// Helper: Create ClassExpression from IRI
fn class_expr(iri: &str) -> ClassExpression {
    ClassExpression::Class(class(iri))
}

/// Test 1: Simple direct contradiction
/// Ontology: A ⊑ ⊥
/// When reasoning with A, should detect contradiction
/// Nogood should contain A
#[test]
fn test_nogood_simple_contradiction() {
    let mut ontology = Ontology::new();

    // Create classes
    let class_a = class("http://test.org/A");
    let nothing = Class::owl_nothing();

    // Add axiom: A ⊑ ⊥
    let axiom = SubClassOfAxiom::new(
        ClassExpression::Class(class_a.clone()),
        ClassExpression::Class(nothing),
    );
    ontology.add_axiom(axiom.into()).unwrap();

    // Run speculative reasoner
    let simple_result = SimpleReasoner::new(ontology.clone()).is_consistent();
    let speculative_result = SpeculativeTableauxReasoner::new(ontology).is_consistent();

    assert!(simple_result.is_ok());
    assert!(speculative_result.is_ok());
    assert_eq!(
        simple_result.unwrap(),
        speculative_result.unwrap(),
        "SpeculativeReasoner should agree with SimpleReasoner"
    );
}

/// Test 2: Chain contradiction
/// Ontology: A ⊑ B, B ⊑ C, C ⊑ ⊥
/// When reasoning, should detect contradiction
/// Nogood should be sound (may contain A, B, C)
#[test]
fn test_nogood_chain_contradiction() {
    let mut ontology = Ontology::new();

    // A ⊑ B
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/A"),
                class_expr("http://test.org/B"),
            )
            .into(),
        )
        .unwrap();

    // B ⊑ C
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/B"),
                class_expr("http://test.org/C"),
            )
            .into(),
        )
        .unwrap();

    // C ⊑ ⊥
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/C"),
                ClassExpression::Class(Class::owl_nothing()),
            )
            .into(),
        )
        .unwrap();

    // Add assertion that triggers reasoning
    let individual = NamedIndividual::new("http://test.org/ind1");
    let assertion =
        ClassAssertionAxiom::new(individual.iri().clone(), class_expr("http://test.org/A"));
    ontology.add_axiom(assertion.into()).unwrap();

    // Run reasoner
    let simple_result = SimpleReasoner::new(ontology.clone()).is_consistent();
    let speculative_result = SpeculativeTableauxReasoner::new(ontology).is_consistent();

    assert!(simple_result.is_ok());
    assert!(speculative_result.is_ok());
    assert_eq!(
        simple_result.unwrap(),
        speculative_result.unwrap(),
        "SpeculativeReasoner should agree with SimpleReasoner"
    );
}

/// Test 3: Verify nogood pruning doesn't cause false positives
/// Create satisfiable ontology, verify not pruned incorrectly
#[test]
fn test_nogood_no_false_positives() {
    let mut ontology = Ontology::new();

    // Simple satisfiable hierarchy: A ⊑ B, B ⊑ C
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/A"),
                class_expr("http://test.org/B"),
            )
            .into(),
        )
        .unwrap();

    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/B"),
                class_expr("http://test.org/C"),
            )
            .into(),
        )
        .unwrap();

    // Assert A
    let individual = NamedIndividual::new("http://test.org/ind1");
    let assertion =
        ClassAssertionAxiom::new(individual.iri().clone(), class_expr("http://test.org/A"));
    ontology.add_axiom(assertion.into()).unwrap();

    // Run reasoner
    let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
    let result = reasoner.is_consistent();

    // Should be consistent
    assert!(result.is_ok());
    assert!(
        result.unwrap(),
        "Satisfiable hierarchy should be consistent"
    );
}

/// Test 4: Disjunction with contradiction in both branches
/// Ontology: (A ⊔ B), ¬A, ¬B
/// Both branches should lead to contradiction
#[test]
fn test_nogood_disjunction_contradiction() {
    let mut ontology = Ontology::new();

    // Create union A ⊔ B
    let union = ClassExpression::ObjectUnionOf(smallvec![
        Box::new(class_expr("http://test.org/A")),
        Box::new(class_expr("http://test.org/B")),
    ]);

    // Add complement constraints via SubClassOf
    // A ⊑ ⊥ (equivalent to ¬A)
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/A"),
                ClassExpression::Class(Class::owl_nothing()),
            )
            .into(),
        )
        .unwrap();

    // B ⊑ ⊥ (equivalent to ¬B)
    ontology
        .add_axiom(
            SubClassOfAxiom::new(
                class_expr("http://test.org/B"),
                ClassExpression::Class(Class::owl_nothing()),
            )
            .into(),
        )
        .unwrap();

    // Assert (A ⊔ B) for some individual
    let individual = NamedIndividual::new("http://test.org/ind1");
    let assertion = ClassAssertionAxiom::new(individual.iri().clone(), union);
    ontology.add_axiom(assertion.into()).unwrap();

    // Run reasoner
    let simple_result = SimpleReasoner::new(ontology.clone()).is_consistent();
    let speculative_result = SpeculativeTableauxReasoner::new(ontology).is_consistent();

    assert!(simple_result.is_ok());
    assert!(speculative_result.is_ok());
    assert_eq!(
        simple_result.unwrap(),
        speculative_result.unwrap(),
        "SpeculativeReasoner should agree with SimpleReasoner"
    );
}

/// Test 5: Agreement with SimpleReasoner (correctness oracle)
/// For any ontology, SpeculativeReasoner should agree with SimpleReasoner
#[test]
fn test_speculative_agrees_with_simple() {
    let test_cases = vec![
        // Case 1: Inconsistent
        {
            let mut ont = Ontology::new();
            ont.add_axiom(
                SubClassOfAxiom::new(
                    class_expr("http://test.org/A"),
                    ClassExpression::Class(Class::owl_nothing()),
                )
                .into(),
            )
            .unwrap();
            let ind = NamedIndividual::new("http://test.org/ind");
            ont.add_axiom(
                ClassAssertionAxiom::new(ind.iri().clone(), class_expr("http://test.org/A")).into(),
            )
            .unwrap();
            ont
        },
        // Case 2: Consistent
        {
            let mut ont = Ontology::new();
            ont.add_axiom(
                SubClassOfAxiom::new(
                    class_expr("http://test.org/A"),
                    class_expr("http://test.org/B"),
                )
                .into(),
            )
            .unwrap();
            ont
        },
        // Case 3: Chain consistent
        {
            let mut ont = Ontology::new();
            ont.add_axiom(
                SubClassOfAxiom::new(
                    class_expr("http://test.org/A"),
                    class_expr("http://test.org/B"),
                )
                .into(),
            )
            .unwrap();
            ont.add_axiom(
                SubClassOfAxiom::new(
                    class_expr("http://test.org/B"),
                    class_expr("http://test.org/C"),
                )
                .into(),
            )
            .unwrap();
            ont
        },
    ];

    for (i, ontology) in test_cases.into_iter().enumerate() {
        let simple_result = SimpleReasoner::new(ontology.clone()).is_consistent();
        let speculative_result = SpeculativeTableauxReasoner::new(ontology).is_consistent();

        assert!(simple_result.is_ok(), "Case {}: SimpleReasoner failed", i);
        assert!(
            speculative_result.is_ok(),
            "Case {}: SpeculativeReasoner failed",
            i
        );

        assert_eq!(
            simple_result.unwrap(),
            speculative_result.unwrap(),
            "Case {}: Reasoners disagree!",
            i
        );
    }
}

/// Test 6: Empty ontology should be consistent
#[test]
fn test_empty_ontology_consistent() {
    let ontology = Ontology::new();
    let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
    let result = reasoner.is_consistent();

    assert!(result.is_ok());
    assert!(result.unwrap(), "Empty ontology should be consistent");
}

/// Test 7: Single assertion should be consistent (unless contradictory)
#[test]
fn test_single_assertion() {
    let mut ontology = Ontology::new();

    let individual = NamedIndividual::new("http://test.org/ind1");
    let class_a = class("http://test.org/A");

    let assertion =
        ClassAssertionAxiom::new(individual.iri().clone(), ClassExpression::Class(class_a));
    ontology.add_axiom(assertion.into()).unwrap();

    let mut reasoner = SpeculativeTableauxReasoner::new(ontology);
    let result = reasoner.is_consistent();

    assert!(result.is_ok());
    assert!(result.unwrap(), "Single assertion should be consistent");
}
