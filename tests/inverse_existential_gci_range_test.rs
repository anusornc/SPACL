use std::sync::Arc;

use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The W3C description-logic-901 premise: p, q ⊑ r; range(p) = A;
/// range(q) = B; A disjointWith B. The ranges arrive at the kernel as
/// inverse-existential GCIs (`∃p⁻.⊤ ⊑ A`).
const PREMISE_901: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:ObjectProperty rdf:ID="r"/>
  <owl:ObjectProperty rdf:ID="p">
    <rdfs:subPropertyOf rdf:resource="#r"/>
    <rdfs:range><owl:Class rdf:ID="A"/></rdfs:range>
  </owl:ObjectProperty>
  <owl:ObjectProperty rdf:ID="q">
    <rdfs:subPropertyOf rdf:resource="#r"/>
    <rdfs:range><owl:Class rdf:ID="B"/></rdfs:range>
  </owl:ObjectProperty>
  <owl:Class rdf:about="#A">
    <owl:disjointWith rdf:resource="#B"/>
  </owl:Class>
</rdf:RDF>"##;

/// The conclusion: (≥2 p) ⊓ (≥3 q) ⊑ (≥5 r) — the range-typed witness
/// groups cannot merge across A/B, so five r-successors cannot fit
/// under the negated ≤4.
const CONCLUSION_901: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:Class>
    <owl:intersectionOf rdf:parseType="Collection">
      <owl:Restriction>
        <owl:onProperty rdf:resource="#p"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">2</owl:minCardinality>
      </owl:Restriction>
      <owl:Restriction>
        <owl:onProperty rdf:resource="#q"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">3</owl:minCardinality>
      </owl:Restriction>
    </owl:intersectionOf>
    <rdfs:subClassOf>
      <owl:Restriction>
        <owl:onProperty rdf:resource="#r"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">5</owl:minCardinality>
      </owl:Restriction>
    </rdfs:subClassOf>
  </owl:Class>
</rdf:RDF>"##;

/// Without the disjointness the witness groups may merge, so the same
/// conclusion shape must NOT be entailed — the range typing alone
/// never invents a partition failure.
const PREMISE_901_COMPATIBLE: &str = r##"<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xml:base="http://example.org/">
  <owl:Ontology/>
  <owl:ObjectProperty rdf:ID="r"/>
  <owl:ObjectProperty rdf:ID="p">
    <rdfs:subPropertyOf rdf:resource="#r"/>
    <rdfs:range><owl:Class rdf:ID="A"/></rdfs:range>
  </owl:ObjectProperty>
  <owl:ObjectProperty rdf:ID="q">
    <rdfs:subPropertyOf rdf:resource="#r"/>
    <rdfs:range><owl:Class rdf:ID="B"/></rdfs:range>
  </owl:ObjectProperty>
  <owl:Class rdf:about="#A"/>
</rdf:RDF>"##;

fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    use owl2_reasoner::parser::ParserConfig;
    use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

#[test]
fn inverse_existential_range_gci_types_witnesses_into_a_partition_clash() {
    let premise = Arc::new(parse(PREMISE_901).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_901).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "range-typed A/B witnesses under disjointness force five distinct r-successors"
    );
}

#[test]
fn compatible_ranges_leave_the_cardinality_conclusion_unentailed() {
    let premise = Arc::new(parse(PREMISE_901_COMPATIBLE).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_901).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "without disjointness the witness groups may merge; genuine non-entailment"
    );
}
