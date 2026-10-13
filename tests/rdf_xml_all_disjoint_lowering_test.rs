use owl2_reasoner::Entity;
use owl2_reasoner::IRI;
use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::reasoner::simple::SimpleReasoner;
use owl2_reasoner::{Ontology, OwlResult};

/// The W3C DisjointClasses-003 premise shape: an element-form
/// owl:AllDisjointClasses carrying an owl:members collection.
const ALL_DISJOINT_CLASSES: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:Class rdf:about="Boy" />
<owl:Class rdf:about="Girl" />
<owl:Class rdf:about="Dog" />
<owl:AllDisjointClasses>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="Boy" />
    <rdf:Description rdf:about="Girl" />
    <rdf:Description rdf:about="Dog" />
  </owl:members>
</owl:AllDisjointClasses>
<Boy rdf:about="Stewie" />
</rdf:RDF>"#;

/// The W3C DisjointDataProperties-002 premise shape: declared datatype
/// properties inside an owl:AllDisjointProperties members collection.
const ALL_DISJOINT_DATA_PROPERTIES: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:DatatypeProperty rdf:about="hasName" />
<owl:DatatypeProperty rdf:about="hasAddress" />
<owl:AllDisjointProperties>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="hasName" />
    <rdf:Description rdf:about="hasAddress" />
  </owl:members>
</owl:AllDisjointProperties>
</rdf:RDF>"#;

/// The object flavor of the same construct (DisjointObjectProperties-002).
const ALL_DISJOINT_OBJECT_PROPERTIES: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:ObjectProperty rdf:about="fatherOf" />
<owl:ObjectProperty rdf:about="motherOf" />
<owl:AllDisjointProperties>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="fatherOf" />
    <rdf:Description rdf:about="motherOf" />
  </owl:members>
</owl:AllDisjointProperties>
</rdf:RDF>"#;

/// The legacy AllDifferent spelling (WebOnt one_two): a bare
/// rdf:Description typed owl:AllDifferent with owl:distinctMembers.
const ALL_DIFFERENT_DISTINCT_MEMBERS: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<rdf:Description>
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#AllDifferent"/>
  <owl:distinctMembers rdf:parseType="Collection">
    <rdf:Description rdf:about="http://example.org/j" />
    <rdf:Description rdf:about="http://example.org/k" />
    <rdf:Description rdf:about="http://example.org/i" />
  </owl:distinctMembers>
</rdf:Description>
</rdf:RDF>"#;

/// The modern AllDifferent spelling (DisjointDataProperties-002's
/// conclusion): element form with owl:members.
const ALL_DIFFERENT_MEMBERS: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:AllDifferent>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="Peter" />
    <rdf:Description rdf:about="Peter_Griffin" />
  </owl:members>
</owl:AllDifferent>
</rdf:RDF>"#;

fn parse(content: &str, structural: bool) -> OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

fn iri_set(values: &[std::sync::Arc<IRI>]) -> Vec<String> {
    let mut iris: Vec<String> = values.iter().map(|v| v.as_str().to_owned()).collect();
    iris.sort();
    iris
}

#[test]
fn all_disjoint_classes_members_lower_to_one_axiom() {
    for structural in [false, true] {
        let ontology = parse(ALL_DISJOINT_CLASSES, structural).unwrap();
        let axioms = ontology.disjoint_classes_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DisjointClasses axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].classes()),
            vec![
                "http://example.org/Boy",
                "http://example.org/Dog",
                "http://example.org/Girl",
            ]
        );
        // The marker node must not survive as a typed-individual residue.
        for assertion in ontology.class_assertions() {
            if let owl2_reasoner::ClassExpression::Class(class) = assertion.class_expr() {
                assert_ne!(
                    class.iri().as_str(),
                    "http://www.w3.org/2002/07/owl#AllDisjointClasses",
                    "structural={structural}: marker ClassAssertion residue"
                );
            }
        }
    }
}

