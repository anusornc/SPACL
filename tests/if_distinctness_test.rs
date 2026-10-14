use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The one_two distinctness core: p and q are AllDifferent; if is
/// inverse-functional, so x—if→p and y—if→q force x ≠ y; the
/// functional g then cannot merge s's two successors.
fn fixture(declare_all_different: bool) -> String {
    let all_different = if declare_all_different {
        r#"<owl:AllDifferent>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="p"/>
    <rdf:Description rdf:about="q"/>
  </owl:members>
</owl:AllDifferent>"#
    } else {
        ""
    };
    format!(
        r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="if">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="g">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
{all_different}
<rdf:Description rdf:about="x"><if rdf:resource="p"/></rdf:Description>
<rdf:Description rdf:about="y"><if rdf:resource="q"/></rdf:Description>
<rdf:Description rdf:about="s">
  <g rdf:resource="x"/>
  <g rdf:resource="y"/>
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
fn inverse_functional_injectivity_derives_distinctness() {
    for structural in [false, true] {
        let ontology = parse(&fixture(true), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: p ≠ q under inverse-functional if makes x ≠ y, \\\\
             so s's two functional g-successors cannot merge (W3C one_two's core hop)"
        );
    }
}

#[test]
fn unrelated_targets_leave_the_merge_open() {
    for structural in [false, true] {
        let ontology = parse(&fixture(false), structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: without the AllDifferent, x and y may merge"
        );
    }
}
