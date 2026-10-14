#![cfg(feature = "rio-xml")]

use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};

use owl2_reasoner::core::entities::Individual;
use owl2_reasoner::parser::{rdf_xml::RdfXmlParser, OntologyParser};
use owl2_reasoner::serializer::BinaryOntologyFormat;
use owl2_reasoner::util::ontology_io::load_ontology_with_env;
use owl2_reasoner::{evaluate_consistency, Axiom, ClassExpression, Ontology, OwlError};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const ENV_KEYS: &[&str] = &[
    "OWL2_REASONER_AUTO_CACHE",
    "OWL2_REASONER_BIN_FORMAT",
    "OWL2_REASONER_BIN_ONLY",
    "OWL2_REASONER_DISABLE_PARSE_FALLBACK",
    "OWL2_REASONER_ENABLE_PARSE_FALLBACK",
    "OWL2_REASONER_EXPERIMENTAL_XML_PARSER",
    "OWL2_REASONER_EXPERIMENTAL_XML_STRICT",
    "OWL2_REASONER_FORCE_TEXT",
    "OWL2_REASONER_LARGE_PROFILE_AUTO",
    "OWL2_REASONER_STAGE_TIMING",
    "OWL2_REASONER_STRUCTURAL_XML_PARSER",
    "SPACL_SCHED_MODE",
];

const UBERON_NOMINAL_CLASS: &str = "http://purl.obolibrary.org/obo/IAO_0000078";
const EXPECTED_NOMINAL_MEMBERS: &[&str] = &[
    "http://purl.obolibrary.org/obo/IAO_0000002",
    "http://purl.obolibrary.org/obo/IAO_0000120",
    "http://purl.obolibrary.org/obo/IAO_0000121",
    "http://purl.obolibrary.org/obo/IAO_0000122",
    "http://purl.obolibrary.org/obo/IAO_0000123",
    "http://purl.obolibrary.org/obo/IAO_0000124",
    "http://purl.obolibrary.org/obo/IAO_0000125",
    "http://purl.obolibrary.org/obo/IAO_0000423",
    "http://purl.obolibrary.org/obo/IAO_0000428",
];

