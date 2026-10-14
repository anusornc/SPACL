use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::Ontology;

/// The 0094 reduction: a functional object property whose two subjects
/// are forced identical (the negated differentFrom conclusion) must
/// collide their fillers against the asserted X1 ≠ X2.
const FUNCTIONAL_COLLISION: &str = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:j.0="http://owl2.test/rules/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:FunctionalProperty rdf:about="http://owl2.test/rules/fp">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#ObjectProperty"/>
</owl:FunctionalProperty>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y2">
  <j.0:fp><owl:NamedIndividual rdf:about="http://owl2.test/rules/X2"/></j.0:fp>
</owl:NamedIndividual>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/X1">
  <owl:differentFrom rdf:resource="http://owl2.test/rules/X2"/>
</owl:NamedIndividual>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y1">
  <j.0:fp rdf:resource="http://owl2.test/rules/X1"/>
  <owl:sameAs rdf:resource="http://owl2.test/rules/Y2"/>
</owl:NamedIndividual>
</rdf:RDF>"#;

/// The 0096 reduction: an inverse-functional property whose two objects
/// are forced identical must collide its subjects against the asserted
/// Y1 ≠ Y2.
const INVERSE_FUNCTIONAL_COLLISION: &str = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:j.0="http://owl2.test/rules/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:InverseFunctionalProperty rdf:about="http://owl2.test/rules/ifp">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#ObjectProperty"/>
</owl:InverseFunctionalProperty>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y2">
  <j.0:ifp><owl:NamedIndividual rdf:about="http://owl2.test/rules/X2"/></j.0:ifp>
</owl:NamedIndividual>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y1">
  <j.0:ifp rdf:resource="http://owl2.test/rules/X1"/>
  <owl:differentFrom rdf:resource="http://owl2.test/rules/Y2"/>
</owl:NamedIndividual>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/X1">
  <owl:sameAs rdf:resource="http://owl2.test/rules/X2"/>
</owl:NamedIndividual>
</rdf:RDF>"#;

/// sameAs with no collision behind it stays consistent — the forced
/// merge must not invent a clash.
const SAMEAS_HARMLESS: &str = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:j.0="http://owl2.test/rules/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:FunctionalProperty rdf:about="http://owl2.test/rules/fp">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#ObjectProperty"/>
</owl:FunctionalProperty>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y2">
  <j.0:fp><owl:NamedIndividual rdf:about="http://owl2.test/rules/X2"/></j.0:fp>
</owl:NamedIndividual>
<owl:NamedIndividual rdf:about="http://owl2.test/rules/Y1">
  <j.0:fp rdf:resource="http://owl2.test/rules/X1"/>
  <owl:sameAs rdf:resource="http://owl2.test/rules/Y2"/>
</owl:NamedIndividual>
</rdf:RDF>"#;

fn parse(content: &str, structural: bool) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

#[test]
fn sameas_pair_forces_functional_filler_collision() {
    for structural in [false, true] {
        let ontology = parse(FUNCTIONAL_COLLISION, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: Y1≡Y2 under functional fp forces X1≡X2 against X1≠X2"
        );
    }
}

#[test]
fn sameas_pair_forces_inverse_functional_subject_collision() {
    for structural in [false, true] {
        let ontology = parse(INVERSE_FUNCTIONAL_COLLISION, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: X1≡X2 under inverse-functional ifp forces Y1≡Y2 against Y1≠Y2"
        );
    }
}

#[test]
fn sameas_pair_without_collision_stays_consistent() {
    for structural in [false, true] {
        let ontology = parse(SAMEAS_HARMLESS, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: a collision-free sameAs must merge, not clash"
        );
    }
}
