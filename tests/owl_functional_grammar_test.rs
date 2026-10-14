//! Regression fixtures for the OWL functional-syntax parser grammar
//! coverage exposed by the ORE 2015 corpus (#194).
//!
//! Each fixture is a minimal, self-contained functional-syntax document
//! exercising one construct in the positions the corpus exercises. The
//! corpus itself is gitignored; these fixtures are the committed
//! regression surface.

use owl2_reasoner::logic::axioms::PropertyAssertionObject;
use owl2_reasoner::parser::OwlFunctionalSyntaxParser;
use owl2_reasoner::parser::OntologyParser;

fn parse(input: &str) -> owl2_reasoner::core::ontology::Ontology {
    let parser = OwlFunctionalSyntaxParser::new();
    parser
        .parse_str(input)
        .unwrap_or_else(|e| panic!("parse failed for input:\n{input}\nerror: {e}"))
}

fn parse_err(input: &str) -> String {
    let parser = OwlFunctionalSyntaxParser::new();
    match parser.parse_str(input) {
        Ok(_) => panic!("expected parse failure for input:\n{input}"),
        Err(e) => format!("{}", e),
    }
}

const HEADER: &str = "\
Prefix(owl:=<http://www.w3.org/2002/07/owl#>)
Prefix(xsd:=<http://www.w3.org/2001/XMLSchema#>)
Prefix(rdfs:=<http://www.w3.org/2000/01/rdf-schema#>)
Prefix(:=<http://example.org/ontology#>)

Ontology(<http://example.org/ontology>
";

fn doc(body: &str) -> String {
    format!("{}{})\n", HEADER, body)
}

// ---------------------------------------------------------------------------
// Object restrictions in class-expression positions (#194 error table)
// ---------------------------------------------------------------------------

#[test]
fn object_all_values_from_as_superclass() {
    let ontology = parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
    assert_eq!(ontology.axioms().len(), 1);
}

#[test]
fn object_all_values_from_as_subclass() {
    parse(&doc(
        "SubClassOf(ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#B>) <http://example.org/ontology#A>)",
    ));
}

#[test]
fn object_all_values_from_nested_in_intersection() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectIntersectionOf(<http://example.org/ontology#B> ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#C>)))",
    ));
}

#[test]
fn object_all_values_from_nested_filler() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectSomeValuesFrom(<http://example.org/ontology#p> ObjectAllValuesFrom(<http://example.org/ontology#q> <http://example.org/ontology#B>)))",
    ));
}

#[test]
fn object_all_values_from_as_disjoint_classes_argument() {
    parse(&doc(
        "DisjointClasses(<http://example.org/ontology#A> ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn object_min_cardinality_unqualified() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectMinCardinality(3 <http://example.org/ontology#p>))",
    ));
}

#[test]
fn object_min_cardinality_qualified() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectMinCardinality(3 <http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn object_max_cardinality_qualified() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectMaxCardinality(2 <http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn object_exact_cardinality_qualified() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectExactCardinality(1 <http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn cardinality_number_is_not_mangled_by_tokenizer() {
    // The tokenizer used to drop the first digit of positive numbers
    // and prepend a minus ("10" became "-0"). The parsed cardinality
    // must be preserved; a two-digit cardinality in the sub-expression
    // proves the first digit survives.
    let ontology = parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectMinCardinality(10 <http://example.org/ontology#p>))",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::SubClassOf(sub) = axiom.as_ref() else {
        panic!("expected SubClassOf axiom");
    };
    let owl2_reasoner::logic::axioms::ClassExpression::ObjectMinCardinality(n, _, _) =
        sub.super_class()
    else {
        panic!("expected ObjectMinCardinality");
    };
    assert_eq!(*n, 10);
}

#[test]
fn object_has_value_with_named_individual() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectHasValue(<http://example.org/ontology#p> <http://example.org/ontology#i>))",
    ));
}

#[test]
fn object_has_value_with_anonymous_individual() {
    // The corpus exercises ObjectHasValue with _:genid values.
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectHasValue(<http://example.org/ontology#p> _:genid141))",
    ));
}

