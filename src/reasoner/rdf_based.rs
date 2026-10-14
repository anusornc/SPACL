//! The OWL 2 RDF-Based Semantics lane (ADR 0006 and its 2026-10-09
//! amendment): entailment over the document's **owned RDF graph** —
//! closure of the premise graph plus the semantic conditions' built-in
//! vocabulary, with families landing easiest-first (prop → eqdis →
//! ndis → char/bool → restrict). v0 carries the annotation-property
//! vocabulary family; shapes it does not cover answer `None` and the
//! caller falls back to the Direct evaluation, so routing the lane
//! cannot regress currently-answered rows.

use crate::parser::rdf_xml_streaming::{RdfGraphTerm, RdfGraphTriple};
use std::collections::HashSet;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_PROPERTY_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const OWL_DISTINCT_MEMBERS: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";

/// The built-in annotation properties the OWL 2 RDF-Based semantics
/// types in every interpretation (the spec's semantic conditions on
/// the vocabulary): each of these is an owl:AnnotationProperty in any
/// model, so the typing triple is entailed by the empty graph.
const BUILTIN_ANNOTATION_PROPERTIES: &[&str] = &[
    "label",
    "comment",
    "seeAlso",
    "isDefinedBy",
];
const OWL_BUILTIN_ANNOTATION_PROPERTIES: &[&str] = &[
    "versionInfo",
    "deprecated",
    "priorVersion",
    "backwardCompatibleWith",
    "incompatibleWith",
];

fn typing(subject: &str, class: &str) -> RdfGraphTriple {
    RdfGraphTriple {
        subject: RdfGraphTerm::Iri(subject.to_string()),
        predicate: RDF_TYPE.to_string(),
        object: RdfGraphTerm::Iri(class.to_string()),
    }
}

/// The owned-graph closure with the landed RDF-Based rule families.
/// Whether (sub, super) is declared (or derived) in the pair list.
fn sub_codes_contains(
    pairs: &[(RdfGraphTerm, RdfGraphTerm)],
    sub: &RdfGraphTerm,
    super_class: &RdfGraphTerm,
) -> bool {
    pairs
        .iter()
        .any(|(left, right)| left == sub && right == super_class)
}

/// Resolve an RDF list over a graph; bounded walk, cycles/dangling
/// tails give None.
fn resolve_list_over(
    graph: &HashSet<RdfGraphTriple>,
    head: &RdfGraphTerm,
) -> Option<Vec<RdfGraphTerm>> {
    let mut members = Vec::new();
    let mut current = head.clone();
    for _ in 0..1024 {
        if let RdfGraphTerm::Iri(named) = &current {
            if named == RDF_NIL {
                return Some(members);
            }
        }
        let mut first: Option<RdfGraphTerm> = None;
        let mut rest: Option<RdfGraphTerm> = None;
        for triple in graph.iter() {
            if triple.subject == current {
                if triple.predicate == RDF_FIRST {
                    first = Some(triple.object.clone());
                } else if triple.predicate == RDF_REST {
                    rest = Some(triple.object.clone());
                }
            }
        }
        members.push(first?);
        current = rest?;
    }
    None
}

/// The owned-graph closure with the landed RDF-Based rule families.
pub struct RdfBasedEvaluator {
    closure: HashSet<RdfGraphTriple>,
}

