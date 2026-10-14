//! Contract tests for the EL/QL/RL profile validators (#167): the
//! validators must reject constructs the OWL 2 profile specifications
//! exclude, so the reasoner router stops labeling every ontology as
//! the most restrictive profile (#166 Layer 1).

use std::sync::Arc;

use owl2_reasoner::core::entities::Entity;
use owl2_reasoner::core::iri::IRI;
use owl2_reasoner::core::ontology::Ontology;
use owl2_reasoner::logic::axioms::{
    Axiom, ClassExpression, InverseFunctionalPropertyAxiom, InverseObjectPropertiesAxiom,
    SubClassOfAxiom, SubPropertyChainOfAxiom, TransitivePropertyAxiom,
};
use owl2_reasoner::logic::axioms::property_expressions::ObjectPropertyExpression;
use owl2_reasoner::strategy::profiles::{Owl2Profile, Owl2ProfileValidator};

fn iri(value: &str) -> IRI {
    IRI::new(value).expect("test IRI is valid")
}

fn class(local: &str) -> ClassExpression {
    ClassExpression::Class(owl2_reasoner::core::entities::Class::new(iri(local)))
}

fn ontology_with(axioms: Vec<Axiom>) -> Arc<Ontology> {
    let mut ontology = Ontology::new();
    for axiom in axioms {
        ontology
            .add_axiom(axiom)
            .expect("test axiom is accepted by the ontology");
    }
    Arc::new(ontology)
}

fn inverse_pair(left: &str, right: &str) -> Axiom {
    Axiom::InverseObjectProperties(Box::new(InverseObjectPropertiesAxiom::new(
        Arc::new(iri(left)),
        Arc::new(iri(right)),
    )))
}

fn chain(super_property: &str, members: &[&str]) -> Axiom {
    Axiom::SubPropertyChainOf(Box::new(SubPropertyChainOfAxiom::new(
        members.iter().map(|m| Arc::new(iri(m))).collect(),
        Arc::new(iri(super_property)),
    )))
}

fn validates(ontology: &Arc<Ontology>, profile: Owl2Profile) -> bool {
    let validator = Owl2ProfileValidator::new(Arc::clone(ontology)).expect("validator builds");
    validator
        .validate_profile(profile)
        .expect("validation runs")
        .is_valid
}

/// The pure-EL core: atomic subsumption, existential restriction on a
/// named role, transitive role — all inside OWL 2 EL.
#[test]
fn el_accepts_constructive_core() {
    let role = |local: &str| {
        ObjectPropertyExpression::ObjectProperty(Box::new(
            owl2_reasoner::core::entities::ObjectProperty::new(iri(local)),
        ))
    };
    let ontology = ontology_with(vec![
        Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
            class("http://example.org/a"),
            ClassExpression::ObjectSomeValuesFrom(
                Box::new(role("http://example.org/r")),
                Box::new(class("http://example.org/b")),
            ),
        ))),
        Axiom::TransitiveProperty(Box::new(TransitivePropertyAxiom::new(Arc::new(iri(
            "http://example.org/r",
        ))))),
    ]);
    assert!(
        validates(&ontology, Owl2Profile::EL),
        "atomic subsumption + someValuesFrom + transitivity are in EL"
    );
}

#[test]
fn el_rejects_inverse_property_declarations() {
    let ontology = ontology_with(vec![inverse_pair(
        "http://example.org/p",
        "http://example.org/q",
    )]);
    assert!(
        !validates(&ontology, Owl2Profile::EL),
        "owl:inverseOf declarations are outside OWL 2 EL"
    );
}

#[test]
fn el_rejects_property_chains() {
    let ontology = ontology_with(vec![chain(
        "http://example.org/s",
        &["http://example.org/p", "http://example.org/q"],
    )]);
    assert!(
        !validates(&ontology, Owl2Profile::EL),
        "property chains are outside OWL 2 EL"
    );
}

#[test]
fn el_rejects_union_in_superclass() {
    let union = ClassExpression::ObjectUnionOf(vec![
        Box::new(class("http://example.org/b")),
        Box::new(class("http://example.org/c")),
    ]
    .into());
    let ontology = ontology_with(vec![Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
        class("http://example.org/a"),
        union,
    )))]);
    assert!(
        !validates(&ontology, Owl2Profile::EL),
        "ObjectUnionOf is outside OWL 2 EL"
    );
}

#[test]
fn ql_rejects_property_chains() {
    let ontology = ontology_with(vec![chain(
        "http://example.org/s",
        &["http://example.org/p", "http://example.org/q"],
    )]);
    assert!(
        !validates(&ontology, Owl2Profile::QL),
        "property chains are outside OWL 2 QL"
    );
}