#[test]
fn object_has_self() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectHasSelf(<http://example.org/ontology#p>))",
    ));
}

#[test]
fn object_one_of() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectOneOf(<http://example.org/ontology#i1> <http://example.org/ontology#i2>))",
    ));
}

#[test]
fn object_some_values_from_over_inverse_property() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> ObjectSomeValuesFrom(ObjectInverseOf(<http://example.org/ontology#p>) <http://example.org/ontology#B>))",
    ));
}

// ---------------------------------------------------------------------------
// Data restrictions (family audited alongside DataHasValue)
// ---------------------------------------------------------------------------

#[test]
fn data_has_value() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataHasValue(<http://example.org/ontology#dp> \"3\"^^xsd:integer))",
    ));
}

#[test]
fn data_some_values_from_datatype() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataSomeValuesFrom(<http://example.org/ontology#dp> xsd:integer))",
    ));
}

#[test]
fn data_all_values_from_datatype() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataAllValuesFrom(<http://example.org/ontology#dp> xsd:string))",
    ));
}

#[test]
fn data_min_cardinality() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataMinCardinality(1 <http://example.org/ontology#dp> xsd:integer))",
    ));
}

#[test]
fn data_max_cardinality() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataMaxCardinality(2 <http://example.org/ontology#dp> xsd:integer))",
    ));
}

#[test]
fn data_exact_cardinality() {
    parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataExactCardinality(1 <http://example.org/ontology#dp> xsd:integer))",
    ));
}

#[test]
fn data_one_of_data_range() {
    parse(&doc(
        "DataPropertyRange(<http://example.org/ontology#dp> DataOneOf(\"a\"^^xsd:string \"b\"^^xsd:string))",
    ));
}

