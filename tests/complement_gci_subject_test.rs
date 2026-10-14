use std::sync::Arc;

use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::evaluation::evaluate_entailment;
use owl2_reasoner::Ontology;

/// The dl-205/dl-208(a) shape: an equivalence whose conjuncts are
/// complements of named classes (C6 ≡ ¬C2 ⊓ ¬C4), over an individual
/// asserted into the complements.
fn complement_conjuncts() -> String { format!(r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:Class rdf:about="C6">
  <owl:equivalentClass>
    <owl:Class>
      <owl:intersectionOf rdf:parseType="Collection">
        <owl:Class>
          <owl:complementOf rdf:resource="C2"/>
        </owl:Class>
        <owl:Class>
          <owl:complementOf rdf:resource="C4"/>
        </owl:Class>
      </owl:intersectionOf>
    </owl:Class>
  </owl:equivalentClass>
</owl:Class>
<rdf:Description rdf:about="x">
  <rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C2"/></owl:Class>
  </rdf:type>
  <rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C4"/></owl:Class>
  </rdf:type>
</rdf:Description>
{EXISTENTIAL_DISTRACTOR}</rdf:RDF>"##) }

/// An unrelated existential definition: complement+boolean-only
/// ontologies are decided exactly by the boolean lane, so every
/// fixture below carries this distractor to evict that fragment and
/// force the merge probe (the fact-finder's decisive experiment).
const EXISTENTIAL_DISTRACTOR: &str = "<owl:ObjectProperty rdf:about=\"Pdist\"/>
<owl:Class rdf:about=\"Ddist\">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource=\"Pdist\"/>
      <owl:someValuesFrom rdf:resource=\"C2\"/>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>";



/// The dl-208(b) shape: an equivalence with a negated named filler
/// (C80 ≡ ∃R1.¬C16), over an edge whose target asserts the complement.
fn negated_existential_filler() -> String { format!(r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="R1"/>
<owl:Class rdf:about="C80">
  <owl:equivalentClass>
    <owl:Restriction>
      <owl:onProperty rdf:resource="R1"/>
      <owl:someValuesFrom>
        <owl:Class><owl:complementOf rdf:resource="C16"/></owl:Class>
      </owl:someValuesFrom>
    </owl:Restriction>
  </owl:equivalentClass>
</owl:Class>
<rdf:Description rdf:about="x">
  <R1 rdf:resource="y"/>
</rdf:Description>
<rdf:Description rdf:about="y">
  <rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C16"/></owl:Class>
  </rdf:type>
</rdf:Description>
{EXISTENTIAL_DISTRACTOR}</rdf:RDF>"##) }



/// The dl-201 GCI shape: complement conjuncts on the left of a plain
/// subclass axiom.
fn complement_gci() -> String { format!(r##"<rdf:RDF xmlns="http://example.org/"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
  xml:base="http://example.org/">
<owl:Ontology/>
<owl:Class>
  <owl:intersectionOf rdf:parseType="Collection">
    <owl:Class><owl:complementOf rdf:resource="C102"/></owl:Class>
    <owl:Class><owl:complementOf rdf:resource="C108"/></owl:Class>
  </owl:intersectionOf>
  <rdfs:subClassOf>
    <owl:Class rdf:about="C110"/>
  </rdfs:subClassOf>
</owl:Class>
<rdf:Description rdf:about="x">
  <rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C102"/></owl:Class>
  </rdf:type>
  <rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C108"/></owl:Class>
  </rdf:type>
</rdf:Description>
{EXISTENTIAL_DISTRACTOR}</rdf:RDF>"##) }



fn parse(content: &str) -> owl2_reasoner::OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(content)
}

fn entail(premise: &str, conclusion_class: &str) -> String {
    let conclusion = format!(
        r#"<rdf:RDF xmlns="http://example.org/" xmlns:owl="http://www.w3.org/2002/07/owl#" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xml:base="http://example.org/">
<owl:Ontology/>
<rdf:Description rdf:about="x">
  <rdf:type rdf:resource="{conclusion_class}"/>
</rdf:Description>
</rdf:RDF>"#
    );
    let response = evaluate_entailment(
        Arc::new(parse(premise).unwrap()),
        Arc::new(parse(&conclusion).unwrap()),
        "premise.owl",
        "conclusion.owl",
    );
    response.semantic_result.as_str().to_string()
}

#[test]
fn complement_conjunct_equivalence_derives_the_class() {
    assert_eq!(
        entail(&complement_conjuncts(), "C6"),
        "true",
        "x in both complements satisfies the equivalence's down-direction"
    );
}

#[test]
fn negated_named_fillers_witness_the_existential() {
    assert_eq!(
        entail(&negated_existential_filler(), "C80"),
        "true",
        "an R1-edge to a recorded complement witnesses exists R1.not-C16"
    );
}

#[test]
fn complement_conjunct_gci_derives_the_superclass() {
    assert_eq!(
        entail(&complement_gci(), "C110"),
        "true",
        "the anonymous subject (not-C102 AND not-C108) holds structurally on x"
    );
}

#[test]
fn positive_conjuncts_still_need_their_classes() {
    // The control: without C4's complement recorded, C6 must NOT follow.
    let premise = complement_conjuncts()
        .replace(
            r#"<rdf:type>
    <owl:Class><owl:complementOf rdf:resource="C4"/></owl:Class>
  </rdf:type>"#,
            "",
        );
    assert_eq!(entail(&premise, "C6"), "false");
}