#[test]
fn ql_rejects_inverse_functional_roles() {
    let ontology = ontology_with(vec![Axiom::InverseFunctionalProperty(Box::new(
        InverseFunctionalPropertyAxiom::new(Arc::new(iri("http://example.org/p"))),
    ))]);
    assert!(
        !validates(&ontology, Owl2Profile::QL),
        "inverse-functional roles are outside OWL 2 QL"
    );
}

#[test]
fn rl_accepts_property_chains() {
    // prp-spo2 is an RL rule: chains are legal OWL 2 RL (this repo's
    // own closure fragment composes them under the RL banner).
    let ontology = ontology_with(vec![chain(
        "http://example.org/s",
        &["http://example.org/p", "http://example.org/q"],
    )]);
    assert!(
        validates(&ontology, Owl2Profile::RL),
        "property chains are inside OWL 2 RL"
    );
}

#[test]
fn rl_accepts_inverse_functional_roles() {
    // prp-ifp is an RL rule: inverse-functionality is legal OWL 2 RL.
    let ontology = ontology_with(vec![Axiom::InverseFunctionalProperty(Box::new(
        InverseFunctionalPropertyAxiom::new(Arc::new(iri("http://example.org/p"))),
    ))]);
    assert!(
        validates(&ontology, Owl2Profile::RL),
        "inverse-functional roles are inside OWL 2 RL"
    );
}

#[test]
fn rl_rejects_super_side_existential() {
    // A rule head cannot introduce a witness: existentials on the
    // superclass side are outside OWL 2 RL.
    let some_values_from = |filler: &str| {
        ClassExpression::ObjectSomeValuesFrom(
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                owl2_reasoner::core::entities::ObjectProperty::new(iri("http://example.org/r")),
            ))),
            Box::new(class(filler)),
        )
    };
    let ontology = ontology_with(vec![Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
        class("http://example.org/a"),
        some_values_from("http://example.org/b"),
    )))]);
    assert!(
        !validates(&ontology, Owl2Profile::RL),
        "superclass-side ObjectSomeValuesFrom is outside OWL 2 RL"
    );
}

#[test]
fn ql_rejects_transitive_roles() {
    let ontology = ontology_with(vec![Axiom::TransitiveProperty(Box::new(
        TransitivePropertyAxiom::new(Arc::new(iri("http://example.org/r"))),
    ))]);
    assert!(
        !validates(&ontology, Owl2Profile::QL),
        "transitive roles are outside OWL 2 QL"
    );
}

#[test]
fn ql_rejects_super_side_existential() {
    let some_values_from = |filler: &str| {
        ClassExpression::ObjectSomeValuesFrom(
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                owl2_reasoner::core::entities::ObjectProperty::new(iri("http://example.org/r")),
            ))),
            Box::new(class(filler)),
        )
    };
    let ontology = ontology_with(vec![Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
        some_values_from("http://example.org/b"),
        class("http://example.org/a"),
    )))]);
    assert!(
        validates(&ontology, Owl2Profile::QL),
        "subclass-side ObjectSomeValuesFrom is inside OWL 2 QL"
    );
    let flipped = ontology_with(vec![Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
        class("http://example.org/a"),
        some_values_from("http://example.org/b"),
    )))]);
    assert!(
        !validates(&flipped, Owl2Profile::QL),
        "superclass-side ObjectSomeValuesFrom is outside OWL 2 QL"
    );
}

#[test]
fn ql_accepts_inverse_property_declarations() {
    // QL (DL-Lite^R) carries inverse roles.
    let ontology = ontology_with(vec![inverse_pair(
        "http://example.org/p",
        "http://example.org/q",
    )]);
    assert!(
        validates(&ontology, Owl2Profile::QL),
        "owl:inverseOf declarations are inside OWL 2 QL"
    );
}

/// The #166 shape: uberon declares inverse properties and property
/// chains, and its TBox leans on superclass-side restrictions. Inverse
/// declarations alone knock out EL; chains knock out QL; the
/// superclass-side restriction knocks out RL — so no restricted profile
/// applies and the router must see OWL 2 DL (Full), not a fabricated EL.
#[test]
fn uberon_shaped_ontology_is_not_el_ql_or_rl() {
    let restriction = Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
        class("http://example.org/a"),
        ClassExpression::ObjectSomeValuesFrom(
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                owl2_reasoner::core::entities::ObjectProperty::new(iri("http://example.org/r")),
            ))),
            Box::new(class("http://example.org/b")),
        ),
    )));
    let ontology = ontology_with(vec![
        inverse_pair("http://example.org/p", "http://example.org/q"),
        chain(
            "http://example.org/s",
            &["http://example.org/p", "http://example.org/q"],
        ),
        restriction,
    ]);
    let validator = Owl2ProfileValidator::new(Arc::clone(&ontology)).expect("validator builds");
    assert_eq!(
        validator
            .get_most_restrictive_profile()
            .expect("most restrictive profile resolves"),
        None,
        "the inverse + chain + super-side restriction shape must fall through to OWL 2 DL"
    );
}
