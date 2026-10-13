use std::sync::Arc;

use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The W3C equivalentProperty-004 premise: p, q functional with domain
/// d; d equivalent to both hasValue restrictions; an asserted p-edge.
/// The entailment lane lowers the domains, so the probe derives x—q→v,
/// functional p merges y≡v, and the reduction's ¬q(x,y) must clash
/// with the derived edge.
const PREMISE_CHAIN: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
  <rdfs:domain rdf:resource="d"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="q">
  <rdfs:domain rdf:resource="d"/>
</owl:ObjectProperty>
<owl:Thing rdf:about="v"/>
<owl:Class rdf:about="d">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="p"/>
      <owl:hasValue rdf:resource="v"/>
    </owl:Restriction>
  </owl:equivalentClass>
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="q"/>
      <owl:hasValue rdf:resource="v"/>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<rdf:Description rdf:about="x">
  <p rdf:resource="y"/>
</rdf:Description>
<rdf:Description rdf:about="y"/>
</rdf:RDF>"##;

/// The same premise with nothing linking the two properties: the
/// equivalence must NOT be entailed — the negation's fresh pair stays
/// peacefully p-connected and q-unconnected.
const PREMISE_UNLINKED: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:ObjectProperty rdf:about="q"/>
</rdf:RDF>"##;

/// p equivalentProperty q — the axiom-level conclusion whose negation
/// is a fresh pair with p(x,y) and ¬q(x,y).
const CONCLUSION_EQUIVALENT: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p">
  <owl:equivalentProperty>
    <owl:ObjectProperty rdf:about="q"/>
  </owl:equivalentProperty>
</owl:ObjectProperty>
</rdf:RDF>"##;

fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

/// ¬p(x,y) is also violated by the INVERSE-realizing edge (y, q, x)
/// with q the declared inverse of p — the endpoints swap (#138).
const INVERSE_REALIZED: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:ObjectProperty rdf:about="q">
  <owl:inverseOf rdf:resource="p"/>
</owl:ObjectProperty>
<rdf:Description rdf:about="y">
  <q rdf:resource="x"/>
</rdf:Description>
<rdf:Description rdf:about="x"/>
<owl:NegativeObjectPropertyAssertion>
  <owl:sourceIndividual rdf:resource="x"/>
  <owl:assertionProperty rdf:resource="p"/>
  <owl:targetIndividual rdf:resource="y"/>
</owl:NegativeObjectPropertyAssertion>
</rdf:RDF>"##;

/// The swapped pair WITHOUT the negative: q(y,x) alone is harmless —
/// guards against the check firing on the un-swapped orientation.
const INVERSE_WITHOUT_NEGATION: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="p"/>
<owl:ObjectProperty rdf:about="q">
  <owl:inverseOf rdf:resource="p"/>
</owl:ObjectProperty>
<rdf:Description rdf:about="y">
  <q rdf:resource="x"/>
</rdf:Description>
<rdf:Description rdf:about="x"/>
</rdf:RDF>"##;

#[test]
fn inverse_realizing_edge_clashes_the_negative_assertion() {
    let ontology = parse(INVERSE_REALIZED).unwrap();
    let reasoner = owl2_reasoner::reasoner::simple::SimpleReasoner::new(ontology);
    assert!(
        !reasoner.is_consistent().unwrap(),
        "q(y,x) with q inverseOf p realizes p(x,y), contradicting the stated ¬p(x,y)"
    );
}

#[test]
fn inverse_edge_without_negation_stays_consistent() {
    let ontology = parse(INVERSE_WITHOUT_NEGATION).unwrap();
    let reasoner = owl2_reasoner::reasoner::simple::SimpleReasoner::new(ontology);
    assert!(
        reasoner.is_consistent().unwrap(),
        "the inverse edge alone violates nothing"
    );
}

#[test]
fn derived_edge_clashes_the_negative_assertion() {
    let premise = Arc::new(parse(PREMISE_CHAIN).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_EQUIVALENT).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "true",
        "the lowered domain gives x the hasValue edges, functional p merges y≡v, \
         and the derived q(x,y) must clash with the reduction's ¬q(x,y)"
    );
}

#[test]
fn unlinked_properties_leave_the_equivalence_unentailed() {
    let premise = Arc::new(parse(PREMISE_UNLINKED).unwrap());
    let conclusion = Arc::new(parse(CONCLUSION_EQUIVALENT).unwrap());
    let response = evaluate_entailment(premise, conclusion, "premise.owl", "conclusion.owl");
    assert_eq!(
        response.semantic_result.as_str(),
        "false",
        "genuine non-entailment: nothing forces q onto the p-connected pair"
    );
}