struct EnvGuard<'a> {
    _lock: MutexGuard<'a, ()>,
    previous: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard<'_> {
    fn isolated() -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let previous = ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect();
        for key in ENV_KEYS {
            std::env::remove_var(key);
        }
        Self {
            _lock: lock,
            previous,
        }
    }

    fn set(&self, key: &str, value: &str) {
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard<'_> {
    fn drop(&mut self) {
        for (key, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[test]
fn binary_v2_v3_and_v4_roundtrip_uberon_nominal_members_and_directions() {
    let env = EnvGuard::isolated();
    let ontology = load_uberon_nominal_fixture();

    assert_uberon_nominal_directions(&ontology);

    for (format, version) in [("v2", 2), ("v3", 3), ("v4", 4)] {
        env.set("OWL2_REASONER_BIN_FORMAT", format);
        let mut bytes = Vec::new();

        BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap();

        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            version,
            "{format}"
        );
        let mut reader = Cursor::new(bytes);
        let decoded = BinaryOntologyFormat::deserialize(&mut reader).unwrap();
        assert_uberon_nominal_directions(&decoded);
    }
}

#[test]
fn binary_v1_rejects_uberon_nominal_without_emitting_partial_bytes() {
    let env = EnvGuard::isolated();
    env.set("OWL2_REASONER_BIN_FORMAT", "v1");
    let ontology = load_uberon_nominal_fixture();
    let mut bytes = Vec::new();

    let error = BinaryOntologyFormat::serialize(&ontology, &mut bytes).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        error.to_string().contains("not representable"),
        "unexpected v1 error: {error}"
    );
    assert!(bytes.is_empty());
}

#[test]
fn loaded_uberon_nominal_evaluates_as_consistent() {
    // #113: the nominal used to abort as unsupported; the sequential
    // layers now decide this satisfiable definition as consistent.
    let _env = EnvGuard::isolated();
    let ontology = load_uberon_nominal_fixture();

    let response = evaluate_consistency(Arc::new(ontology), "uberon_oneof_minimal.owl");

    assert_eq!(response.status.as_str(), "not_evaluated");
    assert_eq!(response.semantic_result.as_str(), "consistent");
    assert!(response.unsupported_fragment.is_none());
    assert_eq!(
        serde_json::to_value(&response).unwrap()["execution_status"],
        "success"
    );
}

#[test]
fn evaluate_cli_reports_unsupported_unknown_and_parse_timing_for_uberon_nominal() {
    let _env = EnvGuard::isolated();
    let fixture = fixture_path("tests/data/uberon_oneof_minimal.owl");

    let mut command = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"));
    command.args(["evaluate", "consistency", fixture.to_str().unwrap()]);
    for key in ENV_KEYS {
        command.env_remove(key);
    }
    let output = command.output().expect("evaluate CLI should start");

    assert!(output.status.success(), "CLI failed: {:?}", output);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI stdout should be one JSON response");
    // #113: decided (satisfiable) instead of unsupported.
    assert_eq!(response["status"], "not_evaluated");
    assert_eq!(response["semantic_result"], "consistent");
    assert!(response["unsupported_fragment"].is_null());
    assert!(response["parse_time_ms"].as_u64().is_some());
}

#[test]
fn malformed_object_enumerations_fail_closed_through_rdf_xml_facade_and_loader() {
    let _env = EnvGuard::isolated();

    for (body, expected) in [
        (
            r#"<rdf:Description rdf:nodeID="list">
                <rdf:first>literal-member</rdf:first>
                <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
              </rdf:Description>"#,
            "literal member",
        ),
        (
            r#"<rdf:Description rdf:nodeID="list">
                <rdf:first rdf:nodeID="anonymous-member"/>
                <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
              </rdf:Description>"#,
            "anonymous individual",
        ),
    ] {
        let content = object_enumeration_document(body);
        assert_object_enumeration_error(RdfXmlParser::new().parse_str(&content), expected);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad-oneof.owl");
        std::fs::write(&path, content).unwrap();
        assert_object_enumeration_error(load_ontology_with_env(&path), expected);
    }
}

fn load_uberon_nominal_fixture() -> Ontology {
    load_ontology_with_env(&fixture_path("tests/data/uberon_oneof_minimal.owl"))
        .expect("Uberon oneOf fixture should load through the public loader")
}

fn assert_uberon_nominal_directions(ontology: &Ontology) {
    let expected: Vec<_> = EXPECTED_NOMINAL_MEMBERS
        .iter()
        .map(|iri| (*iri).to_owned())
        .collect();
    let mut directions = [false, false];

    for axiom in ontology.axioms() {
        if let Axiom::SubClassOf(subclass) = axiom.as_ref() {
            for (index, named, nominal) in [
                (0, subclass.sub_class(), subclass.super_class()),
                (1, subclass.super_class(), subclass.sub_class()),
            ] {
                if matches!(named, ClassExpression::Class(class) if class.iri().as_str() == UBERON_NOMINAL_CLASS)
                    && nominal_members(nominal) == Some(expected.clone())
                {
                    directions[index] = true;
                }
            }
        }
    }

    assert_eq!(directions, [true, true]);
}

fn nominal_members(expression: &ClassExpression) -> Option<Vec<String>> {
    let ClassExpression::ObjectOneOf(members) = expression else {
        return None;
    };

    Some(
        members
            .iter()
            .map(|member| match member {
                Individual::Named(individual) => individual.iri().as_str().to_owned(),
                Individual::Anonymous(_) => panic!("nominal member became anonymous"),
            })
            .collect(),
    )
}

fn object_enumeration_document(list_body: &str) -> String {
    format!(
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
          xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
          xmlns:owl="http://www.w3.org/2002/07/owl#">
          <owl:Class rdf:about="http://example.org/A">
            <rdfs:subClassOf rdf:nodeID="nominal"/>
          </owl:Class>
          <rdf:Description rdf:nodeID="nominal">
            <owl:oneOf rdf:nodeID="list"/>
          </rdf:Description>
          {list_body}
        </rdf:RDF>"#
    )
}

fn assert_object_enumeration_error(result: Result<Ontology, OwlError>, expected: &str) {
    match result.unwrap_err() {
        OwlError::AxiomValidationError {
            axiom_type,
            message,
        } => {
            assert_eq!(axiom_type, "RDF/XML class expression");
            assert!(message.contains(expected), "{message}");
        }
        other => panic!("wrong object enumeration error: {other}"),
    }
}

fn fixture_path(relative_path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}
