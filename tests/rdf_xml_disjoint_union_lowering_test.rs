use owl2_reasoner::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use owl2_reasoner::parser::ParserConfig;
use owl2_reasoner::{ClassExpression, Ontology, OwlResult};

fn parse(content: &str, structural: bool) -> OwlResult<Ontology> {
    let mut parser = RdfXmlStreamingParser::new(ParserConfig::default());
    if structural {
        parser.parse_content_structural(content)
    } else {
        parser.parse_content(content)
    }
}

fn document(body: &str) -> String {
    format!(
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
          xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
          xmlns:owl="http://www.w3.org/2002/07/owl#">
          {body}</rdf:RDF>"#
    )
}

fn disjoint_union_frame(members: &str) -> String {
    document(&format!(
        r#"<owl:Class rdf:about="http://example.org/C">
             <owl:disjointUnionOf rdf:parseType="Collection">{members}</owl:disjointUnionOf>
           </owl:Class>"#
    ))
}

fn union_members(expression: &ClassExpression) -> Option<Vec<String>> {
    let ClassExpression::ObjectUnionOf(operands) = expression else {
        return None;
    };
    Some(
        operands
            .iter()
            .map(|operand| match operand.as_ref() {
                ClassExpression::Class(class) => class.iri().as_str().to_owned(),
                other => panic!("unnamed union operand: {other:?}"),
            })
            .collect(),
    )
}

/// #120: a named class carrying `owl:disjointUnionOf` lowers to the
/// two axiom shapes its Direct Semantics equals — C ≡ the union
/// (bidirectional SubClassOf) plus the pairwise DisjointClasses — the
/// shape W3C New-Feature-DisjointUnion-001 (0049) needs under #119's
/// union choices.
#[test]
fn disjoint_union_frame_lowers_to_equivalence_and_disjointness() {
    let content = disjoint_union_frame(
        r#"<rdf:Description rdf:about="http://example.org/Boy"/>
           <rdf:Description rdf:about="http://example.org/Girl"/>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();

        let mut forward = 0;
        let mut backward = 0;
        for axiom in ontology.subclass_axioms().iter() {
            match (axiom.sub_class(), axiom.super_class()) {
                (ClassExpression::Class(sub), ClassExpression::ObjectUnionOf(_))
                    if sub.iri().as_str() == "http://example.org/C" =>
                {
                    forward += 1;
                    assert_eq!(
                        union_members(axiom.super_class()).unwrap(),
                        vec![
                            "http://example.org/Boy".to_string(),
                            "http://example.org/Girl".to_string()
                        ]
                    );
                }
                (ClassExpression::ObjectUnionOf(_), ClassExpression::Class(sup))
                    if sup.iri().as_str() == "http://example.org/C" =>
                {
                    backward += 1;
                }
                _ => {}
            }
        }
        assert_eq!(
            forward, 1,
            "C ⊑ Boy ⊔ Girl lowers (structural={structural})"
        );
        assert_eq!(
            backward, 1,
            "Boy ⊔ Girl ⊑ C lowers (structural={structural})"
        );

        let disjoint: Vec<_> = ontology
            .disjoint_classes_axioms()
            .iter()
            .map(|axiom| {
                axiom
                    .classes()
                    .iter()
                    .map(|class| class.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            disjoint,
            vec![vec![
                "http://example.org/Boy".to_string(),
                "http://example.org/Girl".to_string()
            ]],
            "the members are pairwise disjoint (structural={structural})"
        );
    }
}

/// #120 review finding: a class carrying two disjointUnionOf lists
/// states two axioms — both lower, the #115 multi-list precedent
/// (nothing valid drops silently).
#[test]
fn two_disjoint_union_lists_each_lower() {
    let content = document(
        r#"<owl:Class rdf:about="http://example.org/C">
             <owl:disjointUnionOf rdf:parseType="Collection">
               <rdf:Description rdf:about="http://example.org/Boy"/>
               <rdf:Description rdf:about="http://example.org/Girl"/>
             </owl:disjointUnionOf>
             <owl:disjointUnionOf rdf:parseType="Collection">
               <rdf:Description rdf:about="http://example.org/X"/>
               <rdf:Description rdf:about="http://example.org/Y"/>
             </owl:disjointUnionOf>
           </owl:Class>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        let mut unions = 0;
        for axiom in ontology.subclass_axioms().iter() {
            if let (
                ClassExpression::Class(sub),
                ClassExpression::ObjectUnionOf(_),
            ) = (axiom.sub_class(), axiom.super_class())
            {
                if sub.iri().as_str() == "http://example.org/C" {
                    unions += 1;
                }
            }
        }
        assert_eq!(
            unions, 2,
            "both lists lower (structural={structural})"
        );
        assert_eq!(
            ontology.disjoint_classes_axioms().len(),
            2,
            "both disjointness axioms appear (structural={structural})"
        );
    }
}

/// #120 scope guard: a single-member disjointUnionOf keeps today's
/// silently-ignored behavior — no equivalence, no trivial
/// DisjointClasses.
#[test]
fn single_member_disjoint_union_frame_stays_unlowered() {
    let content =
        disjoint_union_frame(r#"<rdf:Description rdf:about="http://example.org/Only"/>"#);
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        assert_eq!(
            ontology.subclass_axioms().len(),
            0,
            "the single-member frame does not lower (structural={structural})"
        );
        assert_eq!(
            ontology.disjoint_classes_axioms().len(),
            0,
            "no degenerate disjointness appears (structural={structural})"
        );
    }
}

/// #120 scope guard: an anonymous description member keeps today's
/// silently-ignored behavior — the narrow named-members gate is the
/// #109 lesson.
#[test]
fn anonymous_member_disjoint_union_frame_stays_unlowered() {
    let content = disjoint_union_frame(
        r#"<rdf:Description rdf:about="http://example.org/Boy"/>
           <owl:Restriction>
             <owl:onProperty rdf:resource="http://example.org/p"/>
             <owl:someValuesFrom rdf:resource="http://example.org/D"/>
           </owl:Restriction>"#,
    );
    for structural in [false, true] {
        let ontology = parse(&content, structural).unwrap();
        assert_eq!(
            ontology.subclass_axioms().len(),
            0,
            "the mixed frame does not lower (structural={structural})"
        );
        assert_eq!(
            ontology.disjoint_classes_axioms().len(),
            0,
            "no disjointness appears (structural={structural})"
        );
    }
}
