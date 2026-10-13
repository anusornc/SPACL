use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use owl2_reasoner::logic::axioms::class_expressions::DataRange;
use owl2_reasoner::serializer::BinaryOntologyFormat;
use owl2_reasoner::{
    Axiom, Class, ClassExpression, DataProperty, DataPropertyExpression, ObjectProperty,
    ObjectPropertyExpression, Ontology, SubClassOfAxiom, IRI,
};
use smallvec::smallvec;

static BIN_FORMAT_ENV_LOCK: Mutex<()> = Mutex::new(());

struct BinFormatEnvGuard<'a> {
    _lock: MutexGuard<'a, ()>,
    previous: Option<String>,
}

impl BinFormatEnvGuard<'_> {
    fn set(value: Option<&str>) -> Self {
        let lock = BIN_FORMAT_ENV_LOCK.lock().unwrap();
        let previous = std::env::var("OWL2_REASONER_BIN_FORMAT").ok();
        match value {
            Some(value) => std::env::set_var("OWL2_REASONER_BIN_FORMAT", value),
            None => std::env::remove_var("OWL2_REASONER_BIN_FORMAT"),
        }
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for BinFormatEnvGuard<'_> {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("OWL2_REASONER_BIN_FORMAT", value),
            None => std::env::remove_var("OWL2_REASONER_BIN_FORMAT"),
        }
    }
}