#[test]
fn all_disjoint_data_properties_members_lower_by_declaration() {
    for structural in [false, true] {
        let ontology = parse(ALL_DISJOINT_DATA_PROPERTIES, structural).unwrap();
        let axioms = ontology.disjoint_data_properties_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DisjointDataProperties axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].properties()),
            vec![
                "http://example.org/hasAddress",
                "http://example.org/hasName",
            ]
        );
        assert!(ontology.disjoint_object_properties_axioms().is_empty());
    }
}

#[test]
fn all_disjoint_object_properties_members_lower_by_declaration() {
    for structural in [false, true] {
        let ontology = parse(ALL_DISJOINT_OBJECT_PROPERTIES, structural).unwrap();
        let axioms = ontology.disjoint_object_properties_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DisjointObjectProperties axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].properties()),
            vec![
                "http://example.org/fatherOf",
                "http://example.org/motherOf",
            ]
        );
        assert!(ontology.disjoint_data_properties_axioms().is_empty());
    }
}

#[test]
fn all_different_distinct_members_lower_to_one_axiom() {
    for structural in [false, true] {
        let ontology = parse(ALL_DIFFERENT_DISTINCT_MEMBERS, structural).unwrap();
        let axioms = ontology.different_individuals_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DifferentIndividuals axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].individuals()),
            vec![
                "http://example.org/i",
                "http://example.org/j",
                "http://example.org/k",
            ]
        );
    }
}

#[test]
fn all_different_members_predicate_also_lowers() {
    for structural in [false, true] {
        let ontology = parse(ALL_DIFFERENT_MEMBERS, structural).unwrap();
        let axioms = ontology.different_individuals_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DifferentIndividuals axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].individuals()),
            vec!["http://example.org/Peter", "http://example.org/Peter_Griffin",]
        );
    }
}

#[test]
fn disjoint_classes_premise_plus_member_type_is_inconsistent() {
    // The 0022 entailment's consistency reduction: premise plus the
    // negated conclusion (Stewie: Girl) must clash through the parsed
    // disjointness.
    for structural in [false, true] {
        let mut ontology = parse(ALL_DISJOINT_CLASSES, structural).unwrap();
        assert_class(
            &mut ontology,
            "http://example.org/Stewie",
            "http://example.org/Girl",
        );
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            !reasoner.is_consistent().unwrap(),
            "structural={structural}: Boy disjoint Girl must clash with Stewie: Girl"
        );
    }
}

/// The Description-form spelling of the AllDisjointClasses marker: a
/// bare rdf:Description carrying the rdf:type triple explicitly.
const ALL_DISJOINT_CLASSES_DESCRIPTION_FORM: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<rdf:Description>
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#AllDisjointClasses"/>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="Boy" />
    <rdf:Description rdf:about="Girl" />
  </owl:members>
</rdf:Description>
</rdf:RDF>"#;

/// The Description-form spelling of the AllDisjointProperties marker.
const ALL_DISJOINT_PROPERTIES_DESCRIPTION_FORM: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<owl:Ontology/>
<owl:DatatypeProperty rdf:about="hasName" />
<owl:DatatypeProperty rdf:about="hasAddress" />
<rdf:Description>
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#AllDisjointProperties"/>
  <owl:members rdf:parseType="Collection">
    <rdf:Description rdf:about="hasName" />
    <rdf:Description rdf:about="hasAddress" />
  </owl:members>
</rdf:Description>
</rdf:RDF>"#;

#[test]
fn all_disjoint_classes_description_form_marker_lowers() {
    for structural in [false, true] {
        let ontology = parse(ALL_DISJOINT_CLASSES_DESCRIPTION_FORM, structural).unwrap();
        let axioms = ontology.disjoint_classes_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DisjointClasses axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].classes()),
            vec!["http://example.org/Boy", "http://example.org/Girl",]
        );
    }
}

#[test]
fn all_disjoint_properties_description_form_marker_lowers() {
    for structural in [false, true] {
        let ontology = parse(ALL_DISJOINT_PROPERTIES_DESCRIPTION_FORM, structural).unwrap();
        let axioms = ontology.disjoint_data_properties_axioms();
        assert_eq!(
            axioms.len(),
            1,
            "structural={structural}: one DisjointDataProperties axiom expected"
        );
        assert_eq!(
            iri_set(axioms[0].properties()),
            vec![
                "http://example.org/hasAddress",
                "http://example.org/hasName",
            ]
        );
    }
}

