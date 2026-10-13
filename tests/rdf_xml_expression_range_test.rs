use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::{ClassExpression, Ontology, OwlResult};

/// The W3C oneOf-004 premise shape: a datatype property whose two
/// rdfs:range objects are owl:DataRange/oneOf enumerations over nested
/// rdf:List cells.
const DATA_ONE_OF_RANGES: &str = r#"<?xml version="1.0"?>
<!DOCTYPE rdf:RDF [
  <!ENTITY xsd "http://www.w3.org/2001/XMLSchema#">
]>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xmlns:xsd="http://www.w3.org/2001/XMLSchema#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:DatatypeProperty rdf:ID="p">
  <rdfs:range>
   <owl:DataRange>
    <owl:oneOf>
     <rdf:List>
      <rdf:first rdf:datatype="&xsd;integer">1</rdf:first>
      <rdf:rest>
       <rdf:List>
        <rdf:first rdf:datatype="&xsd;integer">2</rdf:first>
        <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
       </rdf:List>
      </rdf:rest>
     </rdf:List>
    </owl:oneOf>
   </owl:DataRange>
  </rdfs:range>
  <rdfs:range>
   <owl:DataRange>
    <owl:oneOf>
     <rdf:List>
      <rdf:first rdf:datatype="&xsd;integer">2</rdf:first>
      <rdf:rest>
       <rdf:List>
        <rdf:first rdf:datatype="&xsd;integer">3</rdf:first>
        <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
       </rdf:List>
      </rdf:rest>
     </rdf:List>
    </owl:oneOf>
   </owl:DataRange>
  </rdfs:range>
</owl:DatatypeProperty>
</rdf:RDF>"#;

/// The W3C TransitiveProperty-002 premise shape: a symmetric property
/// whose rdfs:range object is a oneOf class expression spelled with
/// parseType=Collection.
const OBJECT_ONE_OF_RANGE: &str = r#"<?xml version="1.0"?>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:SymmetricProperty rdf:ID="symProp">
  <rdfs:range>
    <owl:Class>
      <owl:oneOf rdf:parseType="Collection">
        <owl:Thing rdf:about="http://example.org/a"/>
        <owl:Thing rdf:about="http://example.org/b"/>
      </owl:oneOf>
    </owl:Class>
  </rdfs:range>
</owl:SymmetricProperty>
</rdf:RDF>"#;

/// Control: the plain IRI-object range keeps its existing lowering.
const IRI_RANGE_CONTROL: &str = r#"<?xml version="1.0"?>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:ObjectProperty rdf:ID="q">
  <rdfs:range rdf:resource="http://example.org/C"/>
</owl:ObjectProperty>
</rdf:RDF>"#;

fn parse(text: &str) -> OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content(text)
}

fn parse_structural(text: &str) -> OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    parser.parse_content_structural(text)
}

#[test]
fn data_one_of_ranges_become_enumerated_range_axioms() -> OwlResult<()> {
    let ontology = parse(DATA_ONE_OF_RANGES)?;
    let ranges = ontology.data_property_enumerated_range_axioms();
    assert_eq!(
        ranges.len(),
        2,
        "both DataOneOf ranges must survive the parse"
    );
    let pooled: Vec<Vec<String>> = ranges
        .iter()
        .map(|axiom| {
            assert_eq!(
                axiom.property().as_str(),
                "http://example.org/#p",
                "the range's property is the frame subject"
            );
            axiom.values().iter().map(|v| v.lexical_form().to_string()).collect()
        })
        .collect();
    assert!(pooled.contains(&vec!["1".to_string(), "2".to_string()]));
    assert!(pooled.contains(&vec!["2".to_string(), "3".to_string()]));
    Ok(())
}

#[test]
fn data_one_of_ranges_survive_the_structural_path() -> OwlResult<()> {
    let ontology = parse_structural(DATA_ONE_OF_RANGES)?;
    assert_eq!(
        ontology.data_property_enumerated_range_axioms().len(),
        2,
        "the structural path lowers both DataOneOf ranges too"
    );
    Ok(())
}

