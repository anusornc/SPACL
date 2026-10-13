use owl2_reasoner::logic::axioms::class_expressions::ClassExpression;
use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::OwlError;
use owl2_reasoner::SimpleReasoner;
use std::fs;

const EX: &str = "http://example.org/";

fn boolean_fixture() -> String {
    format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A">
    <rdfs:subClassOf rdf:nodeID="aUnion"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}D">
    <rdfs:subClassOf rdf:nodeID="dIntersection"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}G">
    <rdfs:subClassOf rdf:nodeID="gComplement"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}M">
    <owl:equivalentClass rdf:nodeID="mUnion"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}P">
    <owl:equivalentClass rdf:resource="{EX}Q"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}X">
    <rdfs:subClassOf rdf:resource="{EX}Y"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}R">
    <rdfs:subClassOf rdf:nodeID="sharedUnion"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}S">
    <rdfs:subClassOf rdf:nodeID="sharedUnion"/>
  </owl:Class>
  <owl:Class rdf:about="{EX}T">
    <rdfs:subClassOf rdf:nodeID="reuseUnion"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="anonymousLhs">
    <owl:unionOf rdf:nodeID="lhsList1"/>
    <rdfs:subClassOf rdf:resource="{EX}K"/>
  </rdf:Description>

  <rdf:Description rdf:nodeID="aUnion"><owl:unionOf rdf:nodeID="aList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="aList1"><rdf:first rdf:resource="{EX}B"/><rdf:rest rdf:nodeID="aList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="aList2"><rdf:first rdf:resource="{EX}C"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>

  <rdf:Description rdf:nodeID="dIntersection"><owl:intersectionOf rdf:nodeID="dList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="dList1"><rdf:first rdf:resource="{EX}E"/><rdf:rest rdf:nodeID="dList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="dList2"><rdf:first rdf:resource="{EX}F"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>

  <rdf:Description rdf:nodeID="gComplement"><owl:complementOf rdf:resource="{EX}H"/></rdf:Description>

  <rdf:Description rdf:nodeID="mUnion"><owl:unionOf rdf:nodeID="mList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="mList1"><rdf:first rdf:resource="{EX}N"/><rdf:rest rdf:nodeID="mList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="mList2"><rdf:first rdf:resource="{EX}O"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>

  <rdf:Description rdf:nodeID="lhsList1"><rdf:first rdf:resource="{EX}I"/><rdf:rest rdf:nodeID="lhsList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="lhsList2"><rdf:first rdf:resource="{EX}J"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>

  <rdf:Description rdf:nodeID="sharedUnion"><owl:unionOf rdf:nodeID="sharedList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="sharedList1"><rdf:first rdf:resource="{EX}U"/><rdf:rest rdf:nodeID="sharedList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="sharedList2"><rdf:first rdf:resource="{EX}V"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>

  <rdf:Description rdf:nodeID="reuseUnion"><owl:unionOf rdf:nodeID="reuseList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="reuseList1"><rdf:first rdf:nodeID="sharedComplement"/><rdf:rest rdf:nodeID="reuseList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="reuseList2"><rdf:first rdf:nodeID="sharedComplement"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>
  <rdf:Description rdf:nodeID="sharedComplement"><owl:complementOf rdf:resource="{EX}Z"/></rdf:Description>
</rdf:RDF>"#
    )
}

fn parse_default() -> owl2_reasoner::Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(&boolean_fixture()).unwrap()
}

fn parse_structural() -> owl2_reasoner::Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content_structural(&boolean_fixture()).unwrap()
}

fn parse_default_content(content: &str) -> owl2_reasoner::Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content).unwrap()
}

fn parse_structural_content(content: &str) -> owl2_reasoner::Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content_structural(content).unwrap()
}

fn assert_consistency_in_modes(content: &str, expected: bool) {
    assert_eq!(
        SimpleReasoner::new(parse_default_content(content))
            .is_consistent()
            .unwrap(),
        expected
    );
    assert_eq!(
        SimpleReasoner::new(parse_structural_content(content))
            .is_consistent()
            .unwrap(),
        expected
    );
}

fn assert_axiom_validation_error_in_modes(content: &str, needle: &str) {
    let mut default_parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let default_err = default_parser.parse_content(content).unwrap_err();
    assert_axiom_validation_error(default_err, needle, "default");

    let mut structural_parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let structural_err = structural_parser
        .parse_content_structural(content)
        .unwrap_err();
    assert_axiom_validation_error(structural_err, needle, "structural");
}