/// The W3C miscellaneous-001 soundness core: a wine made from a grape
/// outside the oneOf of a `Wine ⊓ ∀madeFromGrape.{...}` equivalence
/// must not be classified into that equivalence (which would force its
/// functional color to be both White and Red over an AllDifferent).
const MIXED_VACUOUS_GCI_WINE: &str = r#"<?xml version="1.0"?>
<rdf:RDF
  xml:base  = "http://example.org/"
  xmlns     = "http://example.org/"
  xmlns:owl = "http://www.w3.org/2002/07/owl#"
  xmlns:rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:rdfs= "http://www.w3.org/2000/01/rdf-schema#">
<owl:Ontology/>

<owl:ObjectProperty rdf:about="hasColor">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
</owl:ObjectProperty>
<owl:ObjectProperty rdf:about="madeFromGrape"/>

<owl:Class rdf:about="Wine">
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasColor"/>
      <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:cardinality>
    </owl:Restriction>
  </rdfs:subClassOf>
</owl:Class>

<owl:Class rdf:about="RedZinfandel">
  <rdfs:subClassOf rdf:resource="Wine"/>
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasColor"/>
      <owl:hasValue rdf:resource="Red"/>
    </owl:Restriction>
  </rdfs:subClassOf>
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="madeFromGrape"/>
      <owl:hasValue rdf:resource="CabernetGrape"/>
    </owl:Restriction>
  </rdfs:subClassOf>
</owl:Class>

<owl:Class rdf:about="SemillonOrSauvignonBlanc">
  <rdfs:subClassOf>
    <owl:Restriction>
      <owl:onProperty rdf:resource="hasColor"/>
      <owl:hasValue rdf:resource="White"/>
    </owl:Restriction>
  </rdfs:subClassOf>
  <owl:intersectionOf rdf:parseType="Collection">
    <owl:Class rdf:about="Wine"/>
    <owl:Restriction>
      <owl:onProperty rdf:resource="madeFromGrape"/>
      <owl:allValuesFrom>
        <owl:Class>
          <owl:oneOf rdf:parseType="Collection">
            <rdf:Description rdf:about="SemillonGrape"/>
            <rdf:Description rdf:about="SauvignonBlancGrape"/>
          </owl:oneOf>
        </owl:Class>
      </owl:allValuesFrom>
    </owl:Restriction>
  </owl:intersectionOf>
</owl:Class>

<owl:AllDifferent>
  <owl:distinctMembers rdf:parseType="Collection">
    <rdf:Description rdf:about="White"/>
    <rdf:Description rdf:about="Red"/>
  </owl:distinctMembers>
</owl:AllDifferent>

<RedZinfandel rdf:about="AZinfandel"/>
</rdf:RDF>"#;

#[test]
fn mixed_vacuous_subject_gci_never_classifies_an_outsider_grape_wine() {
    // The mixed subject `Wine ⊓ ∀madeFromGrape.{Semillon, Sauvignon}`
    // must only fire after the node's walk settled: AZinfandel uses
    // madeFromGrape (to CabernetGrape), so the vacuous reading of the
    // universal cannot hold and the White-color equivalence must stay
    // away from it (W3C miscellaneous-001 soundness).
    for structural in [false, true] {
        let ontology = parse(MIXED_VACUOUS_GCI_WINE, structural).unwrap();
        let reasoner = SimpleReasoner::new(ontology);
        assert!(
            reasoner.is_consistent().unwrap(),
            "structural={structural}: an outsider-grape wine keeps one color"
        );
    }
}

/// Assert one individual into one named class — the shape the 0022
/// reduction's negated conclusion takes.
fn assert_class(ontology: &mut Ontology, individual: &str, class: &str) {
    ontology
        .add_class_assertion(owl2_reasoner::logic::axioms::ClassAssertionAxiom::new(
            IRI::from(individual).into(),
            owl2_reasoner::ClassExpression::Class(owl2_reasoner::Class::from_shared_iri(
                IRI::from(class).into(),
            )),
        ))
        .unwrap();
}
