use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The dl-035 spy-point core: an Unsatisfiable instance demands three
/// distinct r-fillers; every individual reaches spy through p; spy
/// admits at most two invP-predecessors. p inverseOf invP makes the
/// three r-witnesses three invP-successors of spy — one too many.
fn spy_point(spy_max: u32) -> String {
    format!(
        r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="r"/>
<owl:ObjectProperty rdf:about="p">
  <owl:inverseOf rdf:resource="invP"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="invP"/>
<owl:Class rdf:about="Unsatisfiable">
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="r"/>
      <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">3</owl:minCardinality>
    </owl:Restriction>
  </rdfs:subClassOf>
</owl:Class>
<owl:Class rdf:about="http://www.w3.org/2002/07/owl#Thing">
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="p"/>
      <owl:someValuesFrom>
        <owl:Class>
          <owl:oneOf rdf:parseType="Collection">
            <owl:Thing rdf:about="spy"/>
          </owl:oneOf>
        </owl:Class>
      </owl:someValuesFrom>
    </owl:Restriction>
  </rdfs:subClassOf>
</owl:Class>
<rdf:Description rdf:about="spy">
  <rdf:type>
    <owl:Restriction>
      <owl:onProperty rdf:resource="invP"/>
      <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">{spy_max}</owl:maxCardinality>
    </owl:Restriction>
  </rdf:type>
</rdf:Description>
<rdf:Description rdf:about="x">
  <rdf:type rdf:resource="Unsatisfiable"/>
</rdf:Description>
</rdf:RDF>"##
    )
}

fn parse(content: &str, structural: bool) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

#[test]
fn three_witnesses_cannot_fit_under_the_two_successor_spy() {
    for structural in [false, true] {
        let ontology = parse(&spy_point(2), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: three must-differ r-witnesses all reach spy \
             through p, giving spy three invP-successors against ≤2 (W3C dl-035)"
        );
    }
}

#[test]
fn a_roomy_spy_stays_consistent() {
    for structural in [false, true] {
        let ontology = parse(&spy_point(3), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: ≤3 admits the three witnesses"
        );
    }
}
