use owl2_reasoner::core::entities::Individual;
use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::{Axiom, ClassExpression, Ontology, OwlError, OwlResult};

const UBERON_ENUMERATION: &str = include_str!("data/uberon_oneof_minimal.owl");

fn parse(content: &str, structural: bool) -> OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

fn nominal_members(expression: &ClassExpression) -> Option<Vec<String>> {
    let ClassExpression::ObjectOneOf(members) = expression else {
        return None;
    };
    Some(
        members
            .iter()
            .map(|member| match member {
                Individual::Named(individual) => individual.iri().as_str().to_owned(),
                Individual::Anonymous(_) => panic!("named member became anonymous"),
            })
            .collect(),
    )
}

fn document(body: &str) -> String {
    format!(
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
          xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
          xmlns:owl="http://www.w3.org/2002/07/owl#">
          {body}</rdf:RDF>"#
    )
}

fn assert_expression_error(content: &str, needle: &str) {
    for structural in [false, true] {
        match parse(content, structural).unwrap_err() {
            OwlError::AxiomValidationError {
                axiom_type,
                message,
            } => {
                assert_eq!(axiom_type, "RDF/XML class expression");
                assert!(message.contains(needle), "{message}");
            }
            other => panic!("wrong error in structural={structural}: {other}"),
        }
    }
}

#[test]
fn uberon_enumeration_retains_nine_individuals_in_both_equivalence_directions() {
    let expected: Vec<_> = [
        "IAO_0000002",
        "IAO_0000120",
        "IAO_0000121",
        "IAO_0000122",
        "IAO_0000123",
        "IAO_0000124",
        "IAO_0000125",
        "IAO_0000423",
        "IAO_0000428",
    ]
    .iter()
    .map(|id| format!("http://purl.obolibrary.org/obo/{id}"))
    .collect();

    for structural in [false, true] {
        let ontology = parse(UBERON_ENUMERATION, structural).unwrap();
        let mut directions = [false, false];
        for axiom in ontology.axioms() {
            match axiom.as_ref() {
                Axiom::SubClassOf(subclass) => {
                    for (index, named, nominal) in [
                        (0, subclass.sub_class(), subclass.super_class()),
                        (1, subclass.super_class(), subclass.sub_class()),
                    ] {
                        if let ClassExpression::Class(class) = named {
                            assert_eq!(
                                class.iri().as_str(),
                                "http://purl.obolibrary.org/obo/IAO_0000078"
                            );
                            assert_eq!(nominal_members(nominal), Some(expected.clone()));
                            directions[index] = true;
                        }
                    }
                }
                Axiom::DifferentIndividuals(_) => panic!("parser inferred unique names"),
                _ => {}
            }
        }
        assert_eq!(directions, [true, true], "structural={structural}");
    }
}

#[test]
fn empty_object_enumeration_is_an_explicit_bounded_parser_error() {
    let content = document(
        r#"<owl:Class rdf:about="http://example.org/A">
          <rdfs:subClassOf><owl:Class>
            <owl:oneOf rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
          </owl:Class></rdfs:subClassOf>
        </owl:Class>"#,
    );
    assert_expression_error(&content, "empty object enumeration");
}

#[test]
fn one_of_with_a_competing_class_constructor_is_rejected_in_either_order() {
    for predicates in [
        r#"<owl:oneOf rdf:nodeID="list"/><owl:unionOf rdf:nodeID="list"/>"#,
        r#"<owl:unionOf rdf:nodeID="list"/><owl:oneOf rdf:nodeID="list"/>"#,
    ] {
        let content = document(&format!(
            r#"<owl:Class rdf:about="http://example.org/A"><rdfs:subClassOf rdf:nodeID="nominal"/></owl:Class>
            <rdf:Description rdf:nodeID="nominal">{predicates}</rdf:Description>
            <rdf:Description rdf:nodeID="list"><rdf:first rdf:resource="http://example.org/a"/>
              <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
            </rdf:Description>"#
        ));
        assert_expression_error(&content, "mixed restriction predicates");
    }
}

#[test]
fn object_enumerations_reject_literal_and_anonymous_members_without_coercion() {
    for (member, error) in [
        ("<rdf:first>not an individual</rdf:first>", "literal member"),
        (
            r#"<rdf:first rdf:nodeID="anonymous"/>"#,
            "Unsupported anonymous individual",
        ),
    ] {
        let content = document(&format!(
            r#"<owl:Class rdf:about="http://example.org/A"><rdfs:subClassOf rdf:nodeID="nominal"/></owl:Class>
            <rdf:Description rdf:nodeID="nominal"><owl:oneOf rdf:nodeID="list"/></rdf:Description>
            <rdf:Description rdf:nodeID="list">{member}
              <rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>
            </rdf:Description>"#
        ));
        assert_expression_error(&content, error);
    }
}