fn assert_resource_error_in_modes(content: &str, needle: &str) {
    let mut default_parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let default_err = default_parser.parse_content(content).unwrap_err();
    assert_resource_error(default_err, needle, "default");

    let mut structural_parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let structural_err = structural_parser
        .parse_content_structural(content)
        .unwrap_err();
    assert_resource_error(structural_err, needle, "structural");
}

fn assert_axiom_validation_error(error: OwlError, needle: &str, mode: &str) {
    match error {
        OwlError::AxiomValidationError {
            axiom_type,
            message,
        } => {
            assert_eq!(axiom_type, "RDF/XML class expression");
            assert!(
                message.contains(needle),
                "{mode} error did not contain {needle}: {message}"
            );
        }
        other => panic!("{mode} returned wrong error type: {other}"),
    }
}

fn assert_resource_error(error: OwlError, needle: &str, mode: &str) {
    match error {
        OwlError::ResourceLimitExceeded {
            resource_type,
            message,
            ..
        } => {
            assert!(
                resource_type.contains(needle) || message.contains(needle),
                "{mode} resource error did not contain {needle}: {resource_type}: {message}"
            );
        }
        other => panic!("{mode} returned wrong error type: {other}"),
    }
}

fn class_name(expr: &ClassExpression) -> Option<&str> {
    match expr {
        ClassExpression::Class(class) => Some(class.iri().as_str()),
        _ => None,
    }
}

fn is_class(expr: &ClassExpression, local: &str) -> bool {
    class_name(expr) == Some(format!("{EX}{local}").as_str())
}

fn has_union_operands(expr: &ClassExpression, left: &str, right: &str) -> bool {
    match expr {
        ClassExpression::ObjectUnionOf(operands) => {
            operands.len() == 2 && is_class(&operands[0], left) && is_class(&operands[1], right)
        }
        _ => false,
    }
}

fn has_intersection_operands(expr: &ClassExpression, left: &str, right: &str) -> bool {
    match expr {
        ClassExpression::ObjectIntersectionOf(operands) => {
            operands.len() == 2 && is_class(&operands[0], left) && is_class(&operands[1], right)
        }
        _ => false,
    }
}

fn is_complement_of(expr: &ClassExpression, local: &str) -> bool {
    match expr {
        ClassExpression::ObjectComplementOf(operand) => is_class(operand, local),
        _ => false,
    }
}

fn assert_boolean_lowering(ontology: &owl2_reasoner::Ontology) {
    let subclass_axioms = ontology.subclass_axioms();
    assert_eq!(subclass_axioms.len(), 10);
    assert_eq!(ontology.equivalent_classes_axioms().len(), 1);
    assert!(ontology.equivalent_classes_axioms()[0]
        .classes()
        .iter()
        .any(|iri| iri.as_str() == format!("{EX}P")));
    assert!(ontology.equivalent_classes_axioms()[0]
        .classes()
        .iter()
        .any(|iri| iri.as_str() == format!("{EX}Q")));

    assert!(subclass_axioms
        .iter()
        .any(|axiom| is_class(axiom.sub_class(), "A")
            && has_union_operands(axiom.super_class(), "B", "C")));
    assert!(subclass_axioms
        .iter()
        .any(|axiom| is_class(axiom.sub_class(), "D")
            && has_intersection_operands(axiom.super_class(), "E", "F")));
    assert!(subclass_axioms.iter().any(
        |axiom| is_class(axiom.sub_class(), "G") && is_complement_of(axiom.super_class(), "H")
    ));
    assert!(subclass_axioms
        .iter()
        .any(|axiom| has_union_operands(axiom.sub_class(), "I", "J")
            && is_class(axiom.super_class(), "K")));
    assert!(subclass_axioms
        .iter()
        .any(|axiom| is_class(axiom.sub_class(), "M")
            && has_union_operands(axiom.super_class(), "N", "O")));
    assert!(subclass_axioms
        .iter()
        .any(|axiom| has_union_operands(axiom.sub_class(), "N", "O")
            && is_class(axiom.super_class(), "M")));
    assert!(subclass_axioms
        .iter()
        .any(|axiom| is_class(axiom.sub_class(), "X") && is_class(axiom.super_class(), "Y")));
    assert!(
        subclass_axioms
            .iter()
            .filter(|axiom| has_union_operands(axiom.super_class(), "U", "V"))
            .count()
            == 2
    );
    assert!(subclass_axioms.iter().any(|axiom| {
        is_class(axiom.sub_class(), "T")
            && matches!(axiom.super_class(), ClassExpression::ObjectUnionOf(operands)
                if operands.len() == 2
                    && is_complement_of(&operands[0], "Z")
                    && is_complement_of(&operands[1], "Z"))
    }));
}

