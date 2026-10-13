use std::sync::Arc;

use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The W3C equivalentClass-004 premise: c1 and c2 both equivalentClass
/// (=1 p).
const PREMISE_BOTH_DEFINED: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
    <owl:Ontology/>
    <owl:Class rdf:ID="c1">
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Restriction>
          <owl:onProperty rdf:resource="#p"/>
          <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:cardinality>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
    <owl:Class rdf:ID="c2">
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Restriction>
          <owl:onProperty rdf:resource="#p"/>
          <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:cardinality>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
    <owl:ObjectProperty rdf:ID="p"/>
</rdf:RDF>"##;

/// The soundness control: c1 carries the equivalence, c3 only a
/// one-directional subclass — c1 ≡ c3 must NOT be entailed.
const PREMISE_ONE_DIRECTION: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
    <owl:Ontology/>
    <owl:Class rdf:ID="c1">
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Restriction>
          <owl:onProperty rdf:resource="#p"/>
          <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:cardinality>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
    <owl:Class rdf:ID="c3">
      <rdfs:subClassOf>
        <owl:Restriction>
          <owl:onProperty rdf:resource="#p"/>
          <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:cardinality>
        </owl:Restriction>
      </rdfs:subClassOf>
    </owl:Class>
    <owl:ObjectProperty rdf:ID="p"/>
</rdf:RDF>"##;

fn conclusion_equivalent(left: &str, right: &str) -> String {
    format!(
        r##"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
    <owl:Ontology/>
    <owl:Class rdf:about="{left}">
      <owl:equivalentClass><owl:Class rdf:about="{right}"/></owl:equivalentClass>
    </owl:Class>
</rdf:RDF>"##
    )
}

fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

#[test]
fn equivalent_definitions_entail_the_equivalence() {
    let premise = Arc::new(parse(PREMISE_BOTH_DEFINED).unwrap());
    let conclusion = Arc::new(parse(&conclusion_equivalent("#c1", "#c2")).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "the subsumption witness's negated c2 must expand through the premise's \
         equivalence into (=2p OR =0p), both of which die against c1's =1p"
    );
}

#[test]
fn one_directional_subclass_does_not_entail_the_equivalence() {
    let premise = Arc::new(parse(PREMISE_ONE_DIRECTION).unwrap());
    let conclusion = Arc::new(parse(&conclusion_equivalent("#c1", "#c3")).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "c3 only subclasses the restriction; pushing the negation through a \
         one-directional subclass would be unsound"
    );
}