#[test]
fn object_expression_range_lowers_to_inverse_existential_gci() -> OwlResult<()> {
    let ontology = parse(OBJECT_ONE_OF_RANGE)?;
    let subclasses = ontology.subclass_axioms();
    assert_eq!(
        subclasses.len(),
        1,
        "the expression range lowers to exactly one GCI"
    );
    let axiom = subclasses[0];
    let ClassExpression::ObjectSomeValuesFrom(role, filler) = axiom.sub_class() else {
        panic!(
            "the range reading is an inverse-role existential, got {:?}",
            axiom.sub_class()
        );
    };
    let owl2_reasoner::ObjectPropertyExpression::ObjectInverseOf(inner) = role.as_ref() else {
        panic!("the existential role must be the property inverse, got {role:?}");
    };
    let owl2_reasoner::ObjectPropertyExpression::ObjectProperty(property) = inner.as_ref() else {
        panic!("the inverted role must be a named property, got {inner:?}");
    };
    assert_eq!(property.iri().as_str(), "http://example.org/#symProp");
    assert!(
        matches!(&**filler, ClassExpression::Class(thing)
            if thing.iri().as_str() == "http://www.w3.org/2002/07/owl#Thing"),
        "the existential filler is owl:Thing, got {filler:?}"
    );
    assert!(
        matches!(axiom.super_class(), ClassExpression::ObjectOneOf(members)
            if members.len() == 2),
        "the GCI's super side is the oneOf enumeration, got {:?}",
        axiom.super_class()
    );
    assert_eq!(
        ontology.object_property_range_axioms().len(),
        0,
        "an expression range must not also emit the IRI-range axiom"
    );
    Ok(())
}

#[test]
fn object_expression_range_lowers_on_the_structural_path() -> OwlResult<()> {
    let ontology = parse_structural(OBJECT_ONE_OF_RANGE)?;
    let subclasses = ontology.subclass_axioms();
    assert_eq!(
        subclasses.len(),
        1,
        "the structural path lowers the expression range to one GCI"
    );
    assert!(
        matches!(subclasses[0].sub_class(), ClassExpression::ObjectSomeValuesFrom(..)),
        "the inverse-existential shape holds on the structural path"
    );
    Ok(())
}

#[test]
fn iri_range_control_keeps_object_property_range() -> OwlResult<()> {
    let ontology = parse(IRI_RANGE_CONTROL)?;
    let ranges = ontology.object_property_range_axioms();
    assert_eq!(ranges.len(), 1, "the IRI range still lowers as before");
    assert_eq!(ranges[0].property().as_str(), "http://example.org/#q");
    assert_eq!(ranges[0].range().as_str(), "http://example.org/C");
    Ok(())
}

/// The declaration can trail the range triple in document order — the
/// deferral must resolve against the finalized state, not the
/// mid-document one.
const RANGE_BEFORE_DECLARATION: &str = r#"<?xml version="1.0"?>
<!DOCTYPE rdf:RDF [
  <!ENTITY xsd "http://www.w3.org/2001/XMLSchema#">
]>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:DatatypeProperty rdf:about="http://example.org/late">
  <rdfs:range>
   <owl:DataRange>
    <owl:oneOf>
     <rdf:List>
      <rdf:first rdf:datatype="&xsd;integer">7</rdf:first>
      <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
     </rdf:List>
    </owl:oneOf>
   </owl:DataRange>
  </rdfs:range>
</owl:DatatypeProperty>
<rdf:Description rdf:about="http://example.org/late">
  <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#DatatypeProperty"/>
</rdf:Description>
</rdf:RDF>"#;

#[test]
fn data_one_of_range_resolves_regardless_of_declaration_order() -> OwlResult<()> {
    let ontology = parse(RANGE_BEFORE_DECLARATION)?;
    let ranges = ontology.data_property_enumerated_range_axioms();
    assert_eq!(ranges.len(), 1, "the deferred range survives reordering");
    assert_eq!(ranges[0].values().len(), 1);
    Ok(())
}

