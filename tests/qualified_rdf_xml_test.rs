#![cfg(feature = "rio-xml")]

use owl2_reasoner::logic::axioms::class_expressions::{ClassExpression, DataRange};
use owl2_reasoner::logic::axioms::property_expressions::{
    DataPropertyExpression, ObjectPropertyExpression,
};
use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::{Ontology, OwlError};
use std::fs;

const EX: &str = "http://example.org/";

fn parse_default(content: &str) -> Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content).unwrap()
}

fn parse_structural(content: &str) -> Ontology {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content_structural(content).unwrap()
}

fn assert_axiom_validation_error(content: &str, needle: &str) {
    for (mode, result) in [
        ("default", {
            let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
            parser.parse_content(content)
        }),
        ("structural", {
            let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
            parser.parse_content_structural(content)
        }),
    ] {
        match result.unwrap_err() {
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
}

fn assert_resource_error(content: &str, needle: &str) {
    for (mode, result) in [
        ("default", {
            let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
            parser.parse_content(content)
        }),
        ("structural", {
            let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
            parser.parse_content_structural(content)
        }),
    ] {
        match result.unwrap_err() {
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
}

fn local_name(iri: &str) -> &str {
    iri.rsplit('/').next().unwrap_or(iri)
}

fn is_class(expr: &ClassExpression, local: &str) -> bool {
    matches!(expr, ClassExpression::Class(class) if local_name(class.iri().as_str()) == local)
}

fn object_property_local(property: &ObjectPropertyExpression) -> &str {
    match property {
        ObjectPropertyExpression::ObjectProperty(property) => local_name(property.iri().as_str()),
        ObjectPropertyExpression::ObjectInverseOf(_) => "inverse",
    }
}

fn data_property_local(property: &DataPropertyExpression) -> &str {
    match property {
        DataPropertyExpression::DataProperty(property) => local_name(property.iri().as_str()),
    }
}

fn super_expressions(ontology: &Ontology) -> Vec<&ClassExpression> {
    ontology
        .subclass_axioms()
        .into_iter()
        .map(|axiom| axiom.super_class())
        .collect()
}

fn qualified_fixture() -> String {
    format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:ObjectProperty rdf:about="{EX}pMin"/>
  <owl:ObjectProperty rdf:about="{EX}pMax"/>
  <owl:ObjectProperty rdf:about="{EX}pExact"/>
  <owl:DatatypeProperty rdf:about="{EX}dMin"/>
  <owl:DatatypeProperty rdf:about="{EX}dMax"/>
  <owl:DatatypeProperty rdf:about="{EX}dExact"/>

  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="objMin"/></owl:Class>
  <owl:Class rdf:about="{EX}B"><rdfs:subClassOf rdf:nodeID="objMax"/></owl:Class>
  <owl:Class rdf:about="{EX}C"><rdfs:subClassOf rdf:nodeID="objExact"/></owl:Class>
  <owl:Class rdf:about="{EX}D"><rdfs:subClassOf rdf:nodeID="dataMin"/></owl:Class>
  <owl:Class rdf:about="{EX}E"><rdfs:subClassOf rdf:nodeID="dataMax"/></owl:Class>
  <owl:Class rdf:about="{EX}F"><rdfs:subClassOf rdf:nodeID="dataExact"/></owl:Class>
  <owl:Class rdf:about="{EX}G"><rdfs:subClassOf rdf:nodeID="unqualified"/></owl:Class>

  <rdf:Description rdf:nodeID="objMin">
    <owl:onProperty rdf:resource="{EX}pMin"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
    <owl:onClass rdf:nodeID="unionQualifier"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="objMax">
    <owl:onProperty rdf:resource="{EX}pMax"/>
    <owl:maxQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">
      2
    </owl:maxQualifiedCardinality>
    <owl:onClass rdf:resource="{EX}Doctor"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="objExact">
    <owl:onProperty rdf:resource="{EX}pExact"/>
    <owl:qualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#int">+3</owl:qualifiedCardinality>
    <owl:onClass rdf:resource="{EX}Patient"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="dataMin">
    <owl:onProperty rdf:resource="{EX}dMin"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">4</owl:minQualifiedCardinality>
    <owl:onDataRange rdf:resource="{EX}adultAge"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="dataMax">
    <owl:onProperty rdf:resource="{EX}dMax"/>
    <owl:maxQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">5</owl:maxQualifiedCardinality>
    <owl:onDataRange rdf:resource="{EX}rating"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="dataExact">
    <owl:onProperty rdf:resource="{EX}dExact"/>
    <owl:qualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">6</owl:qualifiedCardinality>
    <owl:onDataRange rdf:resource="{EX}code"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="unqualified">
    <owl:onProperty rdf:resource="{EX}pMin"/>
    <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">7</owl:minCardinality>
  </rdf:Description>

  <rdf:Description rdf:nodeID="unionQualifier"><owl:unionOf rdf:nodeID="unionList1"/></rdf:Description>
  <rdf:Description rdf:nodeID="unionList1"><rdf:first rdf:resource="{EX}Nurse"/><rdf:rest rdf:nodeID="unionList2"/></rdf:Description>
  <rdf:Description rdf:nodeID="unionList2"><rdf:first rdf:nodeID="notRetired"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>
  <rdf:Description rdf:nodeID="notRetired"><owl:complementOf rdf:resource="{EX}Retired"/></rdf:Description>
</rdf:RDF>"#
    )
}

fn assert_qualified_cardinalities(ontology: &Ontology) {
    let expressions = super_expressions(ontology);
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::ObjectMinCardinality(1, property, Some(qualifier))
            if object_property_local(property) == "pMin"
                && matches!(qualifier.as_ref(), ClassExpression::ObjectUnionOf(operands)
                    if operands.len() == 2
                        && is_class(&operands[0], "Nurse")
                        && matches!(operands[1].as_ref(), ClassExpression::ObjectComplementOf(inner) if is_class(inner, "Retired")))
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::ObjectMaxCardinality(2, property, Some(qualifier))
            if object_property_local(property) == "pMax" && is_class(qualifier, "Doctor")
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::ObjectExactCardinality(3, property, Some(qualifier))
            if object_property_local(property) == "pExact" && is_class(qualifier, "Patient")
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::DataMinCardinality(4, property, Some(range))
            if data_property_local(property) == "dMin"
                && matches!(range.as_ref(), DataRange::Datatype(iri) if local_name(iri.as_str()) == "adultAge")
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::DataMaxCardinality(5, property, Some(range))
            if data_property_local(property) == "dMax"
                && matches!(range.as_ref(), DataRange::Datatype(iri) if local_name(iri.as_str()) == "rating")
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::DataExactCardinality(6, property, Some(range))
            if data_property_local(property) == "dExact"
                && matches!(range.as_ref(), DataRange::Datatype(iri) if local_name(iri.as_str()) == "code")
    )));
    assert!(expressions.iter().any(|expr| matches!(
        expr,
        ClassExpression::ObjectMinCardinality(7, property, None)
            if object_property_local(property) == "pMin"
    )));
}