#[test]
fn legacy_fixture_hashes_match_recorded_provenance() {
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("data/qualified_binary_provenance.json")).unwrap();
    assert_eq!(provenance["source_commit"], "c0f6ef4");
    assert_eq!(
        provenance["release_binary_sha256"],
        "5e3d2a9407cbe5f079c3ba1ef4cc3a4293505ff88750b48eab7b85effffedbf5"
    );

    for fixture in provenance["fixtures"].as_array().unwrap() {
        let input = fixture["input"].as_str().unwrap();
        let output = fixture["output"].as_str().unwrap();
        assert!(!input.starts_with('/'), "input path must be repo-relative");
        assert!(
            !output.starts_with('/'),
            "output path must be repo-relative"
        );
        assert!(!input.contains("/tmp/"));
        assert!(!output.contains("/tmp/"));

        assert_eq!(sha256sum(input), fixture["input_sha256"].as_str().unwrap());
        assert_eq!(
            sha256sum(output),
            fixture["output_sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn default_binary_v4_roundtrips_nested_object_and_data_qualified_cardinality() {
    let _env = BinFormatEnvGuard::set(None);
    let ontology = qualified_cardinality_ontology();
    let mut bytes = Vec::new();

    BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap();

    assert_eq!(&bytes[0..4], b"OWLB");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 4);

    let mut reader = Cursor::new(bytes);
    let decoded = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

    let object_min = class_expressions(&decoded)
        .into_iter()
        .find_map(|expr| match expr {
            ClassExpression::ObjectMinCardinality(cardinality, property, Some(filler)) => {
                Some((*cardinality, property.as_ref(), filler.as_ref()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(object_min.0, 2);
    assert_eq!(
        object_property_iri(object_min.1),
        Some(iri("hasVisit").as_str())
    );
    let object_exact = match object_min.2 {
        ClassExpression::ObjectExactCardinality(cardinality, property, Some(filler)) => {
            (*cardinality, property.as_ref(), filler.as_ref())
        }
        other => panic!("unexpected object cardinality filler: {other:?}"),
    };
    assert_eq!(object_exact.0, 1);
    assert_eq!(
        object_property_iri(object_exact.1),
        Some(iri("hasAttendant").as_str())
    );
    assert!(
        matches!(object_exact.2, ClassExpression::Class(class) if class.iri().as_str() == iri("Doctor"))
    );

    let data_max = class_expressions(&decoded)
        .into_iter()
        .find_map(|expr| match expr {
            ClassExpression::DataMaxCardinality(cardinality, property, Some(range)) => {
                Some((*cardinality, property.as_ref(), range.as_ref()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(data_max.0, 1);
    assert_eq!(data_property_iri(data_max.1), Some(iri("age").as_str()));
    assert!(
        matches!(data_max.2, DataRange::Datatype(iri) if iri.as_str() == self::iri("adultAge"))
    );
}

#[test]
fn forced_legacy_write_rejects_qualified_cardinality_before_emitting_partial_bytes() {
    for version in ["v1", "v2", "v3"] {
        let _env = BinFormatEnvGuard::set(Some(version));
        let ontology = qualified_cardinality_ontology();
        let mut bytes = Vec::new();

        let err = BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("qualified cardinality"),
            "unexpected error for {version}: {err}"
        );
        assert!(bytes.is_empty(), "{version} wrote partial legacy bytes");
    }
}

#[test]
fn legacy_v2_and_v3_cardinality_fixtures_read_as_unqualified_expressions() {
    for bytes in [
        include_bytes!("data/qualified_binary_legacy_v2_cardinality.bin").as_slice(),
        include_bytes!("data/qualified_binary_legacy_v3_cardinality.bin").as_slice(),
    ] {
        let mut reader = Cursor::new(bytes);
        let decoded = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

        assert_eq!(decoded.classes().len(), 0);
        assert_eq!(decoded.object_properties().len(), 0);
        assert_eq!(decoded.data_properties().len(), 1);
        assert_eq!(decoded.named_individuals().len(), 3);
        assert_eq!(decoded.axioms().len(), 4);
        assert!(decoded
            .data_properties()
            .iter()
            .any(|property| property.iri().as_str().ends_with("dp")));

        let data_min = class_expressions(&decoded)
            .into_iter()
            .find_map(|expr| match expr {
                ClassExpression::DataMinCardinality(cardinality, property, qualifier) => {
                    Some((*cardinality, property.as_ref(), qualifier.as_ref()))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(data_min.0, 2);
        assert_eq!(data_property_iri(data_min.1), Some("http://example.org/dp"));
        assert!(data_min.2.is_none());
    }
}

#[test]
fn forced_legacy_v2_and_v3_writes_roundtrip_unqualified_cardinality_axioms() {
    for version in [("v2", 2), ("v3", 3)] {
        let _env = BinFormatEnvGuard::set(Some(version.0));
        let ontology = unqualified_cardinality_ontology();
        let mut bytes = Vec::new();

        BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap();

        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            version.1
        );
        let mut reader = Cursor::new(bytes);
        let decoded = BinaryOntologyFormat::deserialize(&mut reader).unwrap();
        assert_eq!(decoded.axioms().len(), 2);
        assert_unqualified_object_max_axiom(&decoded);
        assert_unqualified_data_min_axiom(&decoded);
    }
}

#[test]
fn forced_v1_rejects_metadata_before_emitting_header() {
    let _env = BinFormatEnvGuard::set(Some("v1"));
    let ontology = Ontology::with_iri(IRI::new("http://example.org/metadata").unwrap());
    let mut bytes = Vec::new();

    let err = BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap_err();

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("metadata"));
    assert!(bytes.is_empty());
}

#[test]
fn legacy_v1_fixture_remains_readable() {
    let bytes = include_bytes!("data/qualified_binary_legacy_v1.bin");
    let mut reader = Cursor::new(bytes);

    let decoded = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

    assert!(decoded
        .classes()
        .iter()
        .any(|class| { class.iri().as_str().ends_with("qualified-binary-fixture#A") }));
    assert!(decoded.axioms().iter().any(|axiom| {
        matches!(
            axiom.as_ref(),
            Axiom::SubClassOf(sub)
                if matches!(sub.sub_class(), ClassExpression::Class(class) if class.iri().as_str().ends_with("#A"))
                    && matches!(sub.super_class(), ClassExpression::Class(class) if class.iri().as_str().ends_with("#B"))
        )
    }));
}

#[test]
fn unsupported_binary_header_version_is_rejected() {
    let mut data = Vec::new();
    data.extend_from_slice(b"OWLB");
    data.extend_from_slice(&99u32.to_le_bytes());
    data.extend_from_slice(&[0; 40]);

    let mut reader = Cursor::new(data);
    let err = BinaryOntologyFormat::deserialize(&mut reader).unwrap_err();

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("Unsupported version: 99"));
}

fn qualified_cardinality_ontology() -> Ontology {
    let mut ontology = Ontology::new();
    let patient = class("Patient");
    let doctor = class("Doctor");
    let senior_doctor = class("SeniorDoctor");
    let visit = object_property("hasVisit");
    let attendant = object_property("hasAttendant");
    let age = data_property("age");
    let adult_age = datatype("adultAge");

    ontology.add_class(patient.clone()).unwrap();
    ontology.add_class(doctor.clone()).unwrap();
    ontology.add_class(senior_doctor.clone()).unwrap();
    ontology.add_object_property(visit.clone()).unwrap();
    ontology.add_object_property(attendant.clone()).unwrap();
    ontology.add_data_property(age.clone()).unwrap();

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(patient),
            ClassExpression::ObjectIntersectionOf(smallvec![
                Box::new(ClassExpression::ObjectMinCardinality(
                    2,
                    Box::new(ObjectPropertyExpression::from(visit)),
                    Some(Box::new(ClassExpression::ObjectExactCardinality(
                        1,
                        Box::new(ObjectPropertyExpression::from(attendant)),
                        Some(Box::new(ClassExpression::Class(doctor))),
                    ))),
                )),
                Box::new(ClassExpression::DataMaxCardinality(
                    1,
                    Box::new(DataPropertyExpression::from(age)),
                    Some(Box::new(DataRange::Datatype(adult_age))),
                )),
            ]),
        ))
        .unwrap();

    ontology
}

fn unqualified_cardinality_ontology() -> Ontology {
    let mut ontology = Ontology::new();
    let patient = class("Patient");
    let limited_visit_patient = class("LimitedVisitPatient");
    let measured_patient = class("MeasuredPatient");
    let visit = object_property("hasVisit");
    let age = data_property("age");

    ontology.add_class(patient.clone()).unwrap();
    ontology.add_class(limited_visit_patient.clone()).unwrap();
    ontology.add_class(measured_patient.clone()).unwrap();
    ontology.add_object_property(visit.clone()).unwrap();
    ontology.add_data_property(age.clone()).unwrap();

    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(limited_visit_patient),
            ClassExpression::ObjectMaxCardinality(
                3,
                Box::new(ObjectPropertyExpression::from(visit)),
                None,
            ),
        ))
        .unwrap();
    ontology
        .add_subclass_axiom(SubClassOfAxiom::new(
            ClassExpression::Class(measured_patient),
            ClassExpression::DataMinCardinality(
                1,
                Box::new(DataPropertyExpression::from(age)),
                None,
            ),
        ))
        .unwrap();

    ontology
}

fn class_expressions(ontology: &Ontology) -> Vec<&ClassExpression> {
    let mut expressions = Vec::new();
    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::SubClassOf(sub) => {
                push_class_expression_tree(sub.sub_class(), &mut expressions);
                push_class_expression_tree(sub.super_class(), &mut expressions);
            }
            Axiom::ClassAssertion(assertion) => {
                push_class_expression_tree(assertion.class_expr(), &mut expressions);
            }
            _ => {}
        }
    }
    expressions
}

fn push_class_expression_tree<'a>(
    expr: &'a ClassExpression,
    expressions: &mut Vec<&'a ClassExpression>,
) {
    expressions.push(expr);
    match expr {
        ClassExpression::ObjectIntersectionOf(operands)
        | ClassExpression::ObjectUnionOf(operands) => {
            for operand in operands {
                push_class_expression_tree(operand, expressions);
            }
        }
        ClassExpression::ObjectComplementOf(operand) => {
            push_class_expression_tree(operand, expressions)
        }
        ClassExpression::ObjectSomeValuesFrom(_, filler)
        | ClassExpression::ObjectAllValuesFrom(_, filler) => {
            push_class_expression_tree(filler, expressions);
        }
        ClassExpression::ObjectMinCardinality(_, _, Some(filler))
        | ClassExpression::ObjectMaxCardinality(_, _, Some(filler))
        | ClassExpression::ObjectExactCardinality(_, _, Some(filler)) => {
            push_class_expression_tree(filler, expressions);
        }
        ClassExpression::Class(_)
        | ClassExpression::ObjectOneOf(_)
        | ClassExpression::ObjectHasValue(_, _)
        | ClassExpression::ObjectHasSelf(_)
        | ClassExpression::ObjectMinCardinality(_, _, None)
        | ClassExpression::ObjectMaxCardinality(_, _, None)
        | ClassExpression::ObjectExactCardinality(_, _, None)
        | ClassExpression::DataSomeValuesFrom(_, _)
        | ClassExpression::DataAllValuesFrom(_, _)
        | ClassExpression::DataHasValue(_, _)
        | ClassExpression::DataMinCardinality(_, _, _)
        | ClassExpression::DataMaxCardinality(_, _, _)
        | ClassExpression::DataExactCardinality(_, _, _) => {}
    }
}

fn assert_unqualified_object_max_axiom(ontology: &Ontology) {
    let object_max = class_expressions(ontology)
        .into_iter()
        .find_map(|expr| match expr {
            ClassExpression::ObjectMaxCardinality(cardinality, property, qualifier) => {
                Some((*cardinality, property.as_ref(), qualifier.as_ref()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(object_max.0, 3);
    assert_eq!(
        object_property_iri(object_max.1),
        Some(iri("hasVisit").as_str())
    );
    assert!(object_max.2.is_none());
}

fn assert_unqualified_data_min_axiom(ontology: &Ontology) {
    let data_min = class_expressions(ontology)
        .into_iter()
        .find_map(|expr| match expr {
            ClassExpression::DataMinCardinality(cardinality, property, qualifier) => {
                Some((*cardinality, property.as_ref(), qualifier.as_ref()))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(data_min.0, 1);
    assert_eq!(data_property_iri(data_min.1), Some(iri("age").as_str()));
    assert!(data_min.2.is_none());
}

fn object_property_iri(property: &ObjectPropertyExpression) -> Option<&str> {
    match property {
        ObjectPropertyExpression::ObjectProperty(property) => Some(property.iri().as_str()),
        ObjectPropertyExpression::ObjectInverseOf(_) => None,
    }
}

fn data_property_iri(property: &DataPropertyExpression) -> Option<&str> {
    match property {
        DataPropertyExpression::DataProperty(property) => Some(property.iri().as_str()),
    }
}

fn class(local: &str) -> Class {
    Class::new(iri(local))
}

fn object_property(local: &str) -> ObjectProperty {
    ObjectProperty::new(iri(local))
}

fn data_property(local: &str) -> DataProperty {
    DataProperty::new(iri(local))
}

fn datatype(local: &str) -> owl2_reasoner::IRI {
    owl2_reasoner::IRI::new(iri(local)).unwrap()
}

fn iri(local: &str) -> String {
    format!("http://example.org/qualified-binary-test#{local}")
}

fn sha256sum(repo_relative_path: &str) -> String {
    let path = fixture_path(repo_relative_path);
    let output = Command::new("sha256sum").arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "sha256sum failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn fixture_path(repo_relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(repo_relative_path)
}