#[test]
fn default_streaming_lowers_boolean_class_edges() {
    let ontology = parse_default();
    assert_boolean_lowering(&ontology);
}

#[test]
fn structural_streaming_lowers_boolean_class_edges() {
    let ontology = parse_structural();
    assert_boolean_lowering(&ontology);
}

#[test]
fn structural_file_parse_lowers_boolean_class_edges() {
    let path = std::env::temp_dir().join(format!(
        "rdf_xml_boolean_lowering_{}_{}.owl",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    fs::write(&path, boolean_fixture()).unwrap();

    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let result = parser.parse_file_structural(&path);
    let _ = fs::remove_file(&path);
    assert_boolean_lowering(&result.unwrap());
}

#[test]
fn boolean_list_cycle_errors_instead_of_disappearing() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="cyclicUnion"/></owl:Class>
  <rdf:Description rdf:nodeID="cyclicUnion"><owl:unionOf rdf:nodeID="list"/></rdf:Description>
  <rdf:Description rdf:nodeID="list"><rdf:first rdf:resource="{EX}B"/><rdf:rest rdf:nodeID="list"/></rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error_in_modes(&content, "Cyclic RDF list");
}

#[test]
fn boolean_list_missing_rest_errors_instead_of_disappearing() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="badUnion"/></owl:Class>
  <rdf:Description rdf:nodeID="badUnion"><owl:unionOf rdf:nodeID="list"/></rdf:Description>
  <rdf:Description rdf:nodeID="list"><rdf:first rdf:resource="{EX}B"/></rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error_in_modes(&content, "missing rdf:rest");
}

#[test]
fn retained_boolean_subclass_contradiction_is_unsat_when_asserted() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A">
    <rdfs:subClassOf rdf:nodeID="notA"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="notA">
    <owl:complementOf rdf:resource="{EX}A"/>
  </rdf:Description>
  <owl:NamedIndividual rdf:about="{EX}x">
    <rdf:type rdf:resource="{EX}A"/>
  </owl:NamedIndividual>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, false);
}

#[test]
fn uninstantiated_contradictory_boolean_class_remains_sat() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A">
    <rdfs:subClassOf rdf:nodeID="notA"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="notA">
    <owl:complementOf rdf:resource="{EX}A"/>
  </rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, true);
}

#[test]
fn anonymous_equivalence_named_to_union_direction_is_enforced() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}M">
    <owl:equivalentClass rdf:nodeID="mUnion"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="mUnion">
    <owl:unionOf rdf:nodeID="list1"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list1">
    <rdf:first rdf:resource="{EX}N"/>
    <rdf:rest rdf:nodeID="list2"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list2">
    <rdf:first rdf:resource="{EX}O"/>
    <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
  </rdf:Description>
  <owl:NamedIndividual rdf:about="{EX}x">
    <rdf:type rdf:resource="{EX}M"/>
    <rdf:type rdf:nodeID="notN"/>
    <rdf:type rdf:nodeID="notO"/>
  </owl:NamedIndividual>
  <rdf:Description rdf:nodeID="notN"><owl:complementOf rdf:resource="{EX}N"/></rdf:Description>
  <rdf:Description rdf:nodeID="notO"><owl:complementOf rdf:resource="{EX}O"/></rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, false);
}

