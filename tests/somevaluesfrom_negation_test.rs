use std::sync::Arc;

use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The W3C bnode2someValuesFrom premise: an asserted p-edge to a blank
/// node.
const PREMISE_WITH_EDGE: &str = r##"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:ex="http://example.org/"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:Thing rdf:about="a">
  <ex:p><rdf:Description rdf:nodeID="x"/></ex:p>
</owl:Thing>
</rdf:RDF>"##;

/// The same premise without the edge: ∃p.⊤ must NOT be entailed.
const PREMISE_WITHOUT_EDGE: &str = r##"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:Thing rdf:about="a"/>
</rdf:RDF>"##;

/// a : ∃p.⊤ — the restriction-shaped conclusion whose negation is
/// ∀p.¬Thing.
const CONCLUSION_EXISTENTIAL: &str = r##"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:Thing rdf:about="a">
  <rdf:type>
    <owl:Restriction>
      <owl:onProperty rdf:resource="p"/>
      <owl:someValuesFrom rdf:resource="http://www.w3.org/2002/07/owl#Thing"/>
    </owl:Restriction>
  </rdf:type>
</owl:Thing>
</rdf:RDF>"##;

fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

#[test]
fn asserted_edge_to_a_bnode_entails_the_existential() {
    let premise = Arc::new(parse(PREMISE_WITH_EDGE).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_EXISTENTIAL).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "one p-edge, even to a blank node, satisfies ∃p.⊤ (W3C bnode2someValuesFrom)"
    );
}

#[test]
fn no_edge_leaves_the_existential_unentailed() {
    let premise = Arc::new(parse(PREMISE_WITHOUT_EDGE).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_EXISTENTIAL).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "genuine non-entailment: nothing forces a p-successor into existence"
    );
}
