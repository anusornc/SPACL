use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The 626 proof core: two interleaved functional merges with a
/// propagation round between them. u = ccomp(min1) ⊓ ∃invF.d ⊓ ∀invF.V3;
/// d = c(max0) ⊓ ∃f.ccomp; V3 = ∃invF.d; f functional. The first merge
/// (u=z) enables V3 on y; the second (y=v) is what puts min1 next to
/// max0 on y — but only when the walk reuses already-satisfied
/// existentials instead of spawning fresh witnesses forever.
const DEPTH_TWO_INTERLEAVING: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>

<owl:DatatypeProperty rdf:about="hasX"/>
<owl:ObjectProperty rdf:about="f">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="invF">
  <owl:inverseOf rdf:resource="f"/>
</owl:ObjectProperty>

<owl:Class rdf:about="c">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">0</owl:maxCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="ccomp">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="d">
  <owl:equivalentClass>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class rdf:about="c"/>
        <owl:Restriction>
          <owl:onProperty rdf:resource="f"/>
          <owl:someValuesFrom rdf:resource="ccomp"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="V3">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="invF"/>
      <owl:someValuesFrom rdf:resource="d"/>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="Unsat">
  <owl:equivalentClass>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class rdf:about="ccomp"/>
        <owl:Restriction>
          <owl:onProperty rdf:resource="invF"/>
          <owl:someValuesFrom rdf:resource="d"/>
        </owl:Restriction>
        <owl:Restriction>
          <owl:onProperty rdf:resource="invF"/>
          <owl:allValuesFrom rdf:resource="V3"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </owl:equivalentClass>
</owl:Class>

<rdf:Description rdf:about="u">
  <rdf:type rdf:resource="Unsat"/>
</rdf:Description>
</rdf:RDF>"##;

/// One merge deep (drop the ∀invF.V3 conjunct): u=z still happens, but
/// nothing ever puts the two bounds on one node — stays consistent.
const ONE_MERGE_CONTROL: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>

<owl:DatatypeProperty rdf:about="hasX"/>
<owl:ObjectProperty rdf:about="f">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="invF">
  <owl:inverseOf rdf:resource="f"/>
</owl:ObjectProperty>

<owl:Class rdf:about="c">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">0</owl:maxCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="ccomp">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="d">
  <owl:equivalentClass>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class rdf:about="c"/>
        <owl:Restriction>
          <owl:onProperty rdf:resource="f"/>
          <owl:someValuesFrom rdf:resource="ccomp"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="Unsat">
  <owl:equivalentClass>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class rdf:about="ccomp"/>
        <owl:Restriction>
          <owl:onProperty rdf:resource="invF"/>
          <owl:someValuesFrom rdf:resource="d"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </owl:equivalentClass>
</owl:Class>

<rdf:Description rdf:about="u">
  <rdf:type rdf:resource="Unsat"/>
</rdf:Description>
</rdf:RDF>"##;

fn parse(content: &str, structural: bool) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

#[test]
fn two_interleaved_functional_merges_reach_the_data_clash() {
    for structural in [false, true] {
        let ontology = parse(DEPTH_TWO_INTERLEAVING, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: the walk must reuse satisfied existentials so the \
             depth-2 functional merges complete (W3C description-logic-626/627)"
        );
    }
}

#[test]
fn single_merge_depth_stays_consistent() {
    for structural in [false, true] {
        let ontology = parse(ONE_MERGE_CONTROL, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: one merge deep never pools the two bounds"
        );
    }
}