#[test]
fn datatype_restriction_in_data_some_values_from() {
    let ontology = parse(&doc(
        "SubClassOf(<http://example.org/ontology#A> DataSomeValuesFrom(<http://example.org/ontology#dp> DatatypeRestriction(xsd:integer xsd:minExclusive \"6000\"^^xsd:integer xsd:maxExclusive \"7000\"^^xsd:integer)))",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::SubClassOf(sub) = axiom.as_ref() else {
        panic!("expected SubClassOf");
    };
    let owl2_reasoner::logic::axioms::ClassExpression::DataSomeValuesFrom(_, data_range) =
        sub.super_class()
    else {
        panic!("expected DataSomeValuesFrom");
    };
    match data_range.as_ref() {
        owl2_reasoner::logic::axioms::class_expressions::DataRange::DatatypeRestriction(datatype, facets) => {
            assert_eq!(datatype.as_str(), "http://www.w3.org/2001/XMLSchema#integer");
            assert_eq!(facets.len(), 2);
        }
        other => panic!("expected DatatypeRestriction data range, got {other:?}"),
    }
}

#[test]
fn datatype_restriction_in_data_property_range_lowers_to_base_datatype() {
    // Mirrors the RDF/XML parser's #150 rule: a facet-restricted
    // datatype range under-approximates to its base datatype.
    let ontology = parse(&doc(
        "DataPropertyRange(<http://example.org/ontology#dp> DatatypeRestriction(xsd:integer xsd:minInclusive \"0\"^^xsd:integer))",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::DataPropertyRange(_)
    ));
}

// ---------------------------------------------------------------------------
// Property characteristics (audit of the object-characteristics family)
// ---------------------------------------------------------------------------

#[test]
fn reflexive_object_property_parses() {
    let ontology = parse(&doc(
        "ReflexiveObjectProperty(<http://example.org/ontology#p>)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    assert!(matches!(
        axiom.as_ref(),
        owl2_reasoner::logic::axioms::Axiom::ReflexiveProperty(_)
    ));
}

#[test]
fn symmetric_object_property_parses() {
    let ontology = parse(&doc(
        "SymmetricObjectProperty(<http://example.org/ontology#p>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::SymmetricProperty(_)
    ));
}

#[test]
fn inverse_functional_object_property_parses() {
    let ontology = parse(&doc(
        "InverseFunctionalObjectProperty(<http://example.org/ontology#p>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::InverseFunctionalProperty(_)
    ));
}

#[test]
fn transitive_object_property_still_parses() {
    parse(&doc("TransitiveObjectProperty(<http://example.org/ontology#p>)"));
}

#[test]
fn asymmetric_object_property_still_parses() {
    parse(&doc("AsymmetricObjectProperty(<http://example.org/ontology#p>)"));
}

#[test]
fn irreflexive_object_property_still_parses() {
    parse(&doc("IrreflexiveObjectProperty(<http://example.org/ontology#p>)"));
}

#[test]
fn functional_object_property_still_parses() {
    parse(&doc("FunctionalObjectProperty(<http://example.org/ontology#p>)"));
}

// ---------------------------------------------------------------------------
// Anonymous individuals in the positions the corpus exercises
// ---------------------------------------------------------------------------

#[test]
fn class_assertion_spec_order_class_then_anonymous_individual() {
    // OWL 2 functional syntax: ClassAssertion(C I) — class first
    // (spec section 2.1 example: ClassAssertion(a:Person a:Peter)).
    let ontology = parse(&doc(
        "ClassAssertion(<http://example.org/ontology#A> _:genid141)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::ClassAssertion(assertion) = axiom.as_ref() else {
        panic!("expected ClassAssertion");
    };
    assert_eq!(
        assertion.individual().as_str(),
        "_:genid141",
        "anonymous individual is minted as a _: node IRI like the RDF/XML parser"
    );
}

#[test]
fn object_property_assertion_with_anonymous_object() {
    let ontology = parse(&doc(
        "ObjectPropertyAssertion(<http://example.org/ontology#p> <http://example.org/ontology#i> _:genid93)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::PropertyAssertion(assertion) = axiom.as_ref() else {
        panic!("expected PropertyAssertion");
    };
    let PropertyAssertionObject::Anonymous(anon) = assertion.object() else {
        panic!("expected anonymous object individual");
    };
    assert_eq!(anon.node_id(), "genid93");
}

#[test]
fn data_property_assertion_with_anonymous_subject() {
    let ontology = parse(&doc(
        "DataPropertyAssertion(<http://example.org/ontology#dp> _:genid7 \"x\"^^xsd:string)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::DataPropertyAssertion(assertion) = axiom.as_ref()
    else {
        panic!("expected DataPropertyAssertion");
    };
    assert_eq!(assertion.subject().as_str(), "_:genid7");
}

// ---------------------------------------------------------------------------
// Argument-list axioms (DisjointClasses arity failures were downstream
// symptoms of failing to consume an argument)
// ---------------------------------------------------------------------------

#[test]
fn disjoint_classes_with_multiple_named_arguments() {
    let ontology = parse(&doc(
        "DisjointClasses(<http://example.org/ontology#A> <http://example.org/ontology#B> <http://example.org/ontology#C>)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::DisjointClasses(disjoint) = axiom.as_ref() else {
        panic!("expected DisjointClasses");
    };
    assert_eq!(disjoint.classes().len(), 3);
}

#[test]
fn disjoint_classes_with_complex_argument() {
    parse(&doc(
        "DisjointClasses(<http://example.org/ontology#A> ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn equivalent_classes_named_arguments() {
    let ontology = parse(&doc(
        "EquivalentClasses(<http://example.org/ontology#A> <http://example.org/ontology#B>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::EquivalentClasses(_)
    ));
}

#[test]
fn equivalent_classes_with_complex_argument() {
    // Complex arguments lower to bidirectional SubClassOf pairs, the
    // same materialization the RDF/XML parser applies to deferred
    // equivalentClass edges.
    let ontology = parse(&doc(
        "EquivalentClasses(<http://example.org/ontology#A> ObjectUnionOf(<http://example.org/ontology#B> <http://example.org/ontology#C>))",
    ));
    let sub_class_count = ontology
        .axioms()
        .iter()
        .filter(|axiom| matches!(axiom.as_ref(), owl2_reasoner::logic::axioms::Axiom::SubClassOf(_)))
        .count();
    assert_eq!(sub_class_count, 2, "bidirectional SubClassOf lowering");
}

#[test]
fn disjoint_union_parses() {
    let ontology = parse(&doc(
        "DisjointUnion(<http://example.org/ontology#A> <http://example.org/ontology#B> <http://example.org/ontology#C>)",
    ));
    assert!(!ontology.axioms().is_empty());
}

#[test]
fn disjoint_classes_single_argument_still_rejected() {
    let error = parse_err(&doc("DisjointClasses(<http://example.org/ontology#A>)"));
    assert!(error.contains("at least 2"), "error was: {error}");
}

// ---------------------------------------------------------------------------
// Domain/range with complex class arguments (the "found: (" failures)
// ---------------------------------------------------------------------------

#[test]
fn object_property_domain_with_complex_expression() {
    parse(&doc(
        "ObjectPropertyDomain(<http://example.org/ontology#p> ObjectUnionOf(<http://example.org/ontology#A> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn object_property_range_with_complex_expression() {
    parse(&doc(
        "ObjectPropertyRange(<http://example.org/ontology#p> ObjectUnionOf(<http://example.org/ontology#A> <http://example.org/ontology#B>))",
    ));
}

#[test]
fn object_property_domain_named_still_direct_axiom() {
    let ontology = parse(&doc(
        "ObjectPropertyDomain(<http://example.org/ontology#p> <http://example.org/ontology#A>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::ObjectPropertyDomain(_)
    ));
}

// ---------------------------------------------------------------------------
// Property hierarchy axioms (including property chains)
// ---------------------------------------------------------------------------

#[test]
fn sub_object_property_of_chain() {
    let ontology = parse(&doc(
        "SubObjectPropertyOf(ObjectPropertyChain(<http://example.org/ontology#p> <http://example.org/ontology#q>) <http://example.org/ontology#r>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::SubPropertyChainOf(_)
    ));
}

#[test]
fn equivalent_object_properties() {
    parse(&doc(
        "EquivalentObjectProperties(<http://example.org/ontology#p> <http://example.org/ontology#q>)",
    ));
}

#[test]
fn disjoint_object_properties() {
    parse(&doc(
        "DisjointObjectProperties(<http://example.org/ontology#p> <http://example.org/ontology#q>)",
    ));
}

#[test]
fn inverse_object_properties_still_parses() {
    parse(&doc(
        "InverseObjectProperties(<http://example.org/ontology#p> <http://example.org/ontology#q>)",
    ));
}

// ---------------------------------------------------------------------------
// Data property axiom family
// ---------------------------------------------------------------------------

#[test]
fn sub_data_property_of() {
    parse(&doc(
        "SubDataPropertyOf(<http://example.org/ontology#dp> <http://example.org/ontology#dq>)",
    ));
}

#[test]
fn equivalent_data_properties() {
    parse(&doc(
        "EquivalentDataProperties(<http://example.org/ontology#dp> <http://example.org/ontology#dq>)",
    ));
}

#[test]
fn disjoint_data_properties() {
    parse(&doc(
        "DisjointDataProperties(<http://example.org/ontology#dp> <http://example.org/ontology#dq>)",
    ));
}

#[test]
fn data_property_domain_named() {
    parse(&doc(
        "DataPropertyDomain(<http://example.org/ontology#dp> <http://example.org/ontology#A>)",
    ));
}

#[test]
fn data_property_domain_complex() {
    parse(&doc(
        "DataPropertyDomain(<http://example.org/ontology#dp> ObjectOneOf(<http://example.org/ontology#i1>))",
    ));
}

#[test]
fn data_property_range_datatype() {
    parse(&doc("DataPropertyRange(<http://example.org/ontology#dp> xsd:string)"));
}

#[test]
fn functional_data_property() {
    parse(&doc("FunctionalDataProperty(<http://example.org/ontology#dp>)"));
}

// ---------------------------------------------------------------------------
// Assertion axiom family
// ---------------------------------------------------------------------------

#[test]
fn class_assertion_named_individual() {
    let ontology = parse(&doc(
        "ClassAssertion(<http://example.org/ontology#A> <http://example.org/ontology#i>)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::ClassAssertion(assertion) = axiom.as_ref() else {
        panic!("expected ClassAssertion");
    };
    assert_eq!(
        assertion.individual().as_str(),
        "http://example.org/ontology#i"
    );
}

#[test]
fn data_property_assertion_named_subject() {
    parse(&doc(
        "DataPropertyAssertion(<http://example.org/ontology#dp> <http://example.org/ontology#i> \"v\"^^xsd:string)",
    ));
}

#[test]
fn negative_object_property_assertion() {
    parse(&doc(
        "NegativeObjectPropertyAssertion(<http://example.org/ontology#p> <http://example.org/ontology#i1> <http://example.org/ontology#i2>)",
    ));
}

#[test]
fn same_individual() {
    parse(&doc(
        "SameIndividual(<http://example.org/ontology#i1> <http://example.org/ontology#i2>)",
    ));
}

#[test]
fn different_individuals() {
    parse(&doc(
        "DifferentIndividuals(<http://example.org/ontology#i1> <http://example.org/ontology#i2>)",
    ));
}

// ---------------------------------------------------------------------------
// Prefixed names in expression positions (the "found: owl" failures)
// ---------------------------------------------------------------------------

#[test]
fn prefixed_class_name_in_class_expression() {
    parse(&doc("SubClassOf(owl:Thing <http://example.org/ontology#A>)"));
}

#[test]
fn prefixed_class_name_in_disjoint_classes() {
    parse(&doc(
        "DisjointClasses(owl:Nothing <http://example.org/ontology#A>)",
    ));
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

#[test]
fn typed_literal_with_prefixed_datatype() {
    let ontology = parse(&doc(
        "DataPropertyAssertion(<http://example.org/ontology#dp> <http://example.org/ontology#i> \"42\"^^xsd:integer)",
    ));
    let axiom = ontology.axioms().first().expect("one axiom");
    let owl2_reasoner::logic::axioms::Axiom::DataPropertyAssertion(assertion) = axiom.as_ref()
    else {
        panic!("expected DataPropertyAssertion");
    };
    assert_eq!(assertion.value().lexical_form(), "42");
    assert_eq!(
        assertion.value().datatype().as_str(),
        "http://www.w3.org/2001/XMLSchema#integer"
    );
}

#[test]
fn language_tagged_literal() {
    parse(&doc(
        "DataPropertyAssertion(<http://example.org/ontology#dp> <http://example.org/ontology#i> \"hello\"@en)",
    ));
}

#[test]
fn plain_literal() {
    parse(&doc(
        "DataPropertyAssertion(<http://example.org/ontology#dp> <http://example.org/ontology#i> \"hello\")",
    ));
}

// ---------------------------------------------------------------------------
// Mixed realistic fragment mirroring corpus axiom shapes
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Axiom annotations and DL-safe rules (further corpus-exposed shapes)
// ---------------------------------------------------------------------------

#[test]
fn axiom_annotation_before_subclass_of_arguments() {
    // Every OWL 2 axiom production may carry leading Annotation parts;
    // the ORE corpus exercises them on SubClassOf.
    let ontology = parse(&doc(
        "SubClassOf(Annotation(<http://example.org/ontology#note> \"cjm\"^^xsd:string) <http://example.org/ontology#A> <http://example.org/ontology#B>)",
    ));
    assert_eq!(
        ontology.axioms().len(),
        1,
        "annotation is consumed; the decorated SubClassOf is kept"
    );
}

#[test]
fn axiom_annotation_before_object_property_domain_arguments() {
    let ontology = parse(&doc(
        "ObjectPropertyDomain(Annotation(<http://example.org/ontology#note> \"plain value\") <http://example.org/ontology#p> <http://example.org/ontology#A>)",
    ));
    assert!(matches!(
        ontology.axioms().first().expect("one axiom").as_ref(),
        owl2_reasoner::logic::axioms::Axiom::ObjectPropertyDomain(_)
    ));
}

#[test]
fn axiom_annotation_on_property_characteristic() {
    parse(&doc(
        "TransitiveObjectProperty(Annotation(<http://example.org/ontology#note> \"v\") <http://example.org/ontology#p>)",
    ));
}

#[test]
fn dl_safe_rule_is_skipped_and_dl_axioms_survive() {
    // SWRL DL-safe rules appear in the ORE corpus; they have no
    // rule-engine wiring, so the block is consumed and skipped while
    // the surrounding DL axioms still parse.
    let ontology = parse(&doc(
        "DLSafeRule(Body(ObjectPropertyAtom(<http://example.org/ontology#p> Variable(<urn:swrl#x>) Variable(<urn:swrl#y>)))Head(ClassAtom(<http://example.org/ontology#A> Variable(<urn:swrl#y>))))\nSubClassOf(<http://example.org/ontology#A> <http://example.org/ontology#B>)",
    ));
    assert_eq!(
        ontology.axioms().len(),
        1,
        "only the SubClassOf axiom is emitted; the rule contributes none"
    );
}

#[test]
fn dl_safe_rule_with_builtin_atom_and_nested_literals() {
    // Parentheses inside rule atoms and string literals must not
    // unbalance the rule block skip.
    parse(&doc(
        "DLSafeRule(Body(DataPropertyAtom(<http://example.org/ontology#dp> Variable(<urn:swrl#x>) \"a (balanced) string\") BuiltInAtom(<http://www.w3.org/2003/11/swrlb#add> Variable(<urn:swrl#y>) Variable(<urn:swrl#x>) \"1\"^^xsd:integer))Head(ClassAtom(<http://example.org/ontology#A> Variable(<urn:swrl#y>))))",
    ));
}

#[test]
fn corpus_shaped_fragment_parses() {
    parse(&doc(
        "\
Declaration(Class(<http://example.org/ontology#A>))
Declaration(ObjectProperty(<http://example.org/ontology#p>))
Declaration(DataProperty(<http://example.org/ontology#dp>))
Declaration(NamedIndividual(<http://example.org/ontology#i>))
SubClassOf(<http://example.org/ontology#A> ObjectAllValuesFrom(<http://example.org/ontology#p> <http://example.org/ontology#B>))
SubClassOf(<http://example.org/ontology#A> ObjectMinCardinality(1 <http://example.org/ontology#p>))
EquivalentClasses(<http://example.org/ontology#A> <http://example.org/ontology#B>)
DisjointClasses(<http://example.org/ontology#A> <http://example.org/ontology#B>)
SubObjectPropertyOf(ObjectPropertyChain(<http://example.org/ontology#p> <http://example.org/ontology#q>) <http://example.org/ontology#r>)
ObjectPropertyDomain(<http://example.org/ontology#p> <http://example.org/ontology#A>)
ObjectPropertyRange(<http://example.org/ontology#p> ObjectUnionOf(<http://example.org/ontology#A> <http://example.org/ontology#B>))
ClassAssertion(<http://example.org/ontology#A> <http://example.org/ontology#i>)
ClassAssertion(owl:Thing <http://example.org/ontology#i>)
ObjectPropertyAssertion(<http://example.org/ontology#p> <http://example.org/ontology#i> _:genid1)
DataPropertyAssertion(<http://example.org/ontology#dp> <http://example.org/ontology#i> \"1\"^^xsd:integer)
DataPropertyRange(<http://example.org/ontology#dp> xsd:integer)
TransitiveObjectProperty(<http://example.org/ontology#p>)
SymmetricObjectProperty(<http://example.org/ontology#p>)
ReflexiveObjectProperty(<http://example.org/ontology#p>)
FunctionalObjectProperty(<http://example.org/ontology#p>)
InverseFunctionalObjectProperty(<http://example.org/ontology#p>)
IrreflexiveObjectProperty(<http://example.org/ontology#p>)
AsymmetricObjectProperty(<http://example.org/ontology#p>)
",
    ));
}