#[test]
fn streaming_modes_preserve_qualified_object_and_data_cardinalities() {
    let fixture = qualified_fixture();
    assert_qualified_cardinalities(&parse_default(&fixture));
    assert_qualified_cardinalities(&parse_structural(&fixture));
}

#[test]
fn structural_file_mode_preserves_qualified_cardinality() {
    let path = std::env::temp_dir().join(format!(
        "qualified_rdf_xml_{}_{}.owl",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    fs::write(&path, qualified_fixture()).unwrap();

    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    let ontology = parser.parse_file_structural(&path).unwrap();
    let _ = fs::remove_file(&path);

    assert_qualified_cardinalities(&ontology);
}

#[test]
fn has_self_true_is_preserved_in_streaming_modes() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:ObjectProperty rdf:about="{EX}knows"/>
  <owl:Class rdf:about="{EX}SelfKnower"><rdfs:subClassOf rdf:nodeID="selfRestriction"/></owl:Class>
  <rdf:Description rdf:nodeID="selfRestriction">
    <owl:onProperty rdf:resource="{EX}knows"/>
    <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
  </rdf:Description>
</rdf:RDF>"#
    );

    for ontology in [parse_default(&content), parse_structural(&content)] {
        assert!(super_expressions(&ontology).iter().any(|expr| matches!(
            expr,
            ClassExpression::ObjectHasSelf(property) if object_property_local(property) == "knows"
        )));
    }
}

