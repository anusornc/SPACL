#![cfg(feature = "rio-xml")]

use owl2_reasoner::parser::{rdf_xml::RdfXmlParser, OntologyParser};
use owl2_reasoner::util::ontology_io::load_ontology_with_env;
use owl2_reasoner::ClassExpression;
use owl2_reasoner::OwlError;

fn malformed_union() -> &'static str {
    r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
        xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
        xmlns:owl="http://www.w3.org/2002/07/owl#">
      <owl:Class rdf:about="http://example.org/A">
        <rdfs:subClassOf rdf:nodeID="union"/>
      </owl:Class>
      <owl:Class rdf:nodeID="union"><owl:unionOf rdf:nodeID="list"/></owl:Class>
      <rdf:Description rdf:nodeID="list">
        <rdf:first rdf:resource="http://example.org/B"/>
      </rdf:Description>
    </rdf:RDF>"#
}

fn assert_materialization_error(result: Result<owl2_reasoner::Ontology, OwlError>) {
    assert!(matches!(
        result,
        Err(OwlError::AxiomValidationError { axiom_type, .. })
            if axiom_type == "RDF/XML class expression"
    ));
}

#[test]
fn default_parser_does_not_retry_lossy_legacy_after_materialization_failure() {
    assert_materialization_error(RdfXmlParser::new().parse_str(malformed_union()));
}

#[test]
fn file_parser_and_public_loader_preserve_materialization_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("malformed.owl");
    std::fs::write(&path, malformed_union()).unwrap();
    assert_materialization_error(RdfXmlParser::new().parse_file(&path));
    assert_materialization_error(load_ontology_with_env(&path));
}

#[test]
fn corpus_blocker_shapes_are_preserved_after_bounded_lowering_support() {
    let qualified = RdfXmlParser::new().parse_str(include_str!(
        "data/rdf_xml_qualified_cardinality_unsupported.owl"
    ));
    assert!(qualified
        .unwrap()
        .subclass_axioms()
        .iter()
        .any(|axiom| matches!(
            axiom.super_class(),
            ClassExpression::ObjectMinCardinality(1, _, Some(_))
        )));

    let has_self =
        RdfXmlParser::new().parse_str(include_str!("data/rdf_xml_has_self_unsupported.owl"));
    assert!(has_self
        .unwrap()
        .subclass_axioms()
        .iter()
        .any(|axiom| matches!(axiom.super_class(), ClassExpression::ObjectHasSelf(_))));
}
