use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The dl-004 proof core: rx3 and rx4 under functional rx; x carries
/// the negated conjunction-existential plus one existential per role.
/// The shared-super merge pools c1 ⊓ c2 on one witness, which the
/// negated check must then clash.
const NEGATED_CONJUNCTION: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="rx">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="rx3">
  <rdfs:subPropertyOf rdf:resource="rx"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="rx4">
  <rdfs:subPropertyOf rdf:resource="rx"/>
</owl:ObjectProperty>
<owl:Class rdf:about="c1"/>
<owl:Class rdf:about="c2"/>
<rdf:Description rdf:about="x">
  <rdf:type>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class>
          <owl:complementOf>
            <owl:Restriction>
              <owl:onProperty rdf:resource="rx3"/>
              <owl:someValuesFrom>
                <owl:Class>
                  <owl:intersectionOf rdf:parseType="Collection">
                    <owl:Class rdf:about="c1"/>
                    <owl:Class rdf:about="c2"/>
                  </owl:intersectionOf>
                </owl:Class>
              </owl:someValuesFrom>
            </owl:Restriction>
          </owl:complementOf>
        </owl:Class>
        <owl:Restriction>
          <owl:onProperty rdf:resource="rx3"/>
          <owl:someValuesFrom rdf:resource="c1"/>
        </owl:Restriction>
        <owl:Restriction>
          <owl:onProperty rdf:resource="rx4"/>
          <owl:someValuesFrom rdf:resource="c2"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </rdf:type>
</rdf:Description>
</rdf:RDF>"##;

/// Without the negated restriction the same walk stays peaceful.
const NO_NEGATION: &str = r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="rx">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="rx3">
  <rdfs:subPropertyOf rdf:resource="rx"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="rx4">
  <rdfs:subPropertyOf rdf:resource="rx"/>
</owl:ObjectProperty>
<owl:Class rdf:about="c1"/>
<owl:Class rdf:about="c2"/>
<rdf:Description rdf:about="x">
  <rdf:type>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Restriction>
          <owl:onProperty rdf:resource="rx3"/>
          <owl:someValuesFrom rdf:resource="c1"/>
        </owl:Restriction>
        <owl:Restriction>
          <owl:onProperty rdf:resource="rx4"/>
          <owl:someValuesFrom rdf:resource="c2"/>
        </owl:Restriction>
      </owl:intersectionOf>
    </owl:Class>
  </rdf:type>
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
fn merged_witness_clashes_the_negated_conjunction() {
    for structural in [false, true] {
        let ontology = parse(NEGATED_CONJUNCTION, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: the shared-super merge pools c1 AND c2 on one \
             witness, realizing the negated exists rx3.(c1 AND c2) (W3C dl-004)"
        );
    }
}

#[test]
fn existentials_without_negation_stay_consistent() {
    for structural in [false, true] {
        let ontology = parse(NO_NEGATION, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: nothing negated, the merge is harmless"
        );
    }
}