#[test]
fn forward_referenced_datatype_property_uses_data_qualified_cardinality() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}Measured"><rdfs:subClassOf rdf:nodeID="restriction"/></owl:Class>
  <rdf:Description rdf:nodeID="restriction">
    <owl:onProperty rdf:resource="{EX}value"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
    <owl:onDataRange rdf:resource="{EX}Measurement"/>
  </rdf:Description>
  <owl:DatatypeProperty rdf:about="{EX}value"/>
</rdf:RDF>"#
    );

    for ontology in [parse_default(&content), parse_structural(&content)] {
        assert!(super_expressions(&ontology).iter().any(|expr| matches!(
            expr,
            ClassExpression::DataMinCardinality(1, property, Some(_))
                if data_property_local(property) == "value"
        )));
    }
}

#[test]
fn malformed_qualified_cardinality_errors_instead_of_reinterpreting() {
    let missing_qualifier = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
  </rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error(&missing_qualifier, "missing qualifier");

    let contradictory_qualifiers = missing_qualifier.replace(
        "</rdf:Description>",
        &format!(
            "<owl:onClass rdf:resource=\"{EX}B\"/><owl:onDataRange rdf:resource=\"{EX}D\"/></rdf:Description>"
        ),
    );
    assert_axiom_validation_error(
        &contradictory_qualifiers,
        "both owl:onClass and owl:onDataRange",
    );

    let wrong_datatype = missing_qualifier
        .replace("nonNegativeInteger", "string")
        .replace(
            "</rdf:Description>",
            &format!("<owl:onClass rdf:resource=\"{EX}B\"/></rdf:Description>"),
        );
    assert_axiom_validation_error(&wrong_datatype, "supported xsd integer datatype");

    let overflow = wrong_datatype
        .replace(
            "http://www.w3.org/2001/XMLSchema#string",
            "http://www.w3.org/2001/XMLSchema#integer",
        )
        .replace(">1<", ">4294967296<");
    assert_resource_error(&overflow, "RDF/XML cardinality value");

    let invalid_integer_lexical = wrong_datatype
        .replace(
            "http://www.w3.org/2001/XMLSchema#string",
            "http://www.w3.org/2001/XMLSchema#int",
        )
        .replace(">1<", ">two<");
    assert_axiom_validation_error(&invalid_integer_lexical, "invalid integer lexical value");

    let nbsp_is_not_xml_schema_whitespace = wrong_datatype
        .replace(
            "http://www.w3.org/2001/XMLSchema#string",
            "http://www.w3.org/2001/XMLSchema#int",
        )
        .replace(">1<", ">\u{00A0}1<");
    assert_axiom_validation_error(
        &nbsp_is_not_xml_schema_whitespace,
        "invalid integer lexical value",
    );

    let integer_outside_datatype_range = wrong_datatype
        .replace(
            "http://www.w3.org/2001/XMLSchema#string",
            "http://www.w3.org/2001/XMLSchema#unsignedByte",
        )
        .replace(">1<", ">300<");
    assert_axiom_validation_error(&integer_outside_datatype_range, "outside datatype range");

    let duplicate_count = missing_qualifier.replace(
        "</rdf:Description>",
        &format!(
            "<owl:maxQualifiedCardinality rdf:datatype=\"http://www.w3.org/2001/XMLSchema#nonNegativeInteger\">2</owl:maxQualifiedCardinality><owl:onClass rdf:resource=\"{EX}B\"/></rdf:Description>"
        ),
    );
    assert_axiom_validation_error(&duplicate_count, "exactly one cardinality predicate");

    let duplicate_qualifier = missing_qualifier.replace(
        "</rdf:Description>",
        &format!(
            "<owl:onClass rdf:resource=\"{EX}B\"/><owl:onClass rdf:resource=\"{EX}C\"/></rdf:Description>"
        ),
    );
    assert_axiom_validation_error(&duplicate_qualifier, "duplicate qualifier predicates");

    let wrong_qualifier_kind = missing_qualifier
        .replace(
            "<owl:Class rdf:about=\"http://example.org/A\">",
            &format!("<owl:DatatypeProperty rdf:about=\"{EX}p\"/><owl:Class rdf:about=\"{EX}A\">"),
        )
        .replace(
            "</rdf:Description>",
            &format!("<owl:onClass rdf:resource=\"{EX}B\"/></rdf:Description>"),
        );
    assert_axiom_validation_error(
        &wrong_qualifier_kind,
        "data property cannot use owl:onClass",
    );
}

