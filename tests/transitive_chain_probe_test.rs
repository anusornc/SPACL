use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The 629/632 proof core: a transitive path a–r→b–r→c lets c's inverse
/// universal see a, forcing a into the data-min class against its own
/// data-max class — a clash the probe only finds through the composed
/// edge a–r→c.
fn fixture(declare_transitive: bool) -> String {
    let transitive = if declare_transitive {
        r#"<rdf:type rdf:resource="http://www.w3.org/2002/07/owl#TransitiveProperty"/>"#
    } else {
        ""
    };
    format!(
        r#"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>

<owl:DatatypeProperty rdf:about="hasX"/>

<owl:ObjectProperty rdf:about="r">
  {transitive}
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="invR">
  <owl:inverseOf rdf:resource="r"/>
</owl:ObjectProperty>

<owl:Class rdf:about="Zero">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:maxCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">0</owl:maxCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="One">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasX"/>
      <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:minCardinality>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<owl:Class rdf:about="AllInvROne">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="invR"/>
      <owl:allValuesFrom rdf:resource="One"/>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>

<rdf:Description rdf:about="a">
  <rdf:type rdf:resource="Zero"/>
  <r rdf:resource="b"/>
</rdf:Description>
<rdf:Description rdf:about="b">
  <r rdf:resource="c"/>
</rdf:Description>
<rdf:Description rdf:about="c">
  <rdf:type rdf:resource="AllInvROne"/>
</rdf:Description>
</rdf:RDF>"#
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
fn transitive_path_composes_into_the_inverse_universal() {
    for structural in [false, true] {
        let ontology = parse(&fixture(true), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: a–r→b–r→c with r transitive forces a–r→c, \
             so c's ∀invR.One gives a the min-1 clash against Zero's max-0"
        );
    }
}

#[test]
fn non_transitive_path_leaves_the_far_end_unseen() {
    for structural in [false, true] {
        let ontology = parse(&fixture(false), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: without transitivity c's ∀invR sees only b, \
             and a keeps its max-0 in peace"
        );
    }
}