#[test]
fn nominal_lists_reject_conflicts_missing_links_and_cycles() {
    let cases = [
        (
            r#"<owl:oneOf rdf:nodeID="list"/><owl:oneOf rdf:nodeID="other"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>"#,
        ),
        (
            r#"<owl:oneOf rdf:nodeID="list"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/><rdf:first rdf:resource="http://example.org/b"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>"#,
        ),
        (
            r#"<owl:oneOf rdf:nodeID="list"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:nodeID="list"/>"#,
        ),
        (
            r#"<owl:oneOf rdf:nodeID="list"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/>"#,
        ),
        (
            r#"<owl:oneOf rdf:nodeID="list"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:nodeID="missing"/>"#,
        ),
        (
            r#"<owl:oneOf rdf:nodeID="list"/>"#,
            r#"<rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:nodeID="other"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/>"#,
        ),
    ];
    for (predicate, list) in cases {
        let content = document(&format!(
            r#"<owl:Class rdf:about="http://example.org/A"><rdfs:subClassOf rdf:nodeID="nominal"/></owl:Class>
            <rdf:Description rdf:nodeID="nominal">{predicate}</rdf:Description>
            <rdf:Description rdf:nodeID="list">{list}</rdf:Description>"#
        ));
        assert_expression_error(&content, "");
    }
}

#[test]
fn shared_forward_nominals_preserve_duplicate_members_and_nested_positions() {
    let content = document(
        r#"<owl:Class rdf:about="http://example.org/A"><rdfs:subClassOf rdf:nodeID="nominal"/></owl:Class>
        <owl:NamedIndividual rdf:about="http://example.org/x"><rdf:type rdf:nodeID="nominal"/></owl:NamedIndividual>
        <owl:Class rdf:about="http://example.org/B"><rdfs:subClassOf rdf:nodeID="complement"/></owl:Class>
        <rdf:Description rdf:nodeID="complement"><owl:complementOf rdf:nodeID="nominal"/></rdf:Description>
        <rdf:Description rdf:nodeID="nominal"><owl:oneOf rdf:nodeID="list"/><owl:oneOf rdf:nodeID="list"/></rdf:Description>
        <rdf:Description rdf:nodeID="list"><rdf:first rdf:resource="http://example.org/a"/><rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:nodeID="tail"/></rdf:Description>
        <rdf:Description rdf:nodeID="tail"><rdf:first rdf:resource="http://example.org/a"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        let expected = Some(vec!["http://example.org/a".to_owned(); 2]);
        let mut found = [false, false, false];
        for axiom in ontology.axioms() {
            match axiom.as_ref() {
                Axiom::SubClassOf(subclass) => match subclass.super_class() {
                    ClassExpression::ObjectOneOf(_) => {
                        assert_eq!(nominal_members(subclass.super_class()), expected);
                        found[0] = true;
                    }
                    ClassExpression::ObjectComplementOf(inner) => {
                        assert_eq!(nominal_members(inner), expected);
                        found[1] = true;
                    }
                    _ => {}
                },
                Axiom::ClassAssertion(assertion) => {
                    // Skip the owl:NamedIndividual declaration assertion the
                    // parser emits for typed node elements; the nominal
                    // assertion is the one under test.
                    if let ClassExpression::Class(class) = assertion.class_expr() {
                        if class.iri().as_str()
                            == "http://www.w3.org/2002/07/owl#NamedIndividual"
                        {
                            continue;
                        }
                    }
                    assert_eq!(nominal_members(assertion.class_expr()), expected);
                    found[2] = true;
                }
                Axiom::DifferentIndividuals(_) => panic!("parser inferred unique names"),
                _ => {}
            }
        }
        assert_eq!(found, [true, true, true]);
    }
}

#[test]
fn data_one_of_keeps_literal_values_and_datatypes() {
    let content = document(
        r#"<owl:DatatypeProperty rdf:about="http://example.org/p"/>
        <owl:NamedIndividual rdf:about="http://example.org/x"><rdf:type rdf:nodeID="restriction"/></owl:NamedIndividual>
        <owl:Restriction rdf:nodeID="restriction"><owl:onProperty rdf:resource="http://example.org/p"/>
          <owl:someValuesFrom rdf:nodeID="data"/>
        </owl:Restriction>
        <rdf:Description rdf:nodeID="data"><owl:oneOf rdf:nodeID="list"/></rdf:Description>
        <rdf:Description rdf:nodeID="list"><rdf:first rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">7</rdf:first><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        let mut found = false;
        for axiom in ontology.axioms() {
            if let Axiom::ClassAssertion(assertion) = axiom.as_ref() {
                if let ClassExpression::DataSomeValuesFrom(_, range) = assertion.class_expr() {
                    let owl2_reasoner::logic::axioms::class_expressions::DataRange::DataOneOf(
                        values,
                    ) = range.as_ref()
                    else {
                        panic!("data enumeration became another range");
                    };
                    assert_eq!(values.len(), 1);
                    assert_eq!(values[0].lexical_form(), "7");
                    assert_eq!(
                        values[0].datatype().as_str(),
                        "http://www.w3.org/2001/XMLSchema#integer"
                    );
                    found = true;
                }
            }
        }
        assert!(found);
    }
}