#[test]
fn repeated_identical_restriction_triples_are_deduplicated_but_conflicts_fail() {
    let repeated_identical = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
    <owl:onClass rdf:resource="{EX}B"/>
    <owl:onClass rdf:resource="{EX}B"/>
  </rdf:Description>
</rdf:RDF>"#
    );
    for ontology in [
        parse_default(&repeated_identical),
        parse_structural(&repeated_identical),
    ] {
        assert!(super_expressions(&ontology).iter().any(|expr| matches!(
            expr,
            ClassExpression::ObjectMinCardinality(1, _, Some(qualifier))
                if is_class(qualifier, "B")
        )));
    }

    let repeated_has_self = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
    <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
  </rdf:Description>
</rdf:RDF>"#
    );
    for ontology in [
        parse_default(&repeated_has_self),
        parse_structural(&repeated_has_self),
    ] {
        assert!(super_expressions(&ontology)
            .iter()
            .any(|expr| matches!(expr, ClassExpression::ObjectHasSelf(_))));
    }

    let conflicting_has_self = repeated_has_self.replacen(">true<", ">false<", 1);
    assert_axiom_validation_error(&conflicting_has_self, "duplicate owl:hasSelf predicates");
}

#[test]
fn mixed_restriction_families_error_instead_of_using_parser_precedence() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:someValuesFrom rdf:resource="{EX}B"/>
    <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
  </rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error(&content, "mixed restriction predicates");

    let boolean_mixed_with_cardinality = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:unionOf rdf:nodeID="list"/>
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minCardinality>
  </rdf:Description>
  <rdf:Description rdf:nodeID="list"><rdf:first rdf:resource="{EX}B"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error(
        &boolean_mixed_with_cardinality,
        "mixed restriction predicates",
    );
}

#[test]
fn conflicting_single_valued_restriction_fields_error_instead_of_last_write_wins() {
    let repeated_identical_scalar_values = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:someValuesFrom rdf:resource="{EX}B"/>
    <owl:someValuesFrom rdf:resource="{EX}B"/>
  </rdf:Description>
</rdf:RDF>"#
    );
    for ontology in [
        parse_default(&repeated_identical_scalar_values),
        parse_structural(&repeated_identical_scalar_values),
    ] {
        assert!(super_expressions(&ontology).iter().any(|expr| matches!(
            expr,
            ClassExpression::ObjectSomeValuesFrom(property, filler)
                if object_property_local(property) == "p" && is_class(filler, "B")
        )));
    }

    let on_property_conflict = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:onProperty rdf:resource="{EX}q"/>
    <owl:someValuesFrom rdf:resource="{EX}B"/>
  </rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error(&on_property_conflict, "Conflicting RDF/XML values");

    let some_values_from_conflict = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:someValuesFrom rdf:resource="{EX}B"/>
    <owl:someValuesFrom rdf:resource="{EX}C"/>
  </rdf:Description>
</rdf:RDF>"#
    );
    assert_axiom_validation_error(&some_values_from_conflict, "Conflicting RDF/XML values");
}

#[test]
fn cyclic_qualified_cardinality_qualifier_errors() {
    let content = format!(
        r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="{EX}A"><rdfs:subClassOf rdf:nodeID="r"/></owl:Class>
  <rdf:Description rdf:nodeID="r">
    <owl:onProperty rdf:resource="{EX}p"/>
    <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minQualifiedCardinality>
    <owl:onClass rdf:nodeID="r"/>
  </rdf:Description>
</rdf:RDF>"#
    );

    assert_axiom_validation_error(&content, "Cyclic OWL class expression");
}
