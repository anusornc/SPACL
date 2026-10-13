use std::sync::Arc;

use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The W3C I5.24-003 premise: prop's range is A.
const PREMISE_WITH_RANGE: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:ObjectProperty rdf:ID="prop">
     <rdfs:range><owl:Class rdf:ID="A"/></rdfs:range>
  </owl:ObjectProperty>
</rdf:RDF>"##;

/// Without the range nothing constrains prop's fillers.
const PREMISE_WITHOUT_RANGE: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:ObjectProperty rdf:ID="prop"/>
  <owl:Class rdf:ID="A"/>
</rdf:RDF>"##;

/// Thing ⊑ ∀prop.A — the range's global reading.
const CONCLUSION_UNIVERSAL: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:Class rdf:about="http://www.w3.org/2002/07/owl#Thing">
    <rdfs:subClassOf>
      <owl:Restriction>
        <owl:onProperty rdf:resource="#prop"/>
        <owl:allValuesFrom rdf:resource="#A"/>
      </owl:Restriction>
    </rdfs:subClassOf>
  </owl:Class>
</rdf:RDF>"##;

fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

#[test]
fn object_range_entails_the_global_universal() {
    let premise = Arc::new(parse(PREMISE_WITH_RANGE).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_UNIVERSAL).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "the witness's negated universal must become an existential over the complement, \
         whose filler the lowered range GCI types into A (W3C I5.24-003)"
    );
}

#[test]
fn rangeless_property_leaves_the_universal_unentailed() {
    let premise = Arc::new(parse(PREMISE_WITHOUT_RANGE).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_UNIVERSAL).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "genuine non-entailment: nothing types the fillers"
    );
}