/// A facet-restricted datatype range lowers to its base datatype —
/// recorded incompleteness of the facets, never a drop.
const FACETTED_DATATYPE_RANGE: &str = r#"<?xml version="1.0"?>
<!DOCTYPE rdf:RDF [
  <!ENTITY xsd "http://www.w3.org/2001/XMLSchema#">
]>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xmlns:xsd="http://www.w3.org/2001/XMLSchema#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:DatatypeProperty rdf:about="http://example.org/age">
  <rdfs:range>
   <rdfs:Datatype>
    <owl:onDatatype rdf:resource="&xsd;integer"/>
    <owl:withRestrictions rdf:parseType="Collection">
      <rdf:Description>
        <xsd:minInclusive rdf:datatype="&xsd;integer">0</xsd:minInclusive>
      </rdf:Description>
    </owl:withRestrictions>
   </rdfs:Datatype>
  </rdfs:range>
</owl:DatatypeProperty>
</rdf:RDF>"#;

#[test]
fn facet_restricted_datatype_range_lowers_to_base_datatype() -> OwlResult<()> {
    let ontology = parse(FACETTED_DATATYPE_RANGE)?;
    let ranges = ontology.data_property_range_axioms();
    assert_eq!(
        ranges.len(),
        1,
        "the faceted range lowers to a plain datatype range"
    );
    assert_eq!(
        ranges[0].range().as_str(),
        "http://www.w3.org/2001/XMLSchema#integer"
    );
    Ok(())
}

/// A malformed oneOf data range on a declared datatype property must
/// surface as a parse error, not a silent drop.
const MALFORMED_ONE_OF_RANGE: &str = r#"<?xml version="1.0"?>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xml:base="http://example.org/">
<owl:Ontology/>
<owl:DatatypeProperty rdf:about="http://example.org/bad">
  <rdfs:range>
   <owl:DataRange>
    <owl:oneOf rdf:parseType="Collection">
      <rdf:Description rdf:about="http://example.org/notALiteral"/>
    </owl:oneOf>
   </owl:DataRange>
  </rdfs:range>
</owl:DatatypeProperty>
</rdf:RDF>"#;

#[test]
fn malformed_data_one_of_range_surfaces_as_parse_error() {
    let result = parse(MALFORMED_ONE_OF_RANGE);
    assert!(
        result.is_err(),
        "a non-literal item in a data oneOf range must fail the parse, got {result:?}"
    );
}

/// An undeclared property whose oneOf list is literal-valued is
/// data-shaped by sniffing alone.
const UNDECLARED_DATA_SHAPED_RANGE: &str = r#"<?xml version="1.0"?>
<!DOCTYPE rdf:RDF [
  <!ENTITY xsd "http://www.w3.org/2001/XMLSchema#">
]>
<rdf:RDF
 xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
 xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
 xmlns:owl="http://www.w3.org/2002/07/owl#"
 xml:base="http://example.org/">
<owl:Ontology/>
<rdf:Description rdf:about="http://example.org/sniffed">
  <rdfs:range>
   <owl:DataRange>
    <owl:oneOf>
     <rdf:List>
      <rdf:first rdf:datatype="&xsd;string">x</rdf:first>
      <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
     </rdf:List>
    </owl:oneOf>
   </owl:DataRange>
  </rdfs:range>
</rdf:Description>
</rdf:RDF>"#;

#[test]
fn undeclared_literal_one_of_range_sniffs_to_data() -> OwlResult<()> {
    let ontology = parse(UNDECLARED_DATA_SHAPED_RANGE)?;
    let ranges = ontology.data_property_enumerated_range_axioms();
    assert_eq!(
        ranges.len(),
        1,
        "the literal oneOf range routes to the data side by shape"
    );
    assert_eq!(ranges[0].property().as_str(), "http://example.org/sniffed");
    Ok(())
}