impl RdfBasedEvaluator {
    pub fn new(premise: Vec<RdfGraphTriple>) -> Self {
        let mut closure: HashSet<RdfGraphTriple> = premise.into_iter().collect();
        for local in BUILTIN_ANNOTATION_PROPERTIES {
            closure.insert(typing(&format!("{RDFS}{local}"), OWL_ANNOTATION_PROPERTY));
        }
        for local in OWL_BUILTIN_ANNOTATION_PROPERTIES {
            closure.insert(typing(&format!("{OWL}{local}"), OWL_ANNOTATION_PROPERTY));
        }
        // Family 2 (#182): the equality rules to a fixpoint — eq-sym,
        // eq-trans, eq-rep1/eq-rep2 (sameAs substitution in both
        // positions). Premises here are tiny; iterate to stability.
        loop {
            let mut derived: Vec<RdfGraphTriple> = Vec::new();
            let same_as: Vec<(&RdfGraphTerm, &RdfGraphTerm)> = closure
                .iter()
                .filter(|triple| triple.predicate == OWL_SAME_AS)
                .filter_map(|triple| match (&triple.subject, &triple.object) {
                    (left, right) => Some((left, right)),
                })
                .collect();
            for (left, right) in &same_as {
                derived.push(RdfGraphTriple {
                    subject: (*right).clone(),
                    predicate: OWL_SAME_AS.to_string(),
                    object: (*left).clone(),
                });
                for triple in closure.iter() {
                    if triple.subject == **left {
                        derived.push(RdfGraphTriple {
                            subject: (*right).clone(),
                            predicate: triple.predicate.clone(),
                            object: triple.object.clone(),
                        });
                    }
                    if triple.object == **left {
                        derived.push(RdfGraphTriple {
                            subject: triple.subject.clone(),
                            predicate: triple.predicate.clone(),
                            object: (*right).clone(),
                        });
                    }
                    // Properties are resources too: sameAs substitutes
                    // into the predicate position (rdfbased-sem
                    // sameas-subst's third arm).
                    if let (RdfGraphTerm::Iri(named), RdfGraphTerm::Iri(replacement)) = (left, right)
                    {
                        if triple.predicate == *named {
                            derived.push(RdfGraphTriple {
                                subject: triple.subject.clone(),
                                predicate: replacement.clone(),
                                object: triple.object.clone(),
                            });
                        }
                    }
                }
            }
            for (a, b) in &same_as {
                for (c, d) in &same_as {
                    if b == c {
                        derived.push(RdfGraphTriple {
                            subject: (*a).clone(),
                            predicate: OWL_SAME_AS.to_string(),
                            object: (*d).clone(),
                        });
                    }
                }
            }
            // Family 4 (#184): union terms and inverse-functional
            // data. Each member m of c's unionOf list gets m ⊑ c; an
            // individual typed with a member is typed with c.
            let unions: Vec<(RdfGraphTerm, RdfGraphTerm)> = closure
                .iter()
                .filter(|triple| triple.predicate == OWL_UNION_OF)
                .map(|triple| (triple.subject.clone(), triple.object.clone()))
                .collect();
            for (class, head) in &unions {
                let Some(members) = resolve_list_over(&closure, head) else {
                    continue;
                };
                for member in &members {
                    derived.push(RdfGraphTriple {
                        subject: member.clone(),
                        predicate: RDFS_SUBCLASS_OF.to_string(),
                        object: class.clone(),
                    });
                    for typed in closure.iter() {
                        if typed.predicate == RDF_TYPE && typed.object == *member {
                            derived.push(RdfGraphTriple {
                                subject: typed.subject.clone(),
                                predicate: RDF_TYPE.to_string(),
                                object: class.clone(),
                            });
                        }
                    }
                }
            }
            // Two owners of one value under an inverse-functional
            // property are the same individual (literal equality
            // included — the RDF-Based semantics types data this way).
            let inverse_functional: Vec<String> = closure
                .iter()
                .filter(|triple| {
                    triple.predicate == RDF_TYPE
                        && triple.object == RdfGraphTerm::Iri(OWL_INVERSE_FUNCTIONAL_PROPERTY.to_string())
                })
                .filter_map(|triple| match &triple.subject {
                    RdfGraphTerm::Iri(named) => Some(named.clone()),
                    _ => None,
                })
                .collect();
            for property in &inverse_functional {
                let owned: Vec<(&RdfGraphTerm, &RdfGraphTerm)> = closure
                    .iter()
                    .filter(|triple| triple.predicate == *property)
                    .map(|triple| (&triple.subject, &triple.object))
                    .collect();
                for (left_subject, left_value) in &owned {
                    for (right_subject, right_value) in &owned {
                        if left_subject != right_subject && left_value == right_value {
                            derived.push(RdfGraphTriple {
                                subject: (*left_subject).clone(),
                                predicate: OWL_SAME_AS.to_string(),
                                object: (*right_subject).clone(),
                            });
                        }
                    }
                }
            }
            // Family 5 (#185): the restriction vocabulary's instance
            // and comparison conditions, read on the holder.
            let restriction_properties: Vec<(&RdfGraphTriple, String)> = closure
                .iter()
                .filter(|triple| {
                    [
                        OWL_ALL_VALUES_FROM,
                        OWL_SOME_VALUES_FROM,
                        OWL_HAS_VALUE,
                        OWL_MAX_CARDINALITY,
                        OWL_MAX_QUALIFIED_CARDINALITY,
                    ]
                    .contains(&triple.predicate.as_str())
                })
                .map(|triple| (triple, triple.predicate.clone()))
                .collect();
            let on_property = |holder: &RdfGraphTerm| -> Option<String> {
                closure
                    .iter()
                    .find(|link| link.subject == *holder && link.predicate == OWL_ON_PROPERTY)
                    .and_then(|link| match &link.object {
                        RdfGraphTerm::Iri(named) => Some(named.clone()),
                        _ => None,
                    })
            };
            for (triple, kind) in &restriction_properties {
                let holder = &triple.subject;
                let Some(property) = on_property(holder) else {
                    continue;
                };
                match kind.as_str() {
                    name if name == OWL_ALL_VALUES_FROM => {
                        // An instance of the holder with an edge gets
                        // the filler on the edge's object.
                        for typed in closure.iter() {
                            if typed.predicate == RDF_TYPE && typed.object == *holder {
                                for edge in closure.iter() {
                                    if edge.predicate == property && edge.subject == typed.subject {
                                        derived.push(RdfGraphTriple {
                                            subject: edge.object.clone(),
                                            predicate: RDF_TYPE.to_string(),
                                            object: triple.object.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    name if name == OWL_SOME_VALUES_FROM => {
                        // Edge + typed object puts the holder on the
                        // subject.
                        for edge in closure.iter() {
                            if edge.predicate != property {
                                continue;
                            }
                            if closure.iter().any(|typed| {
                                typed.predicate == RDF_TYPE
                                    && typed.subject == edge.object
                                    && typed.object == triple.object
                            }) {
                                derived.push(RdfGraphTriple {
                                    subject: edge.subject.clone(),
                                    predicate: RDF_TYPE.to_string(),
                                    object: holder.clone(),
                                });
                            }
                        }
                    }
                    name if name == OWL_HAS_VALUE => {
                        // Both directions: a typed holder carries the
                        // value edge; the value edge types the holder.
                        for typed in closure.iter() {
                            if typed.predicate == RDF_TYPE && typed.object == *holder {
                                derived.push(RdfGraphTriple {
                                    subject: typed.subject.clone(),
                                    predicate: property.clone(),
                                    object: triple.object.clone(),
                                });
                            }
                        }
                        for edge in closure.iter() {
                            if edge.predicate == property && edge.object == triple.object {
                                derived.push(RdfGraphTriple {
                                    subject: edge.subject.clone(),
                                    predicate: RDF_TYPE.to_string(),
                                    object: holder.clone(),
                                });
                            }
                        }
                    }
                    name if name == OWL_MAX_CARDINALITY || name == OWL_MAX_QUALIFIED_CARDINALITY => {
                        // n+1 distinct (qualifying) objects under a
                        // max-n restriction join pairwise.
                        let RdfGraphTerm::Literal { lexical, .. } = &triple.object else {
                            continue;
                        };
                        let Ok(maximum) = lexical.parse::<usize>() else {
                            continue;
                        };
                        let qualifier = if kind == OWL_MAX_QUALIFIED_CARDINALITY {
                            closure
                                .iter()
                                .find(|link| link.subject == *holder && link.predicate == OWL_ON_CLASS)
                                .map(|link| link.object.clone())
                        } else {
                            None
                        };
                        for typed in closure.iter() {
                            if typed.predicate != RDF_TYPE || typed.object != *holder {
                                continue;
                            }
                            let objects: Vec<&RdfGraphTerm> = closure
                                .iter()
                                .filter(|edge| {
                                    edge.predicate == property && edge.subject == typed.subject
                                })
                                .filter(|edge| match &qualifier {
                                    Some(class) => closure.iter().any(|c| {
                                        c.predicate == RDF_TYPE
                                            && c.subject == edge.object
                                            && c.object == *class
                                    }),
                                    None => true,
                                })
                                .map(|edge| &edge.object)
                                .collect();
                            let mut distinct: Vec<&RdfGraphTerm> = Vec::new();
                            for object in &objects {
                                if !distinct.iter().any(|known| known == object) {
                                    distinct.push(object);
                                }
                            }
                            if distinct.len() > maximum {
                                for (index, left) in distinct.iter().enumerate() {
                                    for right in distinct.iter().skip(index + 1) {
                                        derived.push(RdfGraphTriple {
                                            subject: (*left).clone(),
                                            predicate: OWL_SAME_AS.to_string(),
                                            object: (*right).clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Family 5 comparisons: same kind, the second's property a
            // superproperty (or equal), filler sub- or equal (values
            // equal for hasValue) -> first rdfs:subClassOf second.
            {
                let sub_classes: Vec<(RdfGraphTerm, RdfGraphTerm)> = closure
                    .iter()
                    .filter(|triple| triple.predicate == RDFS_SUBCLASS_OF)
                    .map(|triple| (triple.subject.clone(), triple.object.clone()))
                    .collect();
                let sub_properties: Vec<(String, String)> = closure
                    .iter()
                    .filter(|triple| triple.predicate == RDFS_SUBPROPERTY_OF)
                    .filter_map(|triple| match (&triple.subject, &triple.object) {
                        (RdfGraphTerm::Iri(left), RdfGraphTerm::Iri(right)) => {
                            Some((left.clone(), right.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                for (first, kind) in &restriction_properties {
                    for (second, other_kind) in &restriction_properties {
                        if kind != other_kind || first.subject == second.subject {
                            continue;
                        }
                        let Some(first_property) = on_property(&first.subject) else {
                            continue;
                        };
                        let Some(second_property) = on_property(&second.subject) else {
                            continue;
                        };
                        let property_ok = first_property == second_property
                            || sub_properties
                                .iter()
                                .any(|(left, right)| *left == first_property && *right == second_property);
                        if !property_ok {
                            continue;
                        }
                        let filler_ok = if kind == OWL_HAS_VALUE {
                            first.object == second.object
                        } else {
                            first.object == second.object
                                || sub_codes_contains(&sub_classes, &first.object, &second.object)
                        };
                        // A universal over a WIDER property is
                        // stronger: p1 ⊑ p2 makes x2=∀p2.c subsume
                        // x1=∀p1.c, so the derived subClassOf flips for
                        // allValuesFrom across (non-equal) properties.
                        let direction_flipped = kind == OWL_ALL_VALUES_FROM
                            && first_property != second_property;
                        if filler_ok {
                            let (subject, object) = if direction_flipped {
                                (second.subject.clone(), first.subject.clone())
                            } else {
                                (first.subject.clone(), second.subject.clone())
                            };
                            derived.push(RdfGraphTriple {
                                subject,
                                predicate: RDFS_SUBCLASS_OF.to_string(),
                                object,
                            });
                        }
                    }
                }
            }
            let before = closure.len();
            closure.extend(derived);
            if closure.len() == before {
                break;
            }
        }
        Self { closure }
    }

    /// Resolve an RDF list (rdf:first / rdf:rest / rdf:nil) on the
    /// owned graph; bounded walk, cycles and dangling tails give None.
    fn resolve_list(&self, head: &RdfGraphTerm) -> Option<Vec<RdfGraphTerm>> {
        let mut members = Vec::new();
        let mut current = head.clone();
        for _ in 0..1024 {
            if let RdfGraphTerm::Iri(named) = &current {
                if named == RDF_NIL {
                    return Some(members);
                }
            }
            let mut first: Option<RdfGraphTerm> = None;
            let mut rest: Option<RdfGraphTerm> = None;
            for triple in self.closure.iter() {
                if triple.subject == current {
                    if triple.predicate == RDF_FIRST {
                        first = Some(triple.object.clone());
                    } else if triple.predicate == RDF_REST {
                        rest = Some(triple.object.clone());
                    }
                }
            }
            members.push(first?);
            current = rest?;
        }
        None
    }

    /// Whether the closure exposes a contradiction the landed families
    /// can see: sameAs coinciding with differentFrom, or a subject
    /// carrying two propertyDisjointWith properties to the same object
    /// (equivalentProperty substitutions feed the disjoint set, so a
    /// property made disjoint with its own equivalent counts).
    /// Positive-only like every lane claim: `None` means no violation
    /// seen, never a consistency proof.
    pub fn has_contradiction(&self) -> Option<bool> {
        let same_as: Vec<(&RdfGraphTerm, &RdfGraphTerm)> = self
            .closure
            .iter()
            .filter(|triple| triple.predicate == OWL_SAME_AS)
            .map(|triple| (&triple.subject, &triple.object))
            .collect();
        let different: Vec<(&RdfGraphTerm, &RdfGraphTerm)> = self
            .closure
            .iter()
            .filter(|triple| triple.predicate == OWL_DIFFERENT_FROM)
            .map(|triple| (&triple.subject, &triple.object))
            .collect();
        for (left, right) in &different {
            if same_as
                .iter()
                .any(|(a, b)| a == left && b == right || a == right && b == left)
            {
                return Some(true);
            }
        }
        // The disjoint property set: declared pairs, symmetrized, with
        // equivalentProperty substitution closing over both members.
        let mut disjoint: HashSet<(String, String)> = HashSet::new();
        let declared: Vec<(String, String)> = self
            .closure
            .iter()
            .filter(|triple| triple.predicate == OWL_PROPERTY_DISJOINT_WITH)
            .filter_map(|triple| match (&triple.subject, &triple.object) {
                (RdfGraphTerm::Iri(left), RdfGraphTerm::Iri(right)) => {
                    Some((left.clone(), right.clone()))
                }
                _ => None,
            })
            .collect();
        let equivalents: Vec<(String, String)> = self
            .closure
            .iter()
            .filter(|triple| triple.predicate == OWL_EQUIVALENT_PROPERTY)
            .filter_map(|triple| match (&triple.subject, &triple.object) {
                (RdfGraphTerm::Iri(left), RdfGraphTerm::Iri(right)) => {
                    Some((left.clone(), right.clone()))
                }
                _ => None,
            })
            .collect();
        let substitute = |property: &str| -> Vec<String> {
            let mut names = vec![property.to_string()];
            for (left, right) in &equivalents {
                if left == property && !names.contains(right) {
                    names.push(right.clone());
                }
                if right == property && !names.contains(left) {
                    names.push(left.clone());
                }
            }
            names
        };
        for (left, right) in &declared {
            for l in substitute(left) {
                for r in substitute(right) {
                    disjoint.insert((l.clone(), r.clone()));
                    disjoint.insert((r.clone(), l.clone()));
                }
            }
        }
        let edges: Vec<(&String, &str, &RdfGraphTerm)> = self
            .closure
            .iter()
            .filter_map(|triple| match &triple.subject {
                RdfGraphTerm::Iri(subject) => Some((subject, triple.predicate.as_str(), &triple.object)),
                _ => None,
            })
            .collect();
        for (subject, property, object) in &edges {
            if disjoint.contains(&(property.to_string(), property.to_string())) {
                return Some(true);
            }
            for (other_subject, other_property, other_object) in &edges {
                if other_subject == subject
                    && other_object == object
                    && disjoint.contains(&(property.to_string(), other_property.to_string()))
                {
                    return Some(true);
                }
            }
        }

        // Family 5 (#185): a max-0 restriction with a (qualifying)
        // edge from a typed holder.
        for triple in self
            .closure
            .iter()
            .filter(|triple| {
                triple.predicate == OWL_MAX_CARDINALITY
                    || triple.predicate == OWL_MAX_QUALIFIED_CARDINALITY
            })
        {
            let RdfGraphTerm::Literal { lexical, .. } = &triple.object else {
                continue;
            };
            if lexical != "0" {
                continue;
            }
            let Some(property) = self
                .closure
                .iter()
                .find(|link| link.subject == triple.subject && link.predicate == OWL_ON_PROPERTY)
                .and_then(|link| match &link.object {
                    RdfGraphTerm::Iri(named) => Some(named.clone()),
                    _ => None,
                })
            else {
                continue;
            };
            let qualifier = if triple.predicate == OWL_MAX_QUALIFIED_CARDINALITY {
                self.closure
                    .iter()
                    .find(|link| link.subject == triple.subject && link.predicate == OWL_ON_CLASS)
                    .map(|link| link.object.clone())
            } else {
                None
            };
            for typed in self.closure.iter() {
                if typed.predicate != RDF_TYPE || typed.object != triple.subject {
                    continue;
                }
                for edge in self.closure.iter() {
                    if edge.predicate != property || edge.subject != typed.subject {
                        continue;
                    }
                    if let Some(class) = &qualifier {
                        if !self.closure.iter().any(|check| {
                            check.predicate == RDF_TYPE
                                && check.subject == edge.object
                                && check.object == *class
                        }) {
                            continue;
                        }
                    }
                    return Some(true);
                }
            }
        }

        // Family 4 (#184): a property declared both symmetric and
        // asymmetric is unusable — symmetry mirrors every edge into
        // what asymmetry forbids, so any use contradicts.
        {
            let symmetric: HashSet<String> = self
                .closure
                .iter()
                .filter(|triple| {
                    triple.predicate == RDF_TYPE
                        && triple.object == RdfGraphTerm::Iri(OWL_SYMMETRIC_PROPERTY.to_string())
                })
                .filter_map(|triple| match &triple.subject {
                    RdfGraphTerm::Iri(named) => Some(named.clone()),
                    _ => None,
                })
                .collect();
            let asymmetric: HashSet<String> = self
                .closure
                .iter()
                .filter(|triple| {
                    triple.predicate == RDF_TYPE
                        && triple.object == RdfGraphTerm::Iri(OWL_ASYMMETRIC_PROPERTY.to_string())
                })
                .filter_map(|triple| match &triple.subject {
                    RdfGraphTerm::Iri(named) => Some(named.clone()),
                    _ => None,
                })
                .collect();
            for triple in self.closure.iter() {
                if symmetric.contains(&triple.predicate) && asymmetric.contains(&triple.predicate) {
                    return Some(true);
                }
            }
        }
        // Family 4 (#184): one individual in both sides of a
        // complementOf pair.
        for complement in self
            .closure
            .iter()
            .filter(|triple| triple.predicate == OWL_COMPLEMENT_OF)
        {
            for typed in self.closure.iter() {
                if typed.predicate == RDF_TYPE && typed.object == complement.object {
                    if self.closure.iter().any(|also| {
                        also.subject == typed.subject
                            && also.predicate == RDF_TYPE
                            && also.object == complement.subject
                    }) {
                        return Some(true);
                    }
                }
            }
        }

        // Family 3 (#183): AllDisjoint*/AllDifferent marker nodes with
        // member lists (owl:members, or AllDifferent's legacy
        // owl:distinctMembers). AllDifferent pairs clash under the
        // sameAs closure; disjoint class members clash on one
        // individual carrying two of them; disjoint property members
        // clash on one subject carrying both to the same object.
        for triple in self.closure.iter() {
            if triple.predicate != RDF_TYPE {
                continue;
            }
            let RdfGraphTerm::Iri(marker) = &triple.object else {
                continue;
            };
            let kind = match marker.as_str() {
                name if name == OWL_ALL_DIFFERENT => 'd',
                name if name == OWL_ALL_DISJOINT_CLASSES => 'c',
                name if name == OWL_ALL_DISJOINT_PROPERTIES => 'p',
                _ => continue,
            };
            for list_predicate in [OWL_MEMBERS, OWL_DISTINCT_MEMBERS] {
                let heads: Vec<&RdfGraphTriple> = self
                    .closure
                    .iter()
                    .filter(|link| link.subject == triple.subject && link.predicate == list_predicate)
                    .collect();
                for head in heads {
                    let Some(members) = self.resolve_list(&head.object) else {
                        continue;
                    };
                    match kind {
                        'd' => {
                            for (index, left) in members.iter().enumerate() {
                                for right in members.iter().skip(index + 1) {
                                    if same_as.iter().any(|(a, b)| {
                                        (a == &left && b == &right) || (a == &right && b == &left)
                                    }) {
                                        return Some(true);
                                    }
                                }
                            }
                        }
                        'c' => {
                            for (index, left) in members.iter().enumerate() {
                                for right in members.iter().skip(index + 1) {
                                    for typed in self.closure.iter() {
                                        if typed.predicate == RDF_TYPE
                                            && typed.object == *right
                                            && self
                                                .closure
                                                .iter()
                                                .any(|also| {
                                                    also.subject == typed.subject
                                                        && also.predicate == RDF_TYPE
                                                        && also.object == *left
                                                })
                                        {
                                            return Some(true);
                                        }
                                    }
                                }
                            }
                        }
                        'p' => {
                            let names: Vec<String> = members
                                .iter()
                                .filter_map(|member| match member {
                                    RdfGraphTerm::Iri(named) => Some(named.clone()),
                                    _ => None,
                                })
                                .collect();
                            for (subject, property, object) in &edges {
                                for other in &names {
                                    if property != other
                                        && names.iter().any(|name| name == *property)
                                        && edges.iter().any(|(s, p, o)| {
                                            s == subject && o == object && p == other
                                        })
                                    {
                                        return Some(true);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }

    /// Whether the closure entails a ground conclusion graph. v0
    /// claims only positive answers — every conclusion triple in the
    /// closure — and returns `None` otherwise (blank-node conclusions
    /// or uncovered ground shapes): a negative claim would need
    /// non-entailment evidence this v0 does not have, so the caller
    /// falls back to the Direct lane, which may answer through its own
    /// reductions.
    pub fn entails_graph(&self, conclusion: &[RdfGraphTriple]) -> Option<bool> {
        if conclusion.iter().any(|triple| {
            matches!(triple.subject, RdfGraphTerm::Blank(_))
                || matches!(triple.object, RdfGraphTerm::Blank(_))
        }) {
            return None;
        }
        conclusion
            .iter()
            .all(|triple| self.closure.contains(triple))
            .then_some(true)
    }
}