#[test]
fn repeated_nominal_expansion_obeys_the_generated_node_budget() {
    let mut body = String::from(
        r#"<owl:Class rdf:about="http://example.org/A"><rdfs:subClassOf rdf:nodeID="n0"/></owl:Class>"#,
    );
    for level in 0..12 {
        body.push_str(&format!(
            r#"<rdf:Description rdf:nodeID="n{level}"><owl:unionOf rdf:nodeID="l{level}"/></rdf:Description>
            <rdf:Description rdf:nodeID="l{level}"><rdf:first rdf:nodeID="n{}"/><rdf:rest rdf:nodeID="r{level}"/></rdf:Description>
            <rdf:Description rdf:nodeID="r{level}"><rdf:first rdf:nodeID="n{}"/><rdf:rest rdf:resource="http://www.w3.org/1999/02/22-rdf-syntax-ns#nil"/></rdf:Description>"#,
            level + 1, level + 1,
        ));
    }
    body.push_str(r#"<rdf:Description rdf:nodeID="n12"><owl:oneOf rdf:parseType="Collection">"#);
    for member in 0..64 {
        body.push_str(&format!(
            r#"<rdf:Description rdf:about="http://example.org/a{member}"/>"#
        ));
    }
    body.push_str("</owl:oneOf></rdf:Description>");
    for structural in [false, true] {
        assert!(matches!(
            parse(&document(&body), structural),
            Err(OwlError::ResourceLimitExceeded { .. })
        ));
    }
}

/// #115: a named class carrying several `owl:oneOf` lists (the W3C
/// description-logic-501/502 TorF shape) is N class equalities under the
/// RDF-based reading — each list lowers to its own bidirectional
/// SubClassOf pair, not just the first list.
#[test]
fn named_class_with_two_oneof_lists_lowers_each_to_bidirectional_subclassof() {
    let content = document(
        r#"<owl:Class rdf:about="http://example.org/C">
             <owl:oneOf rdf:parseType="Collection">
               <rdf:Description rdf:about="http://example.org/a"/>
               <rdf:Description rdf:about="http://example.org/b"/>
             </owl:oneOf>
             <owl:oneOf rdf:parseType="Collection">
               <rdf:Description rdf:about="http://example.org/c"/>
             </owl:oneOf>
           </owl:Class>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        let mut found: Vec<(Vec<String>, bool)> = Vec::new();
        for axiom in ontology.subclass_axioms().iter() {
            let (members, forward) = match (axiom.sub_class(), axiom.super_class()) {
                (ClassExpression::Class(_), ClassExpression::ObjectOneOf(_)) => {
                    let members = axiom.super_class().clone();
                    (nominal_members(&members).unwrap(), true)
                }
                (ClassExpression::ObjectOneOf(_), ClassExpression::Class(_)) => {
                    let members = axiom.sub_class().clone();
                    (nominal_members(&members).unwrap(), false)
                }
                _ => continue,
            };
            found.push((members, forward));
        }
        assert_eq!(
            found.len(),
            4,
            "both enumerations lower in both directions (structural={structural}): {found:?}"
        );
        let forward_lists: Vec<_> = found
            .iter()
            .filter(|(_, forward)| *forward)
            .map(|(members, _)| members.clone())
            .collect();
        assert!(forward_lists.contains(&vec![
            "http://example.org/a".to_string(),
            "http://example.org/b".to_string()
        ]));
        assert!(forward_lists.contains(&vec!["http://example.org/c".to_string()]));
    }
}

/// #115 scope guard: a single-list oneOf frame on a named class keeps
/// today's behavior (other paths own that idiom); only the
/// multiple-enumeration lattice idiom flushes.
#[test]
fn single_list_oneof_frame_stays_unlowered() {
    let content = document(
        r#"<owl:Class rdf:about="http://example.org/C">
             <owl:oneOf rdf:parseType="Collection">
               <rdf:Description rdf:about="http://example.org/a"/>
               <rdf:Description rdf:about="http://example.org/b"/>
             </owl:oneOf>
           </owl:Class>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        assert_eq!(
            ontology.subclass_axioms().len(),
            0,
            "the single-list frame does not lower (structural={structural})"
        );
    }
}