#[test]
fn anonymous_equivalence_union_to_named_direction_is_enforced() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}M">
    <owl:equivalentClass rdf:nodeID="mUnion"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="mUnion">
    <owl:unionOf rdf:nodeID="list1"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list1">
    <rdf:first rdf:resource="{EX}N"/>
    <rdf:rest rdf:nodeID="list2"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list2">
    <rdf:first rdf:resource="{EX}O"/>
    <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
  </rdf:Description>
  <owl:NamedIndividual rdf:about="{EX}x">
    <rdf:type rdf:resource="{EX}N"/>
    <rdf:type rdf:nodeID="notM"/>
  </owl:NamedIndividual>
  <rdf:Description rdf:nodeID="notM"><owl:complementOf rdf:resource="{EX}M"/></rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, false);
}

#[test]
fn deep_blank_node_class_expression_errors_at_resolution_budget() {
    let mut descriptions = String::new();
    for index in 0..140 {
        descriptions.push_str(&format!(
            r#"<rdf:Description rdf:nodeID="n{index}"><owl:complementOf rdf:nodeID="n{}"/></rdf:Description>"#,
            index + 1
        ));
    }
    descriptions.push_str(&format!(
        r#"<rdf:Description rdf:nodeID="n140"><owl:complementOf rdf:resource="{EX}B"/></rdf:Description>"#
    ));
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="n0"/></owl:Class>
  {descriptions}
</rdf:RDF>"#
    );

    assert_resource_error_in_modes(&content, "depth");
}

#[test]
fn exponential_reused_dag_class_expression_errors_at_generated_node_budget() {
    let mut descriptions = String::new();
    for index in 0..30 {
        descriptions.push_str(&format!(
            r#"<rdf:Description rdf:nodeID="n{index}"><owl:unionOf rdf:nodeID="l{index}a"/></rdf:Description>
<rdf:Description rdf:nodeID="l{index}a"><rdf:first rdf:nodeID="n{}"/><rdf:rest rdf:nodeID="l{index}b"/></rdf:Description>
<rdf:Description rdf:nodeID="l{index}b"><rdf:first rdf:nodeID="n{}"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>"#,
            index + 1,
            index + 1
        ));
    }
    descriptions.push_str(&format!(
        r#"<rdf:Description rdf:nodeID="n30"><owl:complementOf rdf:resource="{EX}B"/></rdf:Description>"#
    ));
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="n0"/></owl:Class>
  {descriptions}
</rdf:RDF>"#
    );

    assert_resource_error_in_modes(&content, "generated nodes");
}

#[test]
fn anonymous_one_of_equivalence_preserves_nominals_without_certifying_sat() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A">
    <owl:equivalentClass rdf:nodeID="aOneOf"/>
    <owl:disjointWith rdf:resource="{EX}B"/>
  </owl:Class>
  <rdf:Description rdf:nodeID="aOneOf">
    <owl:oneOf rdf:nodeID="list1"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list1">
    <rdf:first rdf:resource="{EX}a"/>
    <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
  </rdf:Description>
  <owl:NamedIndividual rdf:about="{EX}a">
    <rdf:type rdf:resource="{EX}B"/>
  </owl:NamedIndividual>
</rdf:RDF>"#
    );

    for ontology in [
        parse_default_content(&content),
        parse_structural_content(&content),
    ] {
        // #114: the oneOf-subject GCI asserts a into A, and A is
        // disjoint from B which a is typed into — inconsistent.
        assert!(!SimpleReasoner::new(ontology).is_consistent().unwrap());
    }
}

#[test]
fn empty_union_assertion_is_unsat_in_default_and_structural_modes() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:NamedIndividual rdf:about="{EX}x">
    <rdf:type rdf:nodeID="emptyUnion"/>
  </owl:NamedIndividual>
  <rdf:Description rdf:nodeID="emptyUnion">
    <owl:unionOf rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
  </rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, false);
}

#[test]
fn empty_intersection_assertion_is_sat_in_default_and_structural_modes() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:NamedIndividual rdf:about="{EX}x">
    <rdf:type rdf:nodeID="emptyIntersection"/>
  </owl:NamedIndividual>
  <rdf:Description rdf:nodeID="emptyIntersection">
    <owl:intersectionOf rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
  </rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, true);
}

#[test]
fn vacuous_empty_union_subclass_remains_sat_in_default_and_structural_modes() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <rdf:Description rdf:nodeID="emptyUnion">
    <owl:unionOf rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
    <rdfs:subClassOf rdf:resource="{EX}A"/>
  </rdf:Description>
</rdf:RDF>"#
    );

    assert_consistency_in_modes(&content, true);
}
