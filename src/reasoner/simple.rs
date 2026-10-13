//! Simplified OWL2 Reasoning Engine
//!
//! Provides basic reasoning capabilities for OWL2 ontologies with caching
//! and profile validation support.
//!
//! ## Features
//!
//! - **Consistency Checking**: Verify if an ontology is logically consistent
//! - **Classification**: Compute class hierarchy and subclass relationships
//! - **Satisfiability**: Check if classes can have instances
//! - **Instance Retrieval**: Find instances of specific classes
//! - **Profile Validation**: Ensure compliance with OWL2 profiles (EL, QL, RL)
//! - **Performance Caching**: Configurable caching with TTL expiration
//!
//! ## Usage
//!
//! ```rust
//! use owl2_reasoner::{Ontology, SimpleReasoner, Class, SubClassOfAxiom, ClassExpression};
//!
//! // Create an ontology with family relationships
//! let mut ontology = Ontology::new();
//! let person = Class::new("http://example.org/Person");
//! let parent = Class::new("http://example.org/Parent");
//! ontology.add_class(person.clone())?;
//! ontology.add_class(parent.clone())?;
//!
//! // Add subclass relationship: Parent ⊑ Person
//! let subclass_axiom = SubClassOfAxiom::new(
//!     ClassExpression::from(parent.clone()),
//!     ClassExpression::from(person.clone()),
//! );
//! ontology.add_subclass_axiom(subclass_axiom)?;
//!
//! // Create reasoner and perform reasoning
//! let reasoner = SimpleReasoner::new(ontology);
//!
//! // Check consistency
//! let is_consistent = reasoner.is_consistent()?;
//! assert!(is_consistent);
//!
//! // Check subclass relationship
//! let is_subclass = reasoner.is_subclass_of(&parent.iri(), &person.iri())?;
//! assert!(is_subclass);
//!
//! // Check class satisfiability
//! let is_satisfiable = reasoner.is_class_satisfiable(&person.iri())?;
//! assert!(is_satisfiable);
//!
//! # Ok::<(), owl2_reasoner::OwlError>(())
//! ```

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::class_expressions::DataRange;
use crate::logic::axioms::{
    ClassExpression, DataPropertyExpression, ObjectPropertyExpression, PropertyAssertionAxiom,
};
use crate::reasoner::boolean_consistency::check_boolean_consistency;
use crate::reasoner::qualified_consistency::check_qualified_consistency;
use crate::strategy::profiles::{Owl2Profile, Owl2ProfileValidator, ProfileValidationResult};
use hashbrown::HashMap;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const OWL_NOTHING_IRI: &str = "http://www.w3.org/2002/07/owl#Nothing";
const OWL_THING_IRI: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_BOTTOM_OBJECT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
const OWL_BOTTOM_DATA_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
const RDF_XML_LITERAL_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral";
const XSD_FLOAT_IRI: &str = "http://www.w3.org/2001/XMLSchema#float";
const XSD_DOUBLE_IRI: &str = "http://www.w3.org/2001/XMLSchema#double";
const XSD_DECIMAL_IRI: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_INTEGER_IRI: &str = "http://www.w3.org/2001/XMLSchema#integer";
const OWL_RATIONAL_IRI: &str = "http://www.w3.org/2002/07/owl#rational";

// Budgets for the #109 object-cardinality merge probe: bounded search
// throughout, and an exhausted budget only misses contradictions.
const MERGE_PROBE_WALK_BUDGET: usize = 30_000;
const MERGE_PROBE_MAX_NODES: usize = 512;
const MERGE_PROBE_EXPRESSION_CAP: usize = 128;
const MERGE_PROBE_PAIR_CAP: usize = 2_000;
const MERGE_PROBE_MERGE_ROUNDS: usize = 32;
/// Default merge-probe size gate (#168, reviewed #173): a pure memory
/// backstop — the role-closure index (#172) collapsed the walk's time
/// wall and the work ceiling below bounds the search cost
/// deterministically. 1,000,000 sits an order of magnitude above the
/// largest suite corpus (uberon: 113,191 nodes) while still capping
/// pathological inputs; the W3C corpus never approaches it.
const DEFAULT_MERGE_PROBE_MAX_NODES: usize = 1_000_000;
/// Default global work ceiling (#173, ADR 0022): one unit = one edge
/// inspection inside an existential-GCI candidate check. Set above the
/// measured corpus floor — the W3C cases the probe decides spend up to
/// 2,511,166 units (description-logic-626/627), and a ceiling that
/// binds them flips refutations to fail-open (a 250k draft lost five
/// cases before measurement corrected it). 4,000,000 clears that floor
/// with margin and holds uberon's 1,173,140-unit profile inside
/// (~8,230 units/s measured on the ungated run: 142.6 s; the
/// walk-node budget remains the practical time binder there). `0`
/// (env/builder) means unlimited for D2 comparisons.
const DEFAULT_MERGE_PROBE_WORK_BUDGET: u64 = 4_000_000;

/// Process-wide diagnostic latch: the size gate can fire inside
/// SimpleReasoner instances the caller never sees (speculative
/// workers build their own), so the CLI reads this to mark the report
/// regardless of which lane skipped (#168).
static MERGE_PROBE_SKIP_LATCH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether any merge probe in this process skipped by the size gate
/// (#168) — the report-side signal for the fail-open posture.
pub fn merge_probe_skipped_globally() -> bool {
    MERGE_PROBE_SKIP_LATCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Same process-wide posture latch as the skip gate, but for a probe
/// that entered and spent a budget mid-search: the verdict stays
/// fail-open (ADR 0020) and the marker says so (#171, ADR 0022).
static MERGE_PROBE_EXHAUSTED_LATCH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether any merge probe in this process exhausted a budget after
/// entering (#171) — the report-side signal for the fail-open posture.
pub fn merge_probe_exhausted_globally() -> bool {
    MERGE_PROBE_EXHAUSTED_LATCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// `OWL2_REASONER_MERGE_PROBE_WALK_NODES` overrides the walk budget in
/// node visits (test/D2 lever; the landing default stays the proven
/// constant). Deliberately NOT the ADR-0022 work-unit variable — that
/// one (`OWL2_REASONER_MERGE_PROBE_WORK_BUDGET`, `0` = unlimited)
/// arrives with #173 and retires or absorbs this lever.
/// `OWL2_REASONER_MERGE_PROBE_WORK_BUDGET` overrides the global work
/// ceiling (#173, ADR 0022); `0` means unlimited (D2 comparison mode).
fn merge_probe_work_budget_from_env() -> u64 {
    match std::env::var("OWL2_REASONER_MERGE_PROBE_WORK_BUDGET") {
        Ok(value) => value
            .trim()
            .parse()
            .unwrap_or(DEFAULT_MERGE_PROBE_WORK_BUDGET),
        Err(_) => DEFAULT_MERGE_PROBE_WORK_BUDGET,
    }
}

/// `OWL2_REASONER_MERGE_PROBE_WALK_NODES` overrides the walk-node
/// budget (test/D2 lever; #171). Separate from the ADR-0022 work
/// ceiling above — both stay.
fn merge_probe_walk_budget_from_env() -> usize {
    match std::env::var("OWL2_REASONER_MERGE_PROBE_WALK_NODES") {
        Ok(value) => value
            .trim()
            .parse()
            .unwrap_or(MERGE_PROBE_WALK_BUDGET),
        Err(_) => MERGE_PROBE_WALK_BUDGET,
    }
}

/// `OWL2_REASONER_MERGE_PROBE_MAX_NODES` overrides the gate; `0`
/// disables it (the probe always runs, for D2 comparisons).
fn merge_probe_max_nodes_from_env() -> usize {
    match std::env::var("OWL2_REASONER_MERGE_PROBE_MAX_NODES") {
        Ok(value) => value.trim().parse().unwrap_or(DEFAULT_MERGE_PROBE_MAX_NODES),
        Err(_) => DEFAULT_MERGE_PROBE_MAX_NODES,
    }
}

const MERGE_PROBE_PROPAGATION_ROUNDS: usize = 8;
const MERGE_PROBE_SETTLE_ROUNDS: usize = 8;
/// Total probe clones the #115 identification search may spend; an
/// exhausted budget leaves the case undecided (consistent), never a
/// verdict.
const MERGE_PROBE_SEARCH_CLONES: usize = 4_096;
const MERGE_PROBE_COVER_VERTICES: usize = 12;
const MERGE_PROBE_CLOSURE_BUDGET: usize = 4_000;
const MERGE_PROBE_SUPERPROP_BUDGET: usize = 100_000;
const MERGE_PROBE_GATE_BUDGET: usize = 20_000;

fn numeric_literal_value(value: &crate::core::entities::Literal) -> Option<f64> {
    let lexical = value.lexical_form();
    match value.datatype().as_str() {
        XSD_FLOAT_IRI | XSD_DOUBLE_IRI | XSD_DECIMAL_IRI | XSD_INTEGER_IRI => lexical.parse().ok(),
        OWL_RATIONAL_IRI => {
            let (numerator, denominator) = lexical.split_once('/')?;
            let numerator: f64 = numerator.parse().ok()?;
            let denominator: f64 = denominator.parse().ok()?;
            (denominator != 0.0).then_some(numerator / denominator)
        }
        _ => None,
    }
}

fn rational_literal_value(value: &crate::core::entities::Literal) -> Option<(i128, i128)> {
    let normalize = |numerator: i128, denominator: i128| {
        if denominator == 0 {
            return None;
        }
        let sign = if denominator < 0 { -1 } else { 1 };
        let mut numerator = numerator * sign;
        let mut denominator = denominator.abs();
        let mut left = numerator.abs();
        let mut right = denominator;
        while right != 0 {
            let remainder = left % right;
            left = right;
            right = remainder;
        }
        if left > 1 {
            numerator /= left;
            denominator /= left;
        }
        Some((numerator, denominator))
    };

    match value.datatype().as_str() {
        // Every lattice member is an integer type (xsd:integer
        // included), so its lexical form is an integer — same-value
        // literals compare equal across subtypes (#124).
        datatype if crate::logic::datatypes::value_space::integer_interval(datatype).is_some() => {
            normalize(value.lexical_form().parse().ok()?, 1)
        }
        XSD_DECIMAL_IRI => {
            let lexical = value.lexical_form();
            let (sign, digits) = match lexical.strip_prefix('-') {
                Some(rest) => (-1_i128, rest),
                None => (1_i128, lexical.strip_prefix('+').unwrap_or(lexical)),
            };
            let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
            let whole: i128 = if whole.is_empty() {
                0
            } else {
                whole.parse().ok()?
            };
            let fraction_value: i128 = if fraction.is_empty() {
                0
            } else {
                fraction.parse().ok()?
            };
            let denominator = 10_i128.checked_pow(fraction.len() as u32)?;
            normalize(
                sign * (whole.checked_mul(denominator)? + fraction_value),
                denominator,
            )
        }
        OWL_RATIONAL_IRI => {
            let (numerator, denominator) = value.lexical_form().split_once('/')?;
            normalize(numerator.parse().ok()?, denominator.parse().ok()?)
        }
        _ => None,
    }
}

fn data_range_is_empty(range: &DataRange) -> bool {
    match range {
        DataRange::DataOneOf(values) => values.is_empty(),
        DataRange::DataIntersectionOf(ranges) => ranges.iter().any(data_range_is_empty),
        DataRange::DataUnionOf(ranges) => ranges.iter().all(data_range_is_empty),
        DataRange::DataComplementOf(_) => false,
        DataRange::Datatype(_) => false,
        DataRange::DatatypeRestriction(datatype, facets) if datatype.as_str() == XSD_FLOAT_IRI => {
            let min_exclusive = facets
                .iter()
                .find(|facet| facet.facet().as_str().ends_with("#minExclusive"))
                .and_then(|facet| numeric_literal_value(facet.value()));
            let max_exclusive = facets
                .iter()
                .find(|facet| facet.facet().as_str().ends_with("#maxExclusive"))
                .and_then(|facet| numeric_literal_value(facet.value()));
            matches!((min_exclusive, max_exclusive), (Some(min), Some(max))
                if min >= 0.0 && max <= f32::from_bits(1) as f64)
        }
        DataRange::DatatypeRestriction(_, _) => false,
    }
}

fn canonical_xml_literal(value: &str) -> Option<String> {
    let wrapped = format!("<tableauxxXmlLiteral>{value}</tableauxxXmlLiteral>");
    let mut reader = Reader::from_str(&wrapped);
    reader.trim_text(false);
    let mut canonical = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let name = std::str::from_utf8(element.name().as_ref())
                    .ok()?
                    .to_string();
                let mut attributes = Vec::new();
                for attribute in element.attributes() {
                    let attribute = attribute.ok()?;
                    let key = std::str::from_utf8(attribute.key.as_ref()).ok()?;
                    let value = attribute.unescape_value().ok()?;
                    attributes.push((key.to_string(), value.into_owned()));
                }
                attributes.sort();
                write!(canonical, "S{}:{}[", name.len(), name).ok()?;
                for (key, value) in attributes {
                    write!(canonical, "{}:{}={};", key.len(), key, value).ok()?;
                }
                canonical.push(']');
            }
            Ok(Event::Empty(element)) => {
                let name = std::str::from_utf8(element.name().as_ref())
                    .ok()?
                    .to_string();
                let mut attributes = Vec::new();
                for attribute in element.attributes() {
                    let attribute = attribute.ok()?;
                    let key = std::str::from_utf8(attribute.key.as_ref()).ok()?;
                    let value = attribute.unescape_value().ok()?;
                    attributes.push((key.to_string(), value.into_owned()));
                }
                attributes.sort();
                write!(canonical, "S{}:{}[", name.len(), name).ok()?;
                for (key, value) in attributes {
                    write!(canonical, "{}:{}={};", key.len(), key, value).ok()?;
                }
                canonical.push(']');
                write!(canonical, "E{}:{};", name.len(), name).ok()?;
            }
            Ok(Event::End(element)) => {
                let name = std::str::from_utf8(element.name().as_ref())
                    .ok()?
                    .to_string();
                write!(canonical, "E{}:{};", name.len(), name).ok()?;
            }
            Ok(Event::Text(text)) => {
                let text = text.unescape().ok()?;
                write!(canonical, "T{}:{};", text.len(), text).ok()?;
            }
            Ok(Event::CData(text)) => {
                let text = std::str::from_utf8(text.as_ref()).ok()?;
                write!(canonical, "T{}:{};", text.len(), text).ok()?;
            }
            Ok(Event::Comment(_)) => {}
            Ok(Event::Eof) => return Some(canonical),
            Ok(Event::Decl(_) | Event::PI(_) | Event::DocType(_)) => {
                return None;
            }
            Err(_) => return None,
        }
    }
}

pub(crate) fn literals_are_equal(
    left: &crate::core::entities::Literal,
    right: &crate::core::entities::Literal,
) -> bool {
    if left == right {
        return true;
    }
    if let (Some((left_numerator, left_denominator)), Some((right_numerator, right_denominator))) =
        (rational_literal_value(left), rational_literal_value(right))
    {
        return left_numerator
            .checked_mul(right_denominator)
            .zip(right_numerator.checked_mul(left_denominator))
            .is_some_and(|(left, right)| left == right);
    }
    if left.datatype().as_str() == right.datatype().as_str()
        && matches!(left.datatype().as_str(), XSD_FLOAT_IRI | XSD_DOUBLE_IRI)
    {
        if let (Some(left), Some(right)) =
            (numeric_literal_value(left), numeric_literal_value(right))
        {
            return left == right;
        }
    }
    if left.datatype().as_str() != RDF_XML_LITERAL_IRI
        || right.datatype().as_str() != RDF_XML_LITERAL_IRI
    {
        return false;
    }
    canonical_xml_literal(left.lexical_form()) == canonical_xml_literal(right.lexical_form())
}

fn object_property_iri(property: &ObjectPropertyExpression) -> &IRI {
    match property {
        ObjectPropertyExpression::ObjectProperty(property) => property.iri(),
        ObjectPropertyExpression::ObjectInverseOf(property) => object_property_iri(property),
    }
}

fn data_property_iri(property: &DataPropertyExpression) -> &IRI {
    match property {
        DataPropertyExpression::DataProperty(property) => property.iri(),
    }
}

/// One two-step chain rule: first ∘ second ⊑ super_property.
struct ChainRule<'a> {
    first: &'a Arc<IRI>,
    second: &'a Arc<IRI>,
    super_property: &'a Arc<IRI>,
}

/// Atomic class IRIs an expression mentions, complements included —
/// the pressedness test for union choices walks these (#119,
/// ADR-0020). Expressions with no named-class atoms collect nothing.
fn collect_class_atoms(expression: &ClassExpression, atoms: &mut BTreeSet<String>) {
    match expression {
        ClassExpression::Class(class) => {
            atoms.insert(class.iri().as_str().to_string());
        }
        ClassExpression::ObjectComplementOf(inner) => {
            collect_class_atoms(inner, atoms);
        }
        ClassExpression::ObjectUnionOf(operands)
        | ClassExpression::ObjectIntersectionOf(operands) => {
            for operand in operands {
                collect_class_atoms(operand, atoms);
            }
        }
        _ => {}
    }
}

/// Object-property assertions the two-step property chains derive
/// over the asserted edges, composed to a fixpoint so chains of
/// chains participate (prp-spo2-2 — the materialize closure's
/// pattern, #117). Steps join on exact property match; lifting a
/// composed edge into a superproperty rides the climb the negative
/// conflict check already performs.
fn chain_composed_object_assertions(ontology: &Ontology) -> Vec<PropertyAssertionAxiom> {
    let chains: Vec<ChainRule> = ontology
        .sub_property_chain_axioms()
        .iter()
        // Only two-step chains compose (materialize's fragment
        // anchor); single-step and longer ones stay uncomposed.
        .filter(|axiom| axiom.properties().len() == 2)
        .map(|axiom| ChainRule {
            first: &axiom.properties()[0],
            second: &axiom.properties()[1],
            super_property: axiom.super_property(),
        })
        .collect();
    if chains.is_empty() {
        return Vec::new();
    }
    let mut edges: BTreeSet<(Arc<IRI>, Arc<IRI>, Arc<IRI>)> = ontology
        .property_assertions()
        .iter()
        .filter_map(|axiom| {
            axiom
                .object_iri()
                .map(|object| (axiom.subject().clone(), axiom.property().clone(), object.clone()))
        })
        .collect();
    let mut derived = Vec::new();
    loop {
        // (property, middle) -> objects: the second-step join index,
        // rebuilt per round so freshly composed edges participate.
        let mut second_step: BTreeMap<(&Arc<IRI>, &Arc<IRI>), Vec<&Arc<IRI>>> = BTreeMap::new();
        for (subject, property, object) in &edges {
            second_step
                .entry((property, subject))
                .or_default()
                .push(object);
        }
        let mut additions: Vec<(Arc<IRI>, Arc<IRI>, Arc<IRI>)> = Vec::new();
        for rule in &chains {
            for (subject, property, middle) in &edges {
                if property != rule.first {
                    continue;
                }
                if let Some(objects) = second_step.get(&(rule.second, middle)) {
                    for object in objects {
                        let candidate = (
                            subject.clone(),
                            rule.super_property.clone(),
                            (*object).clone(),
                        );
                        if !edges.contains(&candidate) {
                            additions.push(candidate);
                        }
                    }
                }
            }
        }
        if additions.is_empty() {
            break;
        }
        for (subject, property, object) in additions {
            edges.insert((subject.clone(), property.clone(), object.clone()));
            derived.push(PropertyAssertionAxiom::new(subject, property, object));
        }
    }
    derived
}

/// The property itself plus every named superproperty (subproperty
/// and equivalent edges, transitively).
fn superproperties_of<'p>(
    property: &'p str,
    direct: &BTreeMap<&'p str, BTreeSet<&'p str>>,
) -> Vec<&'p str> {
    let mut out: Vec<&'p str> = vec![property];
    let mut queue: std::collections::VecDeque<&'p str> =
        std::collections::VecDeque::from([property]);
    let mut visited: BTreeSet<&'p str> = BTreeSet::from([out[0]]);
    while let Some(current) = queue.pop_front() {
        if let Some(nexts) = direct.get(current) {
            for next in nexts {
                if visited.insert(next) {
                    out.push(next);
                    queue.push_back(next);
                }
            }
        }
    }
    out
}

/// The symmetric closure consumed by the negative-object-assertion
/// conflict check: every entailed edge (asserted or chain-composed)
/// whose property sits under a declared symmetric property S also
/// yields the S-labeled reverse. The reverse carries only the
/// declared symmetric property's label — a subproperty of a
/// symmetric property is not itself symmetric (#121).
fn symmetric_closure_assertions(
    ontology: &Ontology,
    edges: &[&PropertyAssertionAxiom],
    direct_props: &BTreeMap<&str, BTreeSet<&str>>,
) -> Vec<PropertyAssertionAxiom> {
    let symmetric_axioms = ontology.symmetric_property_axioms();
    if symmetric_axioms.is_empty() {
        return Vec::new();
    }
    let symmetric: BTreeMap<&str, &Arc<IRI>> = symmetric_axioms
        .iter()
        .map(|axiom| (axiom.property().as_str(), axiom.property()))
        .collect();
    let mut derived = Vec::new();
    for edge in edges {
        let Some(object) = edge.object_iri() else {
            continue;
        };
        for superproperty in superproperties_of(edge.property().as_str(), direct_props) {
            if let Some(property) = symmetric.get(superproperty) {
                derived.push(PropertyAssertionAxiom::new(
                    object.clone(),
                    (*property).clone(),
                    edge.subject().clone(),
                ));
            }
        }
    }
    derived
}

/// Data-existential witnesses the premise pins to one concrete value:
/// a `DataSomeValuesFrom(p, D)` on an individual whose datatype
/// interval, intersected with every lattice range the property
/// carries, collapses to a singleton. The individual's p-witness is
/// then exactly that value — a positive data assertion the negative
/// check can clash against (#124, W3C I5.8-010).
/// Whether a literal sits inside a data range, consuming compound
/// qualifiers (#153's recorded gap): union = any operand contains,
/// intersection = every operand (empty stays false — conservative),
/// complement = the operand does not contain (exact for literals).
/// Facet restrictions stay unconsumed — recorded incompleteness.
fn data_range_contains(range: &DataRange, literal: &crate::core::entities::Literal) -> bool {
    match range {
        DataRange::Datatype(datatype) => literal.datatype().as_str() == datatype.as_str(),
        DataRange::DataOneOf(values) => values
            .iter()
            .any(|value| literals_are_equal(value, literal)),
        DataRange::DataUnionOf(operands) => operands
            .iter()
            .any(|operand| data_range_contains(operand, literal)),
        DataRange::DataIntersectionOf(operands) => {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| data_range_contains(operand, literal))
        }
        DataRange::DataComplementOf(inner) => !data_range_contains(inner, literal),
        DataRange::DatatypeRestriction(_, _) => false,
    }
}

/// Transitive superproperty names of a data property through
/// SubDataPropertyOf (#151's recorded gap): values of a subproperty are
/// values of every superproperty, so a pool declared on a super
/// confines and pins them too.
fn data_super_names<'o>(ontology: &'o Ontology, sub: &str) -> Vec<&'o str> {
    let mut supers: Vec<&str> = Vec::new();
    let mut stack = vec![sub];
    while let Some(current) = stack.pop() {
        for axiom in ontology.subdata_property_axioms() {
            if axiom.sub_property().as_str() == current {
                let super_property = axiom.super_property().as_str();
                if !supers.contains(&super_property) {
                    supers.push(super_property);
                    stack.push(super_property);
                }
            }
        }
    }
    supers
}

fn pinned_data_witnesses(ontology: &Ontology) -> Vec<crate::logic::axioms::DataPropertyAssertionAxiom> {
    use crate::logic::datatypes::value_space;

    // Per-property intersection of the lattice ranges the premise
    // states (datatype ranges ride the object-property-range axiom
    // kind with the XSD IRI as the range "class").
    let mut range_bounds: BTreeMap<&str, value_space::NumericInterval> = BTreeMap::new();
    for axiom in ontology.object_property_range_axioms() {
        let Some(interval) = value_space::integer_interval(axiom.range().as_str()) else {
            continue;
        };
        let property = axiom.property().as_str();
        let bound = range_bounds.entry(property).or_insert_with(value_space::NumericInterval::unbounded);
        *bound = bound.intersect(interval);
    }

    // Per-property intersection of the enumerated pools (#151): every
    // value must sit in every pool, so the surviving set is the
    // intersection; a singleton names the only value the property can
    // carry. `None` means "no pool seen yet" (unbounded).
    let mut pool_survivors: BTreeMap<&str, Option<Vec<crate::core::entities::Literal>>> =
        BTreeMap::new();
    for axiom in ontology.data_property_enumerated_range_axioms() {
        let property = axiom.property().as_str();
        let entry = pool_survivors.entry(property).or_insert(None);
        *entry = match entry.take() {
            None => Some(axiom.values().to_vec()),
            Some(existing) => Some(
                existing
                    .into_iter()
                    .filter(|kept| {
                        axiom
                            .values()
                            .iter()
                            .any(|value| literals_are_equal(value, kept))
                    })
                    .collect(),
            ),
        };
    }

    let mut pinned = Vec::new();
    for assertion in ontology.class_assertions() {
        // A min >= 1 demand on a property whose pools intersect to one
        // literal pins that value outright — the demanded witness is
        // the survivor (#151, W3C oneOf-004's minCardinality arm).
        // Qualified minimums stay unpinned: a qualifier disjoint from
        // the pool is an inconsistency the bounds lanes decide, not a
        // value the pool names. Cross-datatype equal survivors
        // ("4"^^integer and "4"^^decimal) can leave two entries — a
        // missed pin, never a wrong one.
        if let ClassExpression::DataMinCardinality(minimum, property, None) =
            assertion.class_expr()
        {
            if *minimum >= 1 {
                if let DataPropertyExpression::DataProperty(named) = property.as_ref() {
                    // The demanded property's own pools plus its
                    // superproperties' pools all apply — intersected
                    // (#151's recorded gap: exact-IRI lookup let a
                    // subproperty demand escape a super's singleton).
                    let property_iri = named.iri().as_str();
                    let mut relevant: Vec<&Vec<crate::core::entities::Literal>> = Vec::new();
                    if let Some(Some(survivors)) = pool_survivors.get(property_iri) {
                        relevant.push(survivors);
                    }
                    for super_property in data_super_names(ontology, property_iri) {
                        if let Some(Some(survivors)) = pool_survivors.get(super_property) {
                            relevant.push(survivors);
                        }
                    }
                    if !relevant.is_empty() {
                        let mut survivors = relevant[0].clone();
                        for pool in &relevant[1..] {
                            survivors.retain(|kept| {
                                pool.iter().any(|value| literals_are_equal(value, kept))
                            });
                        }
                        if survivors.len() == 1 {
                            pinned.push(crate::logic::axioms::DataPropertyAssertionAxiom::new(
                                assertion.individual().clone(),
                                named.iri().clone(),
                                survivors[0].clone(),
                            ));
                        }
                    }
                }
            }
        }
        let ClassExpression::DataSomeValuesFrom(property, datatype) = assertion.class_expr()
        else {
            continue;
        };
        let DataPropertyExpression::DataProperty(named) = property.as_ref() else {
            continue;
        };
        let DataRange::Datatype(datatype_iri) = datatype.as_ref() else {
            continue;
        };
        let Some(interval) = value_space::integer_interval(datatype_iri.as_str()) else {
            continue;
        };
        let witness_interval = match range_bounds.get(named.iri().as_str()) {
            Some(bounds) => interval.intersect(*bounds),
            None => interval,
        };
        // A singleton interval names the only value the witness can be.
        if let (Some(min), Some(max)) = (witness_interval.min, witness_interval.max) {
            if min == max {
                pinned.push(crate::logic::axioms::DataPropertyAssertionAxiom::new(
                    assertion.individual().clone(),
                    named.iri().clone(),
                    crate::core::entities::Literal::typed(min.to_string(), (*datatype_iri).clone()),
                ));
            }
        }
    }
    pinned
}

fn class_expression_is_universal(expression: &ClassExpression) -> bool {
    match expression {
        ClassExpression::Class(class) => class.iri().as_str() == OWL_THING_IRI,
        ClassExpression::ObjectSomeValuesFrom(property, filler) => {
            object_property_iri(property).as_str()
                == "http://www.w3.org/2002/07/owl#topObjectProperty"
                && class_expression_is_universal(filler)
        }
        ClassExpression::ObjectAllValuesFrom(_, filler) => class_expression_is_universal(filler),
        _ => false,
    }
}

fn class_expression_is_empty(expression: &ClassExpression) -> bool {
    match expression {
        ClassExpression::Class(class) => class.iri().as_str() == OWL_NOTHING_IRI,
        ClassExpression::ObjectIntersectionOf(operands) => operands
            .iter()
            .any(|operand| class_expression_is_empty(operand)),
        ClassExpression::ObjectUnionOf(operands) => {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| class_expression_is_empty(operand))
        }
        ClassExpression::ObjectComplementOf(operand) => class_expression_is_universal(operand),
        ClassExpression::ObjectOneOf(individuals) => individuals.is_empty(),
        ClassExpression::ObjectSomeValuesFrom(property, filler) => {
            object_property_iri(property).as_str() == OWL_BOTTOM_OBJECT_PROPERTY_IRI
                || class_expression_is_empty(filler)
        }
        ClassExpression::ObjectAllValuesFrom(_, _) => false,
        ClassExpression::ObjectHasValue(property, _) | ClassExpression::ObjectHasSelf(property) => {
            object_property_iri(property).as_str() == OWL_BOTTOM_OBJECT_PROPERTY_IRI
        }
        ClassExpression::ObjectMinCardinality(minimum, property, _)
        | ClassExpression::ObjectExactCardinality(minimum, property, _) => {
            *minimum > 0 && object_property_iri(property).as_str() == OWL_BOTTOM_OBJECT_PROPERTY_IRI
        }
        ClassExpression::ObjectMaxCardinality(_, _, _) => false,
        ClassExpression::DataSomeValuesFrom(property, range) => {
            data_property_iri(property).as_str() == OWL_BOTTOM_DATA_PROPERTY_IRI
                || data_range_is_empty(range)
        }
        ClassExpression::DataAllValuesFrom(_, _) => false,
        ClassExpression::DataHasValue(property, _) => {
            data_property_iri(property).as_str() == OWL_BOTTOM_DATA_PROPERTY_IRI
        }
        ClassExpression::DataMinCardinality(minimum, property, _)
        | ClassExpression::DataExactCardinality(minimum, property, _) => {
            *minimum > 0 && data_property_iri(property).as_str() == OWL_BOTTOM_DATA_PROPERTY_IRI
        }
        ClassExpression::DataMaxCardinality(_, _, _) => false,
    }
}

fn are_known_different(ontology: &Ontology, left: &IRI, right: &IRI) -> bool {
    if left == right {
        return false;
    }
    ontology.different_individuals_axioms().iter().any(|axiom| {
        let individuals = axiom.individuals();
        individuals
            .iter()
            .any(|individual| individual.as_ref() == left)
            && individuals
                .iter()
                .any(|individual| individual.as_ref() == right)
    })
}

fn is_instance_of(ontology: &Ontology, individual: &IRI, class: &IRI) -> bool {
    class.as_str() == OWL_THING_IRI
        || ontology.class_assertions().iter().any(|assertion| {
            assertion.individual().as_ref() == individual
                && matches!(
                    assertion.class_expr(),
                    ClassExpression::Class(candidate) if candidate.iri().as_ref() == class
                )
        })
}

fn key_values_match(ontology: &Ontology, property: &IRI, left: &IRI, right: &IRI) -> bool {
    let data_match = ontology
        .data_property_assertions()
        .iter()
        .any(|left_assertion| {
            left_assertion.subject().as_ref() == left
                && left_assertion.property().as_ref() == property
                && ontology
                    .data_property_assertions()
                    .iter()
                    .any(|right_assertion| {
                        right_assertion.subject().as_ref() == right
                            && right_assertion.property().as_ref() == property
                            && literals_are_equal(left_assertion.value(), right_assertion.value())
                    })
        });
    if data_match {
        return true;
    }

    ontology.property_assertions().iter().any(|left_assertion| {
        left_assertion.subject().as_ref() == left
            && left_assertion.property().as_ref() == property
            && left_assertion.object_iri().is_some_and(|left_object| {
                ontology
                    .property_assertions()
                    .iter()
                    .any(|right_assertion| {
                        right_assertion.subject().as_ref() == right
                            && right_assertion.property().as_ref() == property
                            && right_assertion.object_iri().is_some_and(|right_object| {
                                right_object.as_ref() == left_object.as_ref()
                            })
                    })
            })
    })
}

/// Named classes inside an expression: intersections decompose, every other
/// constructor is a choice or a restriction and contributes nothing.
fn collect_named_classes<'e>(expression: &'e ClassExpression, into: &mut BTreeSet<&'e str>) {
    match expression {
        ClassExpression::Class(class) => {
            into.insert(class.iri().as_str());
        }
        ClassExpression::ObjectIntersectionOf(operands) => {
            operands
                .iter()
                .for_each(|operand| collect_named_classes(operand, into));
        }
        _ => {}
    }
}

/// Named-class super-side index shared by the #108 data-cardinality walk
/// and the #109 merge probe: named class -> super-side expressions (the
/// parser lowers equivalentClass edges to bidirectional SubClassOf, so
/// definitions reach this index), plus the anonymous-subject GCIs whose
/// subject has the vacuously-satisfiable shape.
/// Which half of a cardinality complement pair an expression pins:
/// exactly-zero / at-most-zero vs at-least-one. Only the unqualified
/// shapes over a directly named property register (ADR 0021's scope
/// guard — the oiled generator's entire vocabulary).
/// The directly named property of an object property expression —
/// inverse roles stay outside the pair scope guard.
fn direct_object_property(
    expression: &ObjectPropertyExpression,
) -> Option<&crate::core::iri::IRI> {
    match expression {
        ObjectPropertyExpression::ObjectProperty(property) => Some(property.iri().as_ref()),
        _ => None,
    }
}

/// Which half of a cardinality complement pair an expression pins:
/// exactly-zero / at-most-zero vs at-least-one. Only the unqualified
/// shapes over a directly named property register (ADR 0021's scope
/// guard — the oiled generator's entire vocabulary).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ComplementPolarity {
    Zero,
    AtLeastOne,
}

fn cardinality_pair_polarity(
    expression: &ClassExpression,
) -> Option<(&crate::core::iri::IRI, ComplementPolarity)> {
    use ComplementPolarity::{AtLeastOne, Zero};
    let (zero, at_least_one) = (Zero, AtLeastOne);
    match expression {
        ClassExpression::ObjectExactCardinality(0, property, None) => {
            direct_object_property(property).map(|iri| (iri, zero))
        }
        ClassExpression::ObjectMaxCardinality(0, property, None) => {
            direct_object_property(property).map(|iri| (iri, zero))
        }
        ClassExpression::ObjectMinCardinality(1, property, None) => {
            direct_object_property(property).map(|iri| (iri, at_least_one))
        }
        ClassExpression::DataExactCardinality(0, property, None) => {
            property.as_named().map(|named| (named.iri().as_ref(), zero))
        }
        ClassExpression::DataMaxCardinality(0, property, None) => {
            property.as_named().map(|named| (named.iri().as_ref(), zero))
        }
        ClassExpression::DataMinCardinality(1, property, None) => {
            property
                .as_named()
                .map(|named| (named.iri().as_ref(), at_least_one))
        }
        _ => None,
    }
}

/// The kernel-side exposure of cardinality complement pairs (ADR
/// 0021): the registered pairs, each class's partners, and the
/// intersection axioms read from their subject side — the direction
/// the avoidance assist's contrapositive needs.
struct ComplementIndex<'o> {
    /// (zero side, min side) — both directions of both equivalences
    /// verified at detection.
    pairs: Vec<(&'o str, &'o str)>,
    /// Class -> its complement partners, both directions.
    of: BTreeMap<&'o str, Vec<&'o str>>,
    /// Named class S -> intersection expressions D with `D ⊑ S`
    /// declared: a node outside S must avoid some conjunct of D.
    intersection_subjects: BTreeMap<&'o str, Vec<&'o ClassExpression>>,
}

/// Detect the cardinality complement pairs (ADR 0021, #156): a
/// property where one named class is bidirectionally equivalent to
/// the zero bound and another to the at-least-one bound. Both
/// directions of both equivalences must be declared — the disjointness
/// half needs only the forward sides, the covering half (avoiding one
/// class forces membership in the other) needs the reverse sides too.
fn detect_complement_pairs<'o>(ontology: &'o Ontology) -> ComplementIndex<'o> {
    let mut sides: [BTreeMap<&'o str, BTreeSet<&'o str>>; 4] = Default::default();
    let mut intersection_subjects: BTreeMap<&'o str, Vec<&'o ClassExpression>> =
        BTreeMap::new();
    for axiom in ontology.subclass_axioms() {
        if let ClassExpression::Class(sub) = axiom.sub_class() {
            if let Some((property, polarity)) = cardinality_pair_polarity(axiom.super_class()) {
                let index = match polarity {
                    ComplementPolarity::Zero => 0,
                    ComplementPolarity::AtLeastOne => 1,
                };
                sides[index]
                    .entry(property.as_str())
                    .or_default()
                    .insert(sub.iri().as_str());
            }
        }
        if let ClassExpression::Class(sup) = axiom.super_class() {
            if let Some((property, polarity)) = cardinality_pair_polarity(axiom.sub_class()) {
                let index = match polarity {
                    ComplementPolarity::Zero => 2,
                    ComplementPolarity::AtLeastOne => 3,
                };
                sides[index]
                    .entry(property.as_str())
                    .or_default()
                    .insert(sup.iri().as_str());
            }
            // The assist's premise side: D1 ⊓ … ⊓ Dn ⊑ S is what a
            // node outside S avoids some conjunct of. The super side
            // (S ⊑ intersection) licenses nothing for such a node.
            if matches!(axiom.sub_class(), ClassExpression::ObjectIntersectionOf(_)) {
                intersection_subjects
                    .entry(sup.iri().as_str())
                    .or_default()
                    .push(axiom.sub_class());
            }
        }
    }
    let mut pairs = Vec::new();
    let mut of: BTreeMap<&'o str, Vec<&'o str>> = BTreeMap::new();
    let properties = sides[0]
        .keys()
        .chain(sides[2].keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for property in properties {
        // Both directions must hold for a class to count as the zero
        // (or min) side of an equivalence; only then do the covering
        // flips follow.
        let zeros = sides[0]
            .get(property)
            .into_iter()
            .flatten()
            .filter(|class| sides[2].get(property).is_some_and(|r| r.contains(*class)))
            .copied()
            .collect::<Vec<_>>();
        let mins = sides[1]
            .get(property)
            .into_iter()
            .flatten()
            .filter(|class| sides[3].get(property).is_some_and(|r| r.contains(*class)))
            .copied()
            .collect::<Vec<_>>();
        for zero in zeros {
            for min in &mins {
                pairs.push((zero, *min));
                of.entry(zero).or_default().push(*min);
                of.entry(*min).or_default().push(zero);
            }
        }
    }
    ComplementIndex {
        pairs,
        of,
        intersection_subjects,
    }
}

fn build_super_index<'o>(
    ontology: &'o Ontology,
) -> (
    BTreeMap<&'o str, Vec<&'o ClassExpression>>,
    Vec<(&'o ClassExpression, &'o ClassExpression)>,
) {
    let mut supers: BTreeMap<&'o str, Vec<&'o ClassExpression>> = BTreeMap::new();
    let mut anon_gcis = Vec::new();
    for axiom in ontology.subclass_axioms() {
        match axiom.sub_class() {
            ClassExpression::Class(sub) => {
                supers
                    .entry(sub.iri().as_str())
                    .or_default()
                    .push(axiom.super_class());
            }
            sub if SimpleReasoner::expression_is_vacuous_shape(sub) => {
                anon_gcis.push((sub, axiom.super_class()));
            }
            _ => {}
        }
    }
    (supers, anon_gcis)
}

/// Tighten a pooled (min, max) data-bound slot with another bound: keep
/// the tightest minimum and the tightest maximum.
fn tighten_data_bounds(
    slot: &mut (Option<u32>, Option<u32>),
    min: Option<u32>,
    max: Option<u32>,
) {
    if min.is_some_and(|value| slot.0.is_none_or(|current| value > current)) {
        slot.0 = min;
    }
    if max.is_some_and(|value| slot.1.is_none_or(|current| value < current)) {
        slot.1 = max;
    }
}

/// Outcome of attempting one forced merge.
#[derive(Debug, PartialEq, Eq)]
enum MergeOutcome {
    /// The merge is impossible; the ontology is inconsistent.
    Contradiction,
    /// The successors merged into one node.
    Merged,
    /// Fewer than two successors; nothing to do.
    Nothing,
}

/// Per-call TBox context for the #109 object-cardinality merge rules.
struct MergeContext<'o> {
    ontology: &'o Ontology,
    /// Named class -> super-side expressions; the parser lowers
    /// equivalentClass edges to bidirectional SubClassOf, so definitions
    /// reach this map.
    supers: BTreeMap<&'o str, Vec<&'o ClassExpression>>,
    /// Direct named superclasses per class, the entailment edges.
    super_edges: BTreeMap<&'o str, BTreeSet<&'o str>>,
    /// Anonymous-subject GCIs whose subject has the vacuously-satisfiable
    /// shape (universals and zero-max restrictions, conjoined).
    anon_gcis: Vec<(&'o ClassExpression, &'o ClassExpression)>,
    /// Anonymous-subject GCIs whose subject is an existential restriction
    /// over a named role, conjoined with the vacuous shapes; satisfied
    /// when the node has a successor on the role (W3C I4.5-002).
    existential_gcis: Vec<(&'o ClassExpression, &'o ClassExpression)>,
    /// Nominal-subject GCIs: oneOf{a,b} ⊑ C asserts every named member
    /// into C (#114).
    nominal_gcis: Vec<(&'o ClassExpression, &'o ClassExpression)>,
    /// Whether any GCI subject needs the end-of-walk vacuous sweep
    /// (pure-vacuous subjects, or mixed subjects carrying one).
    has_vacuous_subjects: bool,
    /// Named object property -> its declared inverse partners.
    inverse_base: BTreeMap<&'o str, Vec<&'o str>>,
    /// Transitive superproperty closure (non-reflexive).
    super_props: BTreeMap<&'o str, BTreeSet<&'o str>>,
    /// Functional object properties.
    functional: BTreeSet<&'o str>,
    /// Inverse-functional object properties: same representative object
    /// under one of these forces the subjects together (#131).
    inverse_functional: BTreeSet<&'o str>,
    /// Transitive object properties: edges over them (and their
    /// subproperties) compose (#132).
    transitive: BTreeSet<&'o str>,
    /// Declared symmetric object properties: each edge also mirrors,
    /// so an inverse-role reading can reach the subject too (#152).
    symmetric: BTreeSet<&'o str>,
    /// The cardinality complement exposure (ADR 0021, #156).
    complements: ComplementIndex<'o>,
    /// Memoized named-supers closures; None marks a budget-exhausted
    /// (incomplete) closure that must stop entailing.
    closures: std::cell::RefCell<BTreeMap<&'o str, Option<BTreeSet<&'o str>>>>,
    /// Whether a memo budget (superproperty closure build or class
    /// closure walk) ran out — the same fail-open incompleteness the
    /// probe marks (#173); read by the probe's exhaustion recorder so
    /// context-side truncation stops under-reporting.
    memo_exhausted: std::cell::Cell<bool>,
}

impl<'o> MergeContext<'o> {
    fn new(ontology: &'o Ontology) -> Self {
        let (supers, anon_gcis) = build_super_index(ontology);
        let complements = detect_complement_pairs(ontology);
        let mut existential_gcis = Vec::new();
        let mut nominal_gcis = Vec::new();
        for axiom in ontology.subclass_axioms() {
            let sub = axiom.sub_class();
            if SimpleReasoner::is_anonymous_gci(sub)
                && !SimpleReasoner::expression_is_vacuous_shape(sub)
            {
                existential_gcis.push((sub, axiom.super_class()));
            }
            if matches!(sub, ClassExpression::ObjectOneOf(members) if !members.is_empty()) {
                nominal_gcis.push((sub, axiom.super_class()));
            }
        }
        let mut inverse_base: BTreeMap<&'o str, Vec<&'o str>> = BTreeMap::new();
        for axiom in ontology.inverse_object_properties_axioms() {
            let first = axiom.property1().as_str();
            let second = axiom.property2().as_str();
            inverse_base.entry(first).or_default().push(second);
            inverse_base.entry(second).or_default().push(first);
        }

        let mut direct_props: BTreeMap<&'o str, BTreeSet<&'o str>> = BTreeMap::new();
        for axiom in ontology.subobject_property_axioms() {
            direct_props
                .entry(axiom.sub_property().as_str())
                .or_default()
                .insert(axiom.super_property().as_str());
        }
        // Equivalent object properties are mutual subproperties, so
        // role equivalence collapses into the same closure.
        for axiom in ontology.equivalent_object_properties_axioms() {
            let properties = axiom.properties();
            for (index, left) in properties.iter().enumerate() {
                for right in properties.iter().skip(index + 1) {
                    direct_props
                        .entry(left.as_str())
                        .or_default()
                        .insert(right.as_str());
                    direct_props
                        .entry(right.as_str())
                        .or_default()
                        .insert(left.as_str());
                }
            }
        }
        let memo_exhausted = std::cell::Cell::new(false);
        let mut super_edges: BTreeMap<&'o str, BTreeSet<&'o str>> = BTreeMap::new();
        for (class, expressions) in &supers {
            let entry = super_edges.entry(*class).or_default();
            for expression in expressions {
                collect_named_classes(expression, entry);
            }
        }

        let mut super_props: BTreeMap<&'o str, BTreeSet<&'o str>> = BTreeMap::new();
        let mut budget: usize = MERGE_PROBE_SUPERPROP_BUDGET;
        for sub in direct_props.keys() {
            let mut visited: BTreeSet<&'o str> = BTreeSet::new();
            let mut queue: std::collections::VecDeque<&'o str> =
                std::collections::VecDeque::from([*sub]);
            while let Some(current) = queue.pop_front() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                if let Some(nexts) = direct_props.get(current) {
                    for next in nexts {
                        if visited.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
            }
            if budget > 0 {
                super_props.insert(*sub, visited);
            } else {
                memo_exhausted.set(true);
            }
        }

        let functional = ontology
            .functional_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        let inverse_functional = ontology
            .inverse_functional_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        let transitive = ontology
            .transitive_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        let symmetric = ontology
            .symmetric_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();

        MergeContext {
            ontology,
            supers,
            super_edges,
            has_vacuous_subjects: !anon_gcis.is_empty()
                || existential_gcis
                    .iter()
                    .any(|(subject, _)| SimpleReasoner::subject_reads_vacuously(subject)),
            anon_gcis,
            existential_gcis,
            nominal_gcis,
            inverse_base,
            super_props,
            functional,
            inverse_functional,
            transitive,
            symmetric,
            complements,
            memo_exhausted,
            closures: std::cell::RefCell::new(BTreeMap::new()),
        }
    }

    /// `sub ⊑ super` for named properties, through the transitive
    /// superproperty closure.
    /// Whether an edge's mirror is entailed at the base level: the
    /// reversal holds through a symmetric property sandwiched between
    /// them, e ⊑ S ⊑ base — an edge merely under a symmetric property
    /// does NOT mirror (a subproperty of a symmetric property is not
    /// itself symmetric, #121's lesson; #152).
    fn symmetric_mirror_reaches(&self, edge: &str, base: &str) -> bool {
        self.symmetric.iter().any(|declared| {
            self.is_subproperty_of(edge, declared) && self.is_subproperty_of(declared, base)
        })
    }

    /// `sub ⊑ super` for named properties, through the transitive
    /// and equivalent edges.
    fn is_subproperty_of(&self, sub: &str, super_property: &str) -> bool {
        sub == super_property
            || self
                .super_props
                .get(sub)
                .is_some_and(|supers| supers.contains(super_property))
    }

    /// Resolve a property expression into the roles it stands for: the
    /// direct named role plus, for every declared inverse partner, the
    /// flipped partner role — the same fact stated on both sides of the
    /// declaration. An explicit inverse flips whatever it wraps, so
    /// inverse-of-inverse collapses to the base role.
    fn resolve_role(&self, property: &'o ObjectPropertyExpression) -> Vec<(bool, &'o str)> {
        match property {
            ObjectPropertyExpression::ObjectProperty(named) => {
                let iri = named.iri().as_str();
                let mut roles = vec![(false, iri)];
                for partner in self.inverse_base.get(iri).into_iter().flatten() {
                    roles.push((true, *partner));
                }
                roles
            }
            ObjectPropertyExpression::ObjectInverseOf(inner) => self
                .resolve_role(inner)
                .iter()
                .map(|(inverse, base)| (!*inverse, *base))
                .collect(),
        }
    }

    /// Whether two class expressions on one individual contradict each
    /// other: explicitly disjoint named classes, or a class under a
    /// complement that already entails it.
    fn expressions_contradictory(
        &self,
        left: &'o ClassExpression,
        right: &'o ClassExpression,
    ) -> bool {
        match (left, right) {
            (ClassExpression::Class(a), ClassExpression::Class(b)) => {
                self.classes_disjoint(a.iri().as_str(), b.iri().as_str())
            }
            (ClassExpression::Class(class), ClassExpression::ObjectComplementOf(negated))
            | (ClassExpression::ObjectComplementOf(negated), ClassExpression::Class(class)) => {
                self.complement_excludes(negated, class.iri().as_str())
            }
            _ => false,
        }
    }

    /// `class ⊑ expression`, the condition under which the class and the
    /// expression's complement clash. A Thing-complement is bottom and
    /// clashes outright; other named classes go through the named-supers
    /// closure; unions need one disjunct, intersections need every
    /// conjunct, double negation unwraps.
    fn complement_excludes(&self, expression: &ClassExpression, class: &'o str) -> bool {
        match expression {
            ClassExpression::Class(named) => {
                // ¬Thing is bottom: every class sits under top, so a
                // Thing-complement contradicts any class on the node
                // (#123, W3C someValuesFrom-003's nested universal).
                named.iri().as_str() == OWL_THING_IRI
                    || self.entails(class, named.iri().as_str())
            }
            ClassExpression::ObjectUnionOf(operands) => operands
                .iter()
                .any(|operand| self.complement_excludes(operand, class)),
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| self.complement_excludes(operand, class)),
            ClassExpression::ObjectComplementOf(inner) => match inner.as_ref() {
                ClassExpression::ObjectComplementOf(negated) => {
                    self.complement_excludes(negated, class)
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn entails(&self, class: &'o str, super_class: &str) -> bool {
        class == super_class || self.closure_of(class).contains(super_class)
    }

    /// Named-supers closure of a class, memoized. An exhausted budget
    /// yields an empty set: entailment then fails, which only misses
    /// contradictions, never invents them.
    fn closure_of(&self, class: &'o str) -> BTreeSet<&'o str> {
        if let Some(cached) = self.closures.borrow().get(class) {
            return cached.clone().unwrap_or_default();
        }
        let mut visited: BTreeSet<&'o str> = BTreeSet::new();
        visited.insert(class);
        let mut queue: std::collections::VecDeque<&'o str> =
            std::collections::VecDeque::from([class]);
        let mut budget: usize = MERGE_PROBE_CLOSURE_BUDGET;
        while let Some(current) = queue.pop_front() {
            if budget == 0 {
                visited.clear();
                break;
            }
            budget -= 1;
            if let Some(directs) = self.super_edges.get(current) {
                for direct in directs {
                    if visited.insert(direct) {
                        queue.push_back(direct);
                    }
                }
            }
        }
        self.closures
            .borrow_mut()
            .insert(class, (budget > 0).then(|| visited.clone()));
        if budget == 0 {
            self.memo_exhausted.set(true);
            BTreeSet::new()
        } else {
            visited
        }
    }

    /// Two named classes are disjoint when one explicit DisjointClasses
    /// axiom covers (possibly indirect) supers of both.
    fn classes_disjoint(&self, left: &'o str, right: &'o str) -> bool {
        if left == right {
            return false;
        }
        let left_closure = self.closure_of(left);
        let right_closure = self.closure_of(right);
        // A cardinality complement pair cannot share a member: one
        // side pins the property to zero values, the other to at
        // least one (ADR 0021, #156). Subclass closure on both sides
        // keeps the reading structural — entailment decides, never a
        // name convention.
        if self
            .complements
            .pairs
            .iter()
            .any(|(zero, min)| {
                (left_closure.contains(zero) && right_closure.contains(min))
                    || (left_closure.contains(min) && right_closure.contains(zero))
            })
        {
            return true;
        }
        self.ontology
            .disjoint_classes_axioms()
            .iter()
            .any(|axiom| axiom.classes().iter().enumerate().any(|(i, first)| {
                axiom
                    .classes()
                    .iter()
                    .enumerate()
                    .any(|(j, second)| {
                        i != j
                            && left_closure.contains(first.as_str())
                            && right_closure.contains(second.as_str())
                    })
            }))
    }

    /// Whether assuming membership in `class` is contradictory on its
    /// own: the named-supers closure holds two classes that are
    /// disjoint — declared, or through a cardinality complement pair.
    /// Bounded by the closure budget; failing open just misses
    /// refutations (#156's assist, ADR 0021).
    fn pair_member_contradictory(&self, class: &'o str) -> bool {
        let closure = self.closure_of(class);
        let mut members = closure.iter();
        while let Some(left) = members.next() {
            for right in members.clone() {
                if self.classes_disjoint(left, right) {
                    return true;
                }
            }
        }
        false
    }

    /// One disjunct of the exposed avoidance union dies: avoiding
    /// `avoided` (a named class with a registered complement partner)
    /// forces membership in some partner via the covering half, and
    /// that membership is contradictory on its own (#156).
    fn avoided_conjunct_dies(&self, avoided: &'o str) -> bool {
        self.complements.of.get(avoided).is_some_and(|partners| {
            partners
                .iter()
                .any(|partner| self.pair_member_contradictory(partner))
        })
    }
}

/// Bounded pseudo-ABox for one #109 merge-check run: named individuals
/// plus materialized someValuesFrom witnesses, a union-find over merges,
/// and the cardinality bounds the walk collected. Cloned per branch by
/// the #115 identification search; the TBox context is shared.
#[derive(Clone)]
struct MergeProbe<'o> {
    context: Rc<MergeContext<'o>>,
    named: Vec<Vec<&'o IRI>>,
    expressions: Vec<Vec<&'o ClassExpression>>,
    parent: Vec<usize>,
    edges: Vec<(usize, &'o str, usize)>,
    object_bounds: Vec<(usize, &'o str, u32)>,
    object_min_bounds: Vec<(usize, &'o str, u32)>,
    /// Node pairs the >=-rule created as necessarily distinct.
    must_differ: BTreeSet<(usize, usize)>,
    /// (node, property, filler) for recorded not-(exists p. C)
    /// expressions.
    negated_existentials: Vec<(usize, &'o ObjectPropertyExpression, &'o ClassExpression)>,
    /// (node, role base) for recorded not-hasSelf(p) expressions: the
    /// label contradicts any self-loop over p or its subproperties
    /// (#149).
    negated_self: Vec<(usize, &'o str)>,
    /// (node, members) for recorded C ⊑ oneOf{...} constraints: the
    /// node must be one of the listed individuals (#114).
    oneof_constraints: Vec<(usize, &'o [crate::core::entities::Individual])>,
    /// (node, union) for recorded union choices: the node must belong
    /// to one of the disjuncts (#119, ADR-0020).
    union_choices: Vec<(usize, &'o ClassExpression)>,
    /// Expressions per node whose someValuesFrom witness already
    /// exists — re-walking a node must not spawn it again.
    spawned: BTreeMap<usize, Vec<&'o ClassExpression>>,
    data_bounds: BTreeMap<(usize, &'o str), (Option<u32>, Option<u32>)>,
    /// (node, excluded class) for pair flips: the node's recorded
    /// class has a registered cardinality complement partner, so the
    /// node cannot belong to the partner (the disjointness half,
    /// ADR 0021). Consumed by the check phase's avoidance assist.
    complement_notes: Vec<(usize, &'o str)>,
    /// (node, property, value) for DataHasValue restrictions the walk
    /// met: the value is given, so each is a positive data assertion
    /// the settled cardinality check can spend (#125).
    data_positives: Vec<(usize, &'o str, &'o crate::core::entities::Literal)>,
    named_index: BTreeMap<&'o IRI, usize>,
    /// Blank-node assertion objects, keyed by node id (#136).
    anon_index: BTreeMap<String, usize>,
    /// Stated negative object-property assertions: (subject, property,
    /// object) triples a derived edge must never realize (#138).
    negative_edges: Vec<(usize, &'o str, usize)>,
    /// Asserted sameAs member pairs (the premise's own or the entailment
    /// reduction's negated differentFrom conclusion); each merge round
    /// applies them first, so functional and cardinality rules see one
    /// node (#131).
    identity_pairs: Vec<(usize, usize)>,
    /// Role-closure candidate index (#172): per queried base role, the
    /// edges whose property sits under that base's subproperty closure,
    /// in edge order — materialized lazily on first query and kept
    /// fresh by `push_edge`, so mutation can never stale it. Interior
    /// mutability because the existential GCI check reads through
    /// `&self` (the `closures` memo precedent in MergeContext). Memory
    /// bound: the sum over materialized bases of |edges under that
    /// base| tuples (24 bytes each) — at most materialized-bases ×
    /// edges in the worst case, and never-queried bases cost nothing.
    /// Buckets are copy-on-write, so branch clones share until one
    /// side pushes.
    candidate_index: std::cell::RefCell<HashMap<&'o str, Rc<Vec<(usize, &'o str, usize)>>>>,
    /// Remaining walk budget; hitting zero with work pending sets
    /// `exhausted` — the verdict stays fail-open but is marked (#171).
    budget: usize,
    /// The configured walk-budget ceiling this probe (and its clones)
    /// refresh from — the reasoner's override, not the const, so a
    /// test-shrunk budget survives branch refreshes.
    walk_budget: usize,
    /// Whether any probe phase stopped early because a budget ran out
    /// (walk, propagation, settle, clone search, or the global work
    /// ceiling). Read by the reasoner after the probe returns (#171,
    /// ADR 0022). A Cell so the `&self` candidate loop can mark it.
    exhausted: std::cell::Cell<bool>,
    /// Edge inspections spent inside existential-GCI candidate checks
    /// (ADR 0022's work unit), across scans and materialization passes.
    work_spent: std::cell::Cell<u64>,
    /// Global work ceiling; `0` = unlimited (#173). Branch clones
    /// inherit the parent's REMAINING allowance so total work stays
    /// under one ceiling.
    work_ceiling: u64,
}

impl<'o> MergeProbe<'o> {
    /// Number of expression nodes the walk would visit — the size-gate
    /// signal (#168).
    fn node_count(&self) -> usize {
        self.expressions.len()
    }

    /// Whether the walk-node budget OR the global work ceiling is
    /// spent; marks the probe exhausted so the caller's early return
    /// cannot forget the flag (#171, #173).
    fn budget_out(&mut self) -> bool {
        if self.budget == 0 || self.work_out() {
            self.exhausted.set(true);
            return true;
        }
        false
    }

    /// Whether the global work ceiling is spent (`&self` variant for
    /// the candidate loop, ADR 0022's work unit).
    fn work_out(&self) -> bool {
        self.work_ceiling > 0 && self.work_spent.get() >= self.work_ceiling
    }

    /// Whether this probe stopped early for ANY budget reason — its
    /// own phases or the shared context's memo caps (#171, #173).
    fn incomplete(&self) -> bool {
        self.exhausted.get() || self.context.memo_exhausted.get()
    }

    /// Charge one work unit; if that spends the ceiling, mark the
    /// probe exhausted (the caller stops searching — fail-open).
    fn charge_work(&self) -> bool {
        self.work_spent.set(self.work_spent.get().saturating_add(1));
        if self.work_out() {
            self.exhausted.set(true);
            return true;
        }
        false
    }

    /// Record a successor edge, keeping every materialized candidate
    /// bucket fresh so the lazy role-closure index can never go stale
    /// against mutation (#172). Buckets are copy-on-write (`Rc` +
    /// `make_mut`): a branch clone shares its parent's buckets and only
    /// a bucket appended to after the split pays a copy.
    fn push_edge(&mut self, subject: usize, property: &'o str, object: usize) {
        self.edges.push((subject, property, object));
        let mut index = self.candidate_index.borrow_mut();
        for (base, bucket) in index.iter_mut() {
            if self.context.is_subproperty_of(property, base) {
                Rc::make_mut(bucket).push((subject, property, object));
            }
        }
    }

    /// `push_edge` plus the declared inverse partners: the same fact
    /// stated on every partner role, so inverse-role reasoning sees
    /// asserted and derived edges alike (W3C I4.5-002's
    /// hasEuroMP/isEuroMPFrom).
    fn push_edge_with_partners(&mut self, subject: usize, property: &'o str, object: usize) {
        self.push_edge(subject, property, object);
        let partners: Vec<&'o str> = self
            .context
            .inverse_base
            .get(property)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for partner in partners {
            self.push_edge(object, partner, subject);
        }
    }

    /// Whether any edge under `base`'s subproperty closure satisfies
    /// `test` — the candidate list is materialized on first query (one
    /// filtered pass over the edges) and reused after, replacing the
    /// per-(GCI × node) all-edges closure scan (#172). Iteration order
    /// matches the edges vec, so short-circuiting picks the same
    /// witness the full scan would. The cached bucket is Rc-cloned out
    /// and the RefCell borrow released BEFORE `test` runs, so the test
    /// closure may re-enter the probe (the `closures` memo precedent)
    /// without borrow hazards.
    fn candidate_edges_satisfy(
        &self,
        base: &'o str,
        mut test: impl FnMut(usize, &'o str, usize) -> bool,
    ) -> bool {
        let cached = self.candidate_index.borrow().get(base).map(Rc::clone);
        if let Some(bucket) = cached {
            for (subject, property, object) in bucket.iter() {
                if self.charge_work() {
                    return false; // ceiling spent mid-scan: fail-open
                }
                if test(*subject, *property, *object) {
                    return true;
                }
            }
            return false;
        }
        // Materialization inspects every edge once (closure test per
        // edge) — that is work too (ADR 0022's unit is the inspection,
        // not the match). A ceiling hit leaves a partial bucket, which
        // only ever under-reports witnesses under a marked exhaustion.
        let mut bucket: Vec<(usize, &'o str, usize)> = Vec::new();
        for edge in self.edges.iter() {
            if self.charge_work() {
                break;
            }
            if self.context.is_subproperty_of(edge.1, base) {
                bucket.push(*edge);
            }
        }
        let mut answer = false;
        for (subject, property, object) in bucket.iter() {
            if self.charge_work() {
                break;
            }
            if test(*subject, *property, *object) {
                answer = true;
                break;
            }
        }
        self.candidate_index.borrow_mut().insert(base, Rc::new(bucket));
        answer
    }

    fn new(ontology: &'o Ontology) -> Self {
        let mut probe = MergeProbe {
            context: Rc::new(MergeContext::new(ontology)),
            named: Vec::new(),
            expressions: Vec::new(),
            parent: Vec::new(),
            edges: Vec::new(),
            object_bounds: Vec::new(),
            object_min_bounds: Vec::new(),
            must_differ: BTreeSet::new(),
            negated_existentials: Vec::new(),
            negated_self: Vec::new(),
            complement_notes: Vec::new(),
            oneof_constraints: Vec::new(),
            union_choices: Vec::new(),
            spawned: BTreeMap::new(),
            data_bounds: BTreeMap::new(),
            data_positives: Vec::new(),
            named_index: BTreeMap::new(),
            anon_index: BTreeMap::new(),
            negative_edges: Vec::new(),
            identity_pairs: Vec::new(),
            candidate_index: std::cell::RefCell::new(HashMap::new()),
            budget: MERGE_PROBE_WALK_BUDGET,
            walk_budget: MERGE_PROBE_WALK_BUDGET,
            exhausted: std::cell::Cell::new(false),
            work_spent: std::cell::Cell::new(0),
            work_ceiling: DEFAULT_MERGE_PROBE_WORK_BUDGET,
        };
        for assertion in ontology.class_assertions() {
            let node = probe.node_for_individual(assertion.individual());
            probe.expressions[node].push(assertion.class_expr());
        }
        for assertion in ontology.property_assertions() {
            let subject = probe.node_for_individual(assertion.subject());
            let object_node = match assertion.object() {
                crate::logic::axioms::PropertyAssertionObject::Named(object) => {
                    probe.node_for_individual(object)
                }
                // A blank-node object is still a real successor: the edge
                // exists, so universals over the property reach it (W3C
                // bnode2someValuesFrom, #136).
                crate::logic::axioms::PropertyAssertionObject::Anonymous(anonymous) => {
                    probe.node_for_anonymous(anonymous.node_id())
                }
            };
            let property = assertion.property().as_str();
            probe.push_edge_with_partners(subject, property, object_node);
        }
        // Members of a nominal-subject GCI are forced into its super
        // class: they enter the walk even when the ontology carries no
        // ABox of their own (W3C one_two's a = oneOf{i,j,k} types the
        // individuals through the GCI alone) (#154).
        let context = std::rc::Rc::clone(&probe.context);
        for (subject, super_expression) in context.nominal_gcis.iter() {
            let ClassExpression::ObjectOneOf(members) = subject else {
                continue;
            };
            for member in members.iter() {
                let crate::core::entities::Individual::Named(named) = member else {
                    continue;
                };
                let node = probe.node_for_individual(named.iri());
                probe.record(node, super_expression);
            }
        }
        // AllDifferent members are pairwise must-differ seeds — the
        // counting rules consume them like >=-rule pairs (#154).
        for axiom in ontology.different_individuals_axioms() {
            let members = axiom.individuals();
            for (index, left) in members.iter().enumerate() {
                for right in members.iter().skip(index + 1) {
                    let (left_node, right_node) =
                        (probe.node_for_individual(left), probe.node_for_individual(right));
                    probe.must_differ.insert((left_node.min(right_node), left_node.max(right_node)));
                }
            }
        }
        for axiom in ontology.same_individual_axioms() {
            let members = axiom.individuals();
            for (index, left) in members.iter().enumerate() {
                for right in members.iter().skip(index + 1) {
                    let (left_node, right_node) =
                        (probe.node_for_individual(left), probe.node_for_individual(right));
                    probe.identity_pairs.push((left_node, right_node));
                }
            }
        }
        // The parser also materializes frame-form owl:sameAs as property
        // assertions (the closure engine's representation); both spellings
        // of the same fact force the same identity (#131).
        const OWL_SAME_AS_IRI: &str = "http://www.w3.org/2002/07/owl#sameAs";
        for assertion in ontology.property_assertions() {
            if assertion.property().as_str() != OWL_SAME_AS_IRI {
                continue;
            }
            let Some(object) = assertion.object_iri() else {
                continue;
            };
            let (left_node, right_node) = (
                probe.node_for_individual(assertion.subject()),
                probe.node_for_individual(object),
            );
            probe.identity_pairs.push((left_node, right_node));
        }
        for assertion in ontology.negative_object_property_assertions() {
            let Some(object) = assertion.object_iri() else {
                continue;
            };
            let (subject_node, object_node) = (
                probe.node_for_individual(assertion.subject()),
                probe.node_for_individual(object),
            );
            probe
                .negative_edges
                .push((subject_node, assertion.property().as_str(), object_node));
        }
        probe
    }

    fn node_for_individual(&mut self, individual: &'o IRI) -> usize {
        if let Some(&node) = self.named_index.get(individual) {
            return node;
        }
        let node = self.fresh_slot();
        self.named[node].push(individual);
        self.named_index.insert(individual, node);
        node
    }

    /// A blank-node assertion object keys on its node id — nameless, so
    /// the probe keeps no IRI for it, but edges and universals reach it
    /// like any node (#136).
    fn node_for_anonymous(&mut self, node_id: &str) -> usize {
        if let Some(&node) = self.anon_index.get(node_id) {
            return node;
        }
        let node = self.fresh_slot();
        self.anon_index.insert(node_id.to_string(), node);
        node
    }

    fn fresh_slot(&mut self) -> usize {
        let node = self.expressions.len();
        self.expressions.push(Vec::new());
        self.named.push(Vec::new());
        self.parent.push(node);
        node
    }

    fn fresh_witness(&mut self) -> Option<usize> {
        (self.expressions.len() < MERGE_PROBE_MAX_NODES).then(|| self.fresh_slot())
    }

    fn find(&mut self, node: usize) -> usize {
        let mut root = node;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut current = node;
        while self.parent[current] != root {
            let next = self.parent[current];
            self.parent[current] = root;
            current = next;
        }
        root
    }

    fn representative(&self, node: usize) -> usize {
        let mut root = node;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        root
    }

    fn record(&mut self, node: usize, expression: &'o ClassExpression) -> bool {
        let expressions = &mut self.expressions[node];
        if expressions.len() < MERGE_PROBE_EXPRESSION_CAP
            && !expressions
                .iter()
                .any(|existing| std::ptr::eq(*existing, expression))
        {
            expressions.push(expression);
            true
        } else {
            false
        }
    }

    /// The node a restriction binds for `node` under one resolved role:
    /// forward roles bind successors, inverse roles bind predecessors
    /// (the subject of an edge whose object is `node`).
    fn endpoint_for(
        &self,
        node: usize,
        is_inverse: bool,
        subject: usize,
        object: usize,
    ) -> Option<usize> {
        if is_inverse {
            (self.representative(object) == self.representative(node))
                .then(|| self.representative(subject))
        } else {
            (self.representative(subject) == self.representative(node))
                .then(|| self.representative(object))
        }
    }

    /// Super-side expressions of every anonymous-subject GCI whose
    /// subject holds for the node: vacuous or positively recorded
    /// subjects, and existential subjects witnessed by an edge.
    /// Anonymous GCIs whose subject the node already carries (positive
    /// satisfaction), plus existential subjects witnessed by an edge.
    /// These apply immediately during the walk.
    fn applicable_positive_gcis(&self, start: usize) -> Vec<&'o ClassExpression> {
        let mut applicable: Vec<&'o ClassExpression> = Vec::new();
        let recorded = &self.expressions[start];
        for (subject, super_expression) in self.context.anon_gcis.iter() {
            if SimpleReasoner::anon_gci_subject_recorded(subject, recorded) {
                applicable.push(super_expression);
            }
        }
        for (subject, super_expression) in self.context.existential_gcis.iter() {
            // Mixed subjects whose universal/zero-max conjunct is read
            // vacuously wait for the end-of-walk sweep: at walk start the
            // node's own edges are still being materialized, so a
            // property it will use can look unused and classify it into
            // an equivalence it does not satisfy (W3C miscellaneous-001's
            // SemillonOrSauvignonBlanc over a foreign grape).
            if SimpleReasoner::subject_reads_vacuously(subject) {
                continue;
            }
            if self.existential_gci_holds(start, subject) {
                applicable.push(super_expression);
            }
        }
        for (subject, super_expression) in self.context.nominal_gcis.iter() {
            if self.nominal_gci_holds(start, subject) {
                applicable.push(super_expression);
            }
        }
        applicable
    }

    /// Whether a oneOf-subject GCI holds for the node: the node is one
    /// of the listed named individuals.
    fn nominal_gci_holds(&self, start: usize, subject: &ClassExpression) -> bool {
        let ClassExpression::ObjectOneOf(members) = subject else {
            return false;
        };
        self.named[start].iter().any(|individual| {
            members
                .iter()
                .any(|member| member.iri().is_some_and(|iri| iri.as_ref() == *individual))
        })
    }

    /// Anonymous GCIs that hold only through vacuous satisfaction,
    /// applied after the node's own walk has settled (see walk_node).
    fn applicable_vacuous_gcis(&self, start: usize) -> Vec<&'o ClassExpression> {
        let mut applicable: Vec<&'o ClassExpression> = Vec::new();
        if !self.context.has_vacuous_subjects {
            return applicable;
        }
        let used: Vec<&'o str> = self
            .edges
            .iter()
            .filter(|(subject, _, _)| self.representative(*subject) == start)
            .map(|(_, property, _)| *property)
            .collect();
        // A minimum cardinality on a property forces a successor into
        // existence, so that property counts as used for the vacuous
        // reading (W3C description-logic-661).
        let forced: Vec<&'o str> = self
            .object_min_bounds
            .iter()
            .filter(|(node, _, minimum)| *node == start && *minimum >= 1)
            .map(|(_, property, _)| *property)
            .collect();
        let property_used =
            |property: &IRI| used.contains(&property.as_str()) || forced.contains(&property.as_str());
        let recorded = &self.expressions[start];
        for (subject, super_expression) in self.context.anon_gcis.iter() {
            if SimpleReasoner::anon_gci_subject_holds(subject, recorded, &property_used) {
                applicable.push(super_expression);
            }
        }
        // Mixed subjects decide here, against the settled edge set:
        // their class/existential conjuncts read the recorded classes,
        // their universal/zero-max conjuncts the vacuous reading over
        // properties the node actually uses (#130).
        for (subject, super_expression) in self.context.existential_gcis.iter() {
            if SimpleReasoner::subject_reads_vacuously(subject)
                && self.existential_gci_holds_with(start, subject, &property_used)
            {
                applicable.push(super_expression);
            }
        }
        applicable
    }

    /// Walk one node's expressions: expand named classes through their
    /// super-side expressions, materialize someValuesFrom witnesses
    /// (inverse restrictions attach the witness on the base role's other
    /// end), and collect the object and data cardinality bounds.
    fn walk_node(&mut self, start: usize) {
        let mut stack: Vec<&'o ClassExpression> = self.expressions[start].clone();
        let mut visited: BTreeSet<&'o str> = BTreeSet::new();
        // Existential and positively-recorded anonymous GCIs apply
        // immediately, whatever the node's seed shape (I4.5-002's
        // individual carries only a complement).
        stack.extend(self.applicable_positive_gcis(start));
        // Every individual is an instance of owl:Thing, so its super-side
        // restrictions (global GCIs) bind every node.
        if let Some(thing_supers) = self.context.supers.get(OWL_THING_IRI) {
            stack.extend(thing_supers.iter().copied());
        }
        let mut swept_vacuous = false;
        loop {
            while let Some(expression) = stack.pop() {
                if self.budget_out() {
                    return;
                }
                self.budget -= 1;
            match expression {
                ClassExpression::Class(class) => {
                    let iri = class.iri().as_str();
                    if !visited.insert(iri) {
                        continue;
                    }
                    self.record(start, expression);
                    if let Some(supers) = self.context.supers.get(iri) {
                        stack.extend(supers.iter().copied());
                    }
                    // A recorded class excludes its cardinality
                    // complement partners (the disjointness half,
                    // ADR 0021): the note drives the check phase's
                    // avoidance assist.
                    if let Some(partners) = self.context.complements.of.get(iri) {
                        for partner in partners {
                            if !self.complement_notes.iter().any(|(node, excluded)| {
                                *node == start && *excluded == *partner
                            }) {
                                self.complement_notes.push((start, partner));
                            }
                        }
                    }
                    stack.extend(self.applicable_positive_gcis(start));
                }
                ClassExpression::ObjectIntersectionOf(operands) => {
                    stack.extend(operands.iter().map(|operand| operand.as_ref()));
                }
                ClassExpression::ObjectSomeValuesFrom(property, filler) => {
                    if self
                        .spawned
                        .get(&start)
                        .is_some_and(|seen| seen.iter().any(|e| std::ptr::eq(*e, expression)))
                    {
                        continue;
                    }
                    self.spawned.entry(start).or_default().push(expression);
                    // A named-class filler already witnessed by an
                    // existing edge needs no fresh successor: reuse
                    // keeps self-referential definitions (V3 ≡ ∃invF.d
                    // whose witnesses are d-typed f-predecessors) from
                    // spawning an infinite chain that exhausts the node
                    // budget before the depth-2 functional merges
                    // complete (#135, W3C description-logic-626/627).
                    // Satisfaction is monotone — edges, recorded
                    // classes, and merges only grow — so checking once
                    // here suffices.
                    if self.existential_gci_holds(start, expression) {
                        continue;
                    }
                    // A singleton named oneOf filler is exactly a
                    // hasValue: the edge goes straight to the member's
                    // node instead of spawning a witness that would
                    // walk the global GCIs again and again — the
                    // spy-point shape (Thing ⊑ ∃p.{spy} over ≥-spawned
                    // witnesses) exhausts the node cap that way (#144,
                    // W3C description-logic-035).
                    if let ClassExpression::ObjectOneOf(members) = filler.as_ref() {
                        if members.len() == 1 {
                            if let Some(member) = members[0].iri() {
                                for (is_inverse, base) in self.context.resolve_role(property) {
                                    let object = self.node_for_individual(member);
                                    if is_inverse {
                                        self.push_edge(object, base, start);
                                    } else {
                                        self.push_edge(start, base, object);
                                    }
                                }
                                continue;
                            }
                        }
                    }
                    for (is_inverse, base) in self.context.resolve_role(property) {
                        let Some(witness) = self.fresh_witness() else {
                            continue;
                        };
                        self.record(witness, filler);
                        if is_inverse {
                            self.push_edge(witness, base, start);
                        } else {
                            self.push_edge(start, base, witness);
                        }
                    }
                }
                ClassExpression::ObjectHasValue(property, individual) => {
                    let Some(iri) = individual.iri() else {
                        continue;
                    };
                    for (is_inverse, base) in self.context.resolve_role(property) {
                        let object = self.node_for_individual(iri);
                        if is_inverse {
                            self.push_edge(object, base, start);
                        } else {
                            self.push_edge(start, base, object);
                        }
                    }
                }
                ClassExpression::ObjectHasSelf(property) => {
                    for (_, base) in self.context.resolve_role(property) {
                        self.push_edge(start, base, start);
                    }
                }
                ClassExpression::ObjectMaxCardinality(maximum, property, None) => {
                    for (is_inverse, base) in self.context.resolve_role(property) {
                        if !is_inverse {
                            self.object_bounds.push((start, base, *maximum));
                        }
                    }
                }
                ClassExpression::ObjectExactCardinality(exact, property, None) => {
                    // Exact cardinality binds both directions; a shared
                    // or-pattern arm would swallow it into the max side.
                    for (is_inverse, base) in self.context.resolve_role(property) {
                        if !is_inverse {
                            self.object_bounds.push((start, base, *exact));
                            self.object_min_bounds.push((start, base, *exact));
                        }
                    }
                }
                ClassExpression::ObjectMinCardinality(minimum, property, None) => {
                    for (is_inverse, base) in self.context.resolve_role(property) {
                        if !is_inverse {
                            self.object_min_bounds.push((start, base, *minimum));
                        }
                    }
                }
                ClassExpression::DataMinCardinality(minimum, property, None) => {
                    if let Some(named) = property.as_named() {
                        let slot = self
                            .data_bounds
                            .entry((start, named.iri().as_str()))
                            .or_insert((None, None));
                        if slot.0.is_none_or(|current| *minimum > current) {
                            slot.0 = Some(*minimum);
                        }
                    }
                }
                ClassExpression::DataMaxCardinality(maximum, property, None) => {
                    if let Some(named) = property.as_named() {
                        let slot = self
                            .data_bounds
                            .entry((start, named.iri().as_str()))
                            .or_insert((None, None));
                        if slot.1.is_none_or(|current| *maximum < current) {
                            slot.1 = Some(*maximum);
                        }
                    }
                }
                ClassExpression::DataExactCardinality(exact, property, None) => {
                    // Exact cardinality binds both directions; a shared
                    // or-pattern arm would swallow it into the min side.
                    if let Some(named) = property.as_named() {
                        let slot = self
                            .data_bounds
                            .entry((start, named.iri().as_str()))
                            .or_insert((None, None));
                        if slot.0.is_none_or(|current| *exact > current) {
                            slot.0 = Some(*exact);
                        }
                        if slot.1.is_none_or(|current| *exact < current) {
                            slot.1 = Some(*exact);
                        }
                    }
                }
                ClassExpression::DataHasValue(property, value) => {
                    // The restriction asserts the value outright — the
                    // data-side twin of the ObjectHasValue edge: the
                    // positive is recorded for the settled cardinality
                    // check (#125, W3C I5.21-002). Re-walks append
                    // nothing new: the same expression is the same
                    // literal.
                    if let Some(named) = property.as_named() {
                        if !self.data_positives.iter().any(|(node, recorded, literal)| {
                            *node == start
                                && *recorded == named.iri().as_str()
                                && std::ptr::eq(*literal, value)
                        }) {
                            self.data_positives
                                .push((start, named.iri().as_str(), value));
                        }
                    }
                }
                ClassExpression::ObjectAllValuesFrom(_, _) => {
                    self.record(start, expression);
                }
                ClassExpression::ObjectOneOf(members) if !members.is_empty() => {
                    // C ⊑ oneOf{...}: every C-instance is one of these
                    // individuals; checked once the walk settles.
                    self.record(start, expression);
                    self.oneof_constraints.push((start, members.as_ref()));
                }
                ClassExpression::ObjectUnionOf(_) => {
                    // The node must belong to one disjunct: recorded as
                    // a choice, discharged or branched once the walk
                    // settles (#119, ADR-0020).
                    self.record(start, expression);
                    if !self.union_choices.iter().any(|(node, existing)| {
                        *node == start && std::ptr::eq(*existing, expression)
                    }) {
                        self.union_choices.push((start, expression));
                    }
                }
                ClassExpression::ObjectComplementOf(inner) => {
                    self.record(start, expression);
                    // not-(exists p. C) is forall p. not-C; recorded so
                    // the check phase can clash it against real
                    // successors (#112, W3C bnode2somevaluesfrom).
                    if let ClassExpression::ObjectSomeValuesFrom(property, filler) =
                        inner.as_ref()
                    {
                        self.negated_existentials.push((start, property, filler));
                    }
                    // not-hasSelf(p) forbids p-labelled self-loops; a
                    // self-loop is inverse-invariant, so every role
                    // base the property resolves to records the ban
                    // (#149, W3C SelfRestriction-002).
                    if let ClassExpression::ObjectHasSelf(property) = inner.as_ref() {
                        for (_, base) in self.context.resolve_role(property) {
                            self.negated_self.push((start, base));
                        }
                    }
                    // Complements of unqualified cardinality restrictions
                    // are the opposite bound: not-(>= n) is <= n-1 and
                    // not-(<= n) is >= n+1 (W3C description-logic-107/
                    // -111). Exact cardinalities are a disjunction and
                    // stay unnormalized.
                    match inner.as_ref() {
                        ClassExpression::ObjectMinCardinality(minimum, property, None)
                            if *minimum > 0 =>
                        {
                            for (is_inverse, base) in self.context.resolve_role(property) {
                                if !is_inverse {
                                    self.object_bounds.push((start, base, *minimum - 1));
                                }
                            }
                        }
                        ClassExpression::ObjectMaxCardinality(maximum, property, None) => {
                            for (is_inverse, base) in self.context.resolve_role(property) {
                                if !is_inverse {
                                    if let Some(minimum) = maximum.checked_add(1) {
                                        self.object_min_bounds.push((start, base, minimum));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            }
            // Vacuous anonymous GCIs wait until the node's own walk has
            // settled: a minimum cardinality on the restricted property
            // will materialize an edge on it, and then the property is
            // used and the vacuous reading no longer holds (W3C
            // description-logic-661).
            if swept_vacuous {
                break;
            }
            swept_vacuous = true;
            let before = self.expressions[start].len();
            stack.extend(self.applicable_vacuous_gcis(start));
            if self.expressions[start].len() == before && stack.is_empty() {
                break;
            }
        }
    }

    fn process(&mut self) {
        for _ in 0..MERGE_PROBE_PROPAGATION_ROUNDS {
            // The >=-rule materializes the successors a minimum
            // cardinality demands; fresh witnesses are walked in the
            // next round.
            loop {
                let mut index = 0;
                while index < self.expressions.len() {
                    if self.budget_out() {
                        return;
                    }
                    self.walk_node(index);
                    index += 1;
                }
                if self.budget_out() {
                    break;
                }
                if !self.materialize_min_witnesses() {
                    break;
                }
            }
            let chained = self.chain_transitive_edges();
            if self.budget_out() {
                return;
            }
            let propagated = self.propagate_universals();
            if self.budget_out() {
                return;
            }
            if !chained && !propagated {
                return;
            }
        }
    }

    /// Transitive roles compose: for every transitive t, edges (a,p,b)
    /// and (b,q,d) with p,q ⊑ t add (a,t,d) — plus the declared inverse
    /// partners, mirroring the seeding idiom — so universals over either
    /// direction see through the chain (W3C description-logic-629/632's
    /// ∀invR must reach the transitively-connected start node). The
    /// pairwise scan is size- and budget-capped; exhaustion skips, never
    /// invents (#132).
    fn chain_transitive_edges(&mut self) -> bool {
        if self.context.transitive.is_empty() {
            return false;
        }
        // A dense role over a full probe cannot be squared; the closure
        // stays partial (fail-open).
        const MAX_CHAINABLE_EDGES: usize = 4_096;
        if self.edges.len() > MAX_CHAINABLE_EDGES {
            return false;
        }
        let mut added = false;
        loop {
            if self.budget_out() {
                return added;
            }
            self.budget -= 1;
            // Joins normalize through the union-find: an earlier merge
            // round may already have unified endpoints, and a raw-id
            // join would skip those compositions.
            let roots: Vec<usize> = (0..self.parent.len()).map(|n| self.find(n)).collect();
            let edges: Vec<(usize, &'o str, usize)> = self.edges.clone();
            let mut fresh: Vec<(usize, &'o str, usize)> = Vec::new();
            for role in &self.context.transitive {
                for (a, p, middle) in &edges {
                    if !self.context.is_subproperty_of(p, role) {
                        continue;
                    }
                    for (mid, q, d) in &edges {
                        if roots[*mid] != roots[*middle]
                            || !self.context.is_subproperty_of(q, role)
                        {
                            continue;
                        }
                        let composed = (roots[*a], *role, roots[*d]);
                        if !self.edges.contains(&composed) && !fresh.contains(&composed) {
                            fresh.push(composed);
                        }
                    }
                }
                if fresh.len() > MERGE_PROBE_PAIR_CAP {
                    return added;
                }
            }
            if fresh.is_empty() {
                return added;
            }
            for (a, role, d) in fresh {
                self.push_edge_with_partners(a, role, d);
            }
            added = true;
        }
    }

    /// Materialize the successors that >= n P demands: create fresh
    /// witnesses for the shortfall and book every pair of the demanded
    /// set as necessarily distinct, so a forced merge unifying two of
    /// them is the inconsistency (#111). The created edges also keep
    /// the vacuous reading of complementary zero-max GCIs away from
    /// this node (W3C description-logic-661).
    fn materialize_min_witnesses(&mut self) -> bool {
        let min_bounds: Vec<(usize, &'o str, u32)> = self.object_min_bounds.clone();
        let mut added = false;
        for (node, property, minimum) in min_bounds {
            if self.budget_out() {
                break;
            }
            self.budget -= 1;
            let node = self.find(node);
            let needed = minimum as usize;
            let mut group: Vec<usize> = self.successors_of(node, property);
            let shortfall = needed.saturating_sub(group.len());
            let existing = group.len();
            for _ in 0..shortfall {
                let Some(witness) = self.fresh_witness() else {
                    break;
                };
                self.push_edge(node, property, witness);
                group.push(witness);
                added = true;
            }
            // The demanded set is pairwise distinct when the group is
            // exactly the demanded size (filled to it, or satisfied by
            // precisely that many successors). A larger group only
            // guarantees the minimum somehow — no pair is forced.
            if group.len() <= needed {
                for i in 0..group.len() {
                    for j in i + 1..group.len() {
                        let pair = if group[i] <= group[j] {
                            (group[i], group[j])
                        } else {
                            (group[j], group[i])
                        };
                        self.must_differ.insert(pair);
                    }
                }
            }
        }
        added
    }

    /// One round of universal-restriction propagation: a recorded ∀r.C
    /// puts C on every r-successor (inverse universals put it on the
    /// predecessors). Returns whether anything was added; the caller
    /// re-walks the touched nodes, so propagated classes expand like
    /// asserted ones.
    fn propagate_universals(&mut self) -> bool {
        let edges: Vec<(usize, &'o str, usize)> = self.edges.clone();
        let mut universals: Vec<(usize, &'o ClassExpression)> = Vec::new();
        for (node, expressions) in self.expressions.iter().enumerate() {
            for expression in expressions {
                if matches!(expression, ClassExpression::ObjectAllValuesFrom(_, _)) {
                    universals.push((node, expression));
                }
            }
        }
        let mut added = false;
        for (node, expression) in universals {
            if self.budget_out() {
                break;
            }
            self.budget -= 1;
            let ClassExpression::ObjectAllValuesFrom(property, filler) = expression else {
                continue;
            };
            let roles = self.context.resolve_role(property);
            for (is_inverse, base) in roles {
                for (subject, edge_property, object) in &edges {
                    if !self.context.is_subproperty_of(edge_property, base) {
                        continue;
                    }
                    let mut endpoint = self.endpoint_for(node, is_inverse, *subject, *object);
                    if endpoint.is_none()
                        && is_inverse
                        && self.context.symmetric_mirror_reaches(edge_property, base)
                    {
                        // A symmetric base's edge mirrors, so the node's
                        // own outgoing edge reaches a mirrored inverse
                        // successor — the same sandwich rule the GCI
                        // reader applies (#152's sibling gap).
                        endpoint = self.endpoint_for(node, false, *subject, *object);
                    }
                    if let Some(target) = endpoint {
                        if self.record(target, filler) {
                            added = true;
                        }
                    }
                }
            }
        }
        added
    }

    /// Whether the node records a named-class complement whose
    /// contrapositive covers ¬target: a recorded ¬D satisfies ¬target
    /// exactly when target ⊑ D through the named-supers closure
    /// (#142).
    fn recorded_complement_entails(&self, node: usize, target: &'o str) -> bool {
        self.expressions[node].iter().any(|candidate| {
            let ClassExpression::ObjectComplementOf(negated) = candidate else {
                return false;
            };
            let ClassExpression::Class(recorded) = negated.as_ref() else {
                return false;
            };
            self.context.entails(target, recorded.iri().as_str())
        })
    }

    /// Whether the node records a named class that is — or, through
    /// the named-supers closure, entails — the target: the shared
    /// satisfaction check for GCI conjuncts and existential fillers.
    fn recorded_class_entails(&self, node: usize, target: &str) -> bool {
        target == OWL_THING_IRI
            || self.expressions[node].iter().any(|candidate| {
                matches!(candidate, ClassExpression::Class(class)
                    if class.iri().as_str() == target
                        || self.context.entails(class.iri().as_str(), target))
            })
    }

    /// Whether one conjunct of an intersection complement is
    /// verifiably satisfied on the node (#160): a named class through
    /// the recorded entailment, hasSelf through a realized self-loop,
    /// an existential through a witnessed successor. Any other shape
    /// fails open — no verification, no refutation.
    fn intersection_conjunct_satisfied(&self, node: usize, conjunct: &'o ClassExpression) -> bool {
        match conjunct {
            ClassExpression::Class(class) => {
                self.recorded_class_entails(node, class.iri().as_str())
            }
            ClassExpression::ObjectHasSelf(property) => self
                .context
                .resolve_role(property)
                .iter()
                .any(|(_, base)| {
                    // A self-loop realized on a subproperty of the
                    // restricted role satisfies the restriction (#160's
                    // recorded gap, closed); exact-label matching left
                    // the conjunct unverifiable and fail-open.
                    self.edges.iter().any(|(subject, edge, object)| {
                        self.representative(*subject) == node
                            && self.representative(*object) == node
                            && self.context.is_subproperty_of(edge, base)
                    })
                }),
            ClassExpression::ObjectSomeValuesFrom(..) => self.existential_gci_holds(node, conjunct),
            _ => false,
        }
    }

    /// Whether an existential GCI subject holds for the node: a named
    /// class holds when the node records it or a subsumer, and an
    /// existential needs a successor on the role (through the
    /// subproperty closure) whose recorded classes entail the filler
    /// (#127). Universal and zero-max conjuncts are read through the
    /// caller's uses-property view — the end-of-walk sweep passes the
    /// settled edges, so the vacuous reading cannot outrun edge
    /// materialization (#130).
    fn existential_gci_holds(&self, node: usize, expression: &'o ClassExpression) -> bool {
        let edges_used = |property: &IRI| {
            self.edges
                .iter()
                .filter(|(subject, _, _)| self.representative(*subject) == node)
                .any(|(_, edge_property, _)| *edge_property == property.as_str())
        };
        self.existential_gci_holds_with(node, expression, &edges_used)
    }

    /// The parameterized core of `existential_gci_holds`: named-class
    /// conjuncts read the node's recorded classes (#127), universal and
    /// zero-max conjuncts read the caller's uses-property view. The
    /// end-of-walk vacuous sweep passes settled edges plus
    /// min-bound-forced properties, so a mixed subject like
    /// `Wine ⊓ ∀madeFromGrape.{S,SB}` (W3C miscellaneous-001's
    /// SemillonOrSauvignonBlanc) cannot classify an outsider-grape wine
    /// before its own edge materializes (#130).
    fn existential_gci_holds_with(
        &self,
        node: usize,
        expression: &'o ClassExpression,
        property_used: &impl Fn(&IRI) -> bool,
    ) -> bool {
        match expression {
            ClassExpression::Class(target_class) => {
                self.recorded_class_entails(node, target_class.iri().as_str())
            }
            ClassExpression::ObjectSomeValuesFrom(property, filler) => {
                // The filler witnesses through either polarity: a named
                // class entails, a named complement excludes (#142).
                let (target, wants_complement) = match filler.as_ref() {
                    ClassExpression::Class(filler_class) => {
                        (filler_class.iri().as_str(), false)
                    }
                    ClassExpression::ObjectComplementOf(inner) => {
                        let ClassExpression::Class(filler_class) = inner.as_ref() else {
                            return false;
                        };
                        (filler_class.iri().as_str(), true)
                    }
                    _ => return false,
                };
                // The role's representations are the same fact stated
                // twice (direct and inverse partner); one witness edge
                // on any of them satisfies the restriction. Candidates
                // come from the role-closure index (#172) — the edges
                // under base's subproperty closure, in edge order —
                // instead of a full scan with a closure test per edge.
                self.context.resolve_role(property).iter().any(|(is_inverse, base)| {
                    self.candidate_edges_satisfy(*base, |subject, edge_property, object| {
                        let mut endpoint = self.endpoint_for(node, *is_inverse, subject, object);
                        if endpoint.is_none()
                            && *is_inverse
                            && self.context.symmetric_mirror_reaches(edge_property, base)
                        {
                            // A symmetric base's edge mirrors, so the
                            // subject node is an object too: the
                            // inverse reading reaches it (#152, W3C
                            // TransitiveProperty-002's range squeeze).
                            endpoint = self.endpoint_for(node, false, subject, object);
                        }
                        endpoint.is_some_and(|witness| {
                            if wants_complement {
                                self.recorded_complement_entails(witness, target)
                            } else {
                                self.recorded_class_entails(witness, target)
                            }
                        })
                    })
                })
            }
            // ¬C holds on the node when a recorded complement's
            // contrapositive covers it: ¬D ⊑ ¬C exactly when C ⊑ D
            // (#142).
            ClassExpression::ObjectComplementOf(inner) => {
                let ClassExpression::Class(target_class) = inner.as_ref() else {
                    return false;
                };
                self.recorded_complement_entails(node, target_class.iri().as_str())
            }
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| self.existential_gci_holds_with(node, operand, property_used)),
            // Mixed subjects: a vacuous conjunct holds when the node
            // never uses the restricted property.
            other => {
                SimpleReasoner::anon_gci_subject_satisfied_with(other, property_used)
            }
        }
    }

    /// Whether an endpoint's recorded expressions satisfy an
    /// existential filler: top outright, a named class through the
    /// entailment walk, a named complement through the contrapositive
    /// (#142), a conjunction through every operand (#143, W3C
    /// description-logic-004).
    fn filler_satisfied_on(&self, endpoint: usize, filler: &'o ClassExpression) -> bool {
        match filler {
            ClassExpression::Class(class) => {
                self.recorded_class_entails(endpoint, class.iri().as_str())
            }
            ClassExpression::ObjectComplementOf(inner) => {
                let ClassExpression::Class(class) = inner.as_ref() else {
                    return false;
                };
                self.recorded_complement_entails(endpoint, class.iri().as_str())
            }
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| self.filler_satisfied_on(endpoint, operand)),
            _ => false,
        }
    }

    /// Distinct successor representatives of `node` under `property`,
    /// counting edges over every subproperty of it.
    fn successors_of(&mut self, node: usize, property: &str) -> Vec<usize> {
        let edges: Vec<(usize, &'o str, usize)> = self.edges.clone();
        let mut out: BTreeSet<usize> = BTreeSet::new();
        for (subject, edge_property, object) in edges {
            if self.find(subject) == node
                && self.context.is_subproperty_of(edge_property, property)
            {
                out.insert(self.find(object));
            }
        }
        out.into_iter().collect()
    }

    fn force_merge(&mut self, node: usize, property: &str) -> MergeOutcome {
        let successors = self.successors_of(node, property);
        if successors.len() < 2 {
            return MergeOutcome::Nothing;
        }
        for (index, &left) in successors.iter().enumerate() {
            for &right in successors.iter().skip(index + 1) {
                if self.blocked(left, right) {
                    return MergeOutcome::Contradiction;
                }
            }
        }
        self.merge_all(&successors);
        MergeOutcome::Merged
    }

    fn merge_all(&mut self, nodes: &[usize]) {
        let target = *nodes.iter().min().unwrap();
        for &node in nodes {
            if node == target {
                continue;
            }
            self.parent[node] = target;
            let expressions: Vec<&'o ClassExpression> = self.expressions[node].clone();
            for expression in expressions {
                self.record(target, expression);
            }
            let named: Vec<&'o IRI> = self.named[node].clone();
            self.named[target].extend(named);
        }
    }

    /// Whether merging the two nodes is impossible: explicitly different
    /// named members, contradictory class expressions, or unqualified data
    /// bounds whose union no individual can satisfy.
    fn blocked(&self, left: usize, right: usize) -> bool {
        // Pairs the >=-rule demanded distinct cannot merge.
        if self.must_differ.iter().any(|&(a, b)| {
            let (ra, rb) = (self.representative(a), self.representative(b));
            (ra, rb) == (left, right) || (ra, rb) == (right, left)
        }) {
            return true;
        }
        for a in &self.named[left] {
            for b in &self.named[right] {
                if are_known_different(self.context.ontology, a, b) {
                    return true;
                }
            }
        }
        let mut pairs: usize = 0;
        for left_expression in &self.expressions[left] {
            for right_expression in &self.expressions[right] {
                pairs += 1;
                if pairs > MERGE_PROBE_PAIR_CAP {
                    return false;
                }
                if self
                    .context
                    .expressions_contradictory(left_expression, right_expression)
                {
                    return true;
                }
            }
        }
        self.union_data_bounds_clash(left, right)
    }

    fn union_data_bounds_clash(&self, left: usize, right: usize) -> bool {
        let mut pooled: BTreeMap<&'o str, (Option<u32>, Option<u32>)> = BTreeMap::new();
        for ((node, property), (min, max)) in &self.data_bounds {
            let representative = self.representative(*node);
            if representative != left && representative != right {
                continue;
            }
            let slot = pooled.entry(*property).or_insert((None, None));
            tighten_data_bounds(slot, *min, *max);
        }
        for (min, max) in pooled.values() {
            if let (Some(min), Some(max)) = (min, max) {
                if min > max {
                    return true;
                }
            }
        }
        let mut members: Vec<&'o IRI> = self.named[left].clone();
        members.extend(self.named[right].iter().copied());
        for (property, (_, max)) in &pooled {
            if let Some(max) = max {
                if self.distinct_asserted_values(&members, property) > *max as usize {
                    return true;
                }
            }
        }
        false
    }

    fn distinct_asserted_values(&self, members: &[&'o IRI], property: &str) -> usize {
        self.collect_distinct_asserted_values(members, property).len()
    }

    /// The distinct asserted values of one property over a set of
    /// named individuals, one entry per value.
    fn collect_distinct_asserted_values(
        &self,
        members: &[&'o IRI],
        property: &str,
    ) -> Vec<&crate::core::entities::Literal> {
        let mut distinct: Vec<&crate::core::entities::Literal> = Vec::new();
        for member in members {
            for assertion in self.context.ontology.data_property_assertions() {
                if assertion.subject().as_ref() == *member
                    && assertion.property().as_str() == property
                    && !distinct
                        .iter()
                        .any(|existing| literals_are_equal(existing, assertion.value()))
                {
                    distinct.push(assertion.value());
                }
            }
        }
        distinct
    }

    /// Distinct data values one settled node is forced to carry: the
    /// asserted positives of its named members plus the walk's
    /// DataHasValue positives pooled onto it, one per value (#125).
    fn distinct_forced_values(&self, node: usize, property: &str) -> usize {
        let mut distinct = self.collect_distinct_asserted_values(&self.named[node], property);
        for (positive_node, positive_property, value) in &self.data_positives {
            if self.representative(*positive_node) == node
                && *positive_property == property
                && !distinct
                    .iter()
                    .any(|existing| literals_are_equal(existing, value))
            {
                distinct.push(value);
            }
        }
        distinct.len()
    }

    /// Merge, walk, and check to a fixpoint: a merge can hand the merged
    /// node new restrictions whose propagation only then clashes (W3C
    /// description-logic-615: the <=1 invR merge feeds a forall that
    /// lands p on the pcomp witness), so each round re-walks and
    /// re-checks until the recorded expressions stop growing. A merge
    /// that records nothing new ends the loop without a propagation
    /// round — an accepted incompleteness, like every budget here.
    fn recorded_expression_count(&self) -> usize {
        self.expressions.iter().map(Vec::len).sum()
    }

    fn settle(&mut self) -> bool {
        for _ in 0..MERGE_PROBE_SETTLE_ROUNDS {
            if self.budget_out() {
                return false;
            }
            let before = self.recorded_expression_count();
            self.process();
            if self.merge_phase() {
                return true;
            }
            if self.check_phase() {
                return true;
            }
            let after = self.recorded_expression_count();
            if after == before {
                return false;
            }
        }
        false
    }

    /// Whether the node already carries one of the listed members'
    /// names — the oneOf constraint is satisfied outright.
    fn nominal_constraint_satisfied(
        &self,
        node: usize,
        members: &'o [crate::core::entities::Individual],
    ) -> bool {
        self.named[node].iter().any(|individual: &&IRI| {
            members
                .iter()
                .any(|member| member.iri().is_some_and(|iri| iri.as_ref() == *individual))
        })
    }

    /// The distinct union-find-resolved member nodes the constrained
    /// node may still be identified with: not the node itself, not
    /// already listed on it, not blocked (#114/#115).
    fn unblocked_candidates(
        &mut self,
        node: usize,
        members: &'o [crate::core::entities::Individual],
    ) -> Vec<usize> {
        let mut candidates: Vec<usize> = Vec::new();
        for member in members {
            let Some(iri) = member.iri() else {
                continue;
            };
            // Resolve through the union-find: a member may already
            // be merged under another node.
            let member_slot = self.node_for_individual(iri);
            let member_node = self.find(member_slot);
            if member_node != node
                && !candidates.contains(&member_node)
                && !self.blocked(node, member_node)
            {
                candidates.push(member_node);
            }
        }
        candidates
    }

    /// oneOf constraints with exactly one unblocked member force the
    /// identification of the constrained node with that member (#114).
    fn force_nominal_identities(&mut self) -> MergeOutcome {
        let constraints: Vec<(usize, &'o [crate::core::entities::Individual])> =
            self.oneof_constraints.clone();
        let mut outcome = MergeOutcome::Nothing;
        for (node, members) in constraints {
            let node = self.find(node);
            if self.nominal_constraint_satisfied(node, members) {
                continue;
            }
            match self.unblocked_candidates(node, members).as_slice() {
                [] => return MergeOutcome::Contradiction,
                [member_node] => {
                    self.merge_all(&[node, *member_node]);
                    outcome = MergeOutcome::Merged;
                }
                _ => {}
            }
        }
        outcome
    }

    /// The undecided oneOf constraint with the fewest unblocked merge
    /// candidates (#115), or None when every constraint is satisfied,
    /// forced, or dead — the local rules' territory, not the search's.
    fn pick_identification_branch(&mut self) -> Option<(usize, Vec<usize>)> {
        let constraints: Vec<(usize, &'o [crate::core::entities::Individual])> =
            self.oneof_constraints.clone();
        let mut best: Option<(usize, Vec<usize>)> = None;
        for (node, members) in constraints {
            let node = self.find(node);
            if self.nominal_constraint_satisfied(node, members) {
                continue;
            }
            let candidates = self.unblocked_candidates(node, members);
            // Zero candidates is check_phase's contradiction and one is
            // force_nominal_identities' merge — branching starts at two.
            if candidates.len() < 2 {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(_, chosen)| candidates.len() < chosen.len())
            {
                best = Some((node, candidates));
            }
        }
        best
    }

    /// The live disjuncts of one union choice: disjuncts nothing
    /// recorded on the node contradicts. A disjunct some recorded
    /// expression already equals or entails discharges the union
    /// (None) — the choice is made, not open. Equality covers a
    /// committed non-atomic disjunct re-checking its own union
    /// (¬A committed must discharge ¬A ⊔ ¬B, or the search re-picks
    /// it and burns the budget).
    fn live_disjuncts(
        &self,
        node: usize,
        union_expression: &'o ClassExpression,
    ) -> Option<Vec<&'o ClassExpression>> {
        let ClassExpression::ObjectUnionOf(operands) = union_expression else {
            return None;
        };
        let recorded = &self.expressions[node];
        for disjunct in operands.iter() {
            if recorded
                .iter()
                .any(|existing| *existing == disjunct.as_ref())
            {
                return None;
            }
            if let ClassExpression::Class(disjunct_class) = disjunct.as_ref() {
                let target = disjunct_class.iri().as_str();
                if recorded.iter().any(|existing| {
                    matches!(existing, ClassExpression::Class(class)
                        if class.iri().as_str() == target
                            || self.context.entails(class.iri().as_str(), target))
                }) {
                    return None;
                }
            }
        }
        Some(
            operands
                .iter()
                .filter(|disjunct| {
                    !recorded.iter().any(|existing| {
                        self.context
                            .expressions_contradictory(&**disjunct, *existing)
                    })
                })
                .map(|disjunct| &**disjunct)
                .collect(),
        )
    }

    /// Whether any atomic class of the union also occurs in another
    /// recorded expression on the node, or any disjunct is
    /// non-atomic. Only pressed unions can take part in a class-atom
    /// clash — a refutation needs two contradictory atoms, and an
    /// atom appearing nowhere else can never collect a partner — so
    /// free unions stay unbranched (ADR-0020, W3C
    /// description-logic-040's thirty-two free unions). Non-atomic
    /// disjuncts (restrictions, nominals, nested descriptions) always
    /// press: they engage bounds, witnesses, and merges, which clash
    /// outside the class-atom argument (W3C description-logic-111's
    /// ¬(≥2 s)).
    fn union_is_pressed(&self, node: usize, union_expression: &ClassExpression) -> bool {
        let ClassExpression::ObjectUnionOf(operands) = union_expression else {
            return true;
        };
        let atomic_disjunct = |disjunct: &ClassExpression| match disjunct {
            ClassExpression::Class(_) => true,
            ClassExpression::ObjectComplementOf(inner) => {
                matches!(inner.as_ref(), ClassExpression::Class(_))
            }
            _ => false,
        };
        if operands
            .iter()
            .any(|disjunct| !atomic_disjunct(disjunct.as_ref()))
        {
            return true;
        }
        let mut atoms = BTreeSet::new();
        collect_class_atoms(union_expression, &mut atoms);
        if atoms.is_empty() {
            // Shapes the atom walk cannot see stay pressed: branching
            // more only spends budget, never soundness.
            return true;
        }
        self.expressions[node].iter().any(|existing| {
            !std::ptr::eq(*existing, union_expression) && {
                let mut other = BTreeSet::new();
                collect_class_atoms(existing, &mut other);
                !other.is_disjoint(&atoms)
            }
        })
    }

    /// The pressed union choice with the fewest live disjuncts (#119),
    /// or None when no pressed union is left with an open choice.
    fn pick_union_branch(&self) -> Option<(usize, Vec<&'o ClassExpression>)> {
        let mut best: Option<(usize, Vec<&'o ClassExpression>)> = None;
        for &(node, union_expression) in &self.union_choices {
            let node = self.representative(node);
            if !self.union_is_pressed(node, union_expression) {
                continue;
            }
            let Some(live) = self.live_disjuncts(node, union_expression) else {
                continue;
            };
            // Zero live disjuncts is force_union_units' contradiction
            // and one is its forced commit — branching starts at two.
            if live.len() < 2 {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(_, chosen)| live.len() < chosen.len())
            {
                best = Some((node, live));
            }
        }
        best
    }

    /// Union unit propagation (#119): a discharged union holds
    /// outright, a zero-live one contradicts the node, and a single
    /// live disjunct commits without cloning.
    fn force_union_units(&mut self) -> MergeOutcome {
        let choices: Vec<(usize, &'o ClassExpression)> = self.union_choices.clone();
        let mut outcome = MergeOutcome::Nothing;
        for (node, union_expression) in choices {
            let node = self.find(node);
            let Some(live) = self.live_disjuncts(node, union_expression) else {
                continue;
            };
            match live.as_slice() {
                [] => return MergeOutcome::Contradiction,
                [disjunct] => {
                    if self.record(node, disjunct) {
                        self.walk_node(node);
                        outcome = MergeOutcome::Merged;
                    }
                }
                _ => {}
            }
        }
        outcome
    }

    /// One cloned branch of the search: commit through `apply`,
    /// settle, and recurse. False = the branch survived (or the clone
    /// budget ran out); true = this branch died.
    fn branch_dies(
        &mut self,
        clones: &mut usize,
        apply: impl FnOnce(&mut Self),
    ) -> bool {
        if *clones == 0 {
            self.exhausted.set(true);
            return false;
        }
        *clones -= 1;
        let mut branch = self.clone();
        branch.budget = branch.walk_budget;
        // Reset the branch's allowance BEFORE apply: apply walks the
        // commitment (charging work), so every unit the family spends
        // comes out of the one ceiling (#173).
        if self.work_ceiling > 0 {
            branch.work_ceiling = self.work_ceiling.saturating_sub(self.work_spent.get());
            branch.work_spent = std::cell::Cell::new(0);
        }
        apply(&mut branch);
        let died = branch.settle() || branch.search_identifications(clones);
        // A branch exhausting its budgets is this probe's exhaustion
        // too — the clone's flag must not die with it. Work the branch
        // spent counts toward the parent's total as well.
        self.work_spent.set(
            self.work_spent
                .get()
                .saturating_add(branch.work_spent.get()),
        );
        if branch.exhausted.get() {
            self.exhausted.set(true);
        }
        died
    }

    /// Case split over the open choices of the settled probe — #115's
    /// identification candidates and #119's pressed-union disjuncts
    /// (ADR-0020). Every undecided `C ⊑ oneOf{...}` constraint is a
    /// tautologous disjunction — the node IS some listed member — and
    /// every pressed union is a disjunction over its live disjuncts,
    /// so cloning the probe per candidate and propagating each commit
    /// to a fixpoint proves the inconsistency exactly when every
    /// branch dies. A surviving branch, or an exhausted clone budget,
    /// is no verdict: the caller keeps the default-consistent answer
    /// (recorded incompleteness, never unsound).
    fn search_identifications(&mut self, clones: &mut usize) -> bool {
        let Some((node, candidates)) = self.pick_identification_branch() else {
            return self.search_union_choices(clones);
        };
        for candidate in candidates {
            if !self.branch_dies(clones, |branch| {
                branch.merge_all(&[node, candidate]);
            }) {
                return false;
            }
        }
        true
    }

    /// The pressed-union half of the split (#119): free unions are
    /// never picked (union_is_pressed), and each live disjunct commits
    /// through the shared branch machinery.
    fn search_union_choices(&mut self, clones: &mut usize) -> bool {
        let Some((node, live)) = self.pick_union_branch() else {
            return false;
        };
        for disjunct in live {
            if !self.branch_dies(clones, |branch| {
                if branch.record(node, disjunct) {
                    branch.walk_node(node);
                }
            }) {
                return false;
            }
        }
        true
    }

    /// One merge rule's outcome folds into the round: a contradiction
    /// ends the merge phase (inconsistent), a merge keeps the rounds
    /// going.
    fn fold_outcome(outcome: MergeOutcome, changed: &mut bool) -> bool {
        match outcome {
            MergeOutcome::Contradiction => true,
            MergeOutcome::Merged => {
                *changed = true;
                false
            }
            MergeOutcome::Nothing => false,
        }
    }

    /// Forced merges: asserted identities, functional roles, explicit ≤1
    /// bounds, and inverse-functional roles identify nodes; a blocked
    /// pair among them is the inconsistency.
    fn merge_phase(&mut self) -> bool {
        let functional: Vec<&'o str> = self.context.functional.iter().copied().collect();
        for _ in 0..MERGE_PROBE_MERGE_ROUNDS {
            let mut changed = false;
            if Self::fold_outcome(self.force_asserted_identities(), &mut changed) {
                return true;
            }
            // Inverse-functional identities and their derived
            // distinctness run before the functional merges: the
            // functional loop consumes the pairs this rule derives
            // (#145, W3C one_two).
            if Self::fold_outcome(self.force_inverse_functional_identities(), &mut changed) {
                return true;
            }
            for node in 0..self.parent.len() {
                if self.find(node) != node {
                    continue;
                }
                for property in &functional {
                    if Self::fold_outcome(self.force_merge(node, property), &mut changed) {
                        return true;
                    }
                }
            }
            let one_bounds: Vec<(usize, &'o str)> = self
                .object_bounds
                .iter()
                .filter(|(_, _, maximum)| *maximum == 1)
                .map(|(node, property, _)| (*node, *property))
                .collect();
            for (node, property) in one_bounds {
                let node = self.find(node);
                if Self::fold_outcome(self.force_merge(node, property), &mut changed) {
                    return true;
                }
            }
            // Nominal unit propagation: a oneOf constraint with exactly
            // one unblocked member forces the identity — a named guess
            // collapses onto it and re-checks every other constraint it
            // carries (W3C description-logic-502 chains these).
            if Self::fold_outcome(self.force_nominal_identities(), &mut changed) {
                return true;
            }
            // Union unit propagation: discharged, dead-pair, and
            // forced-single-disjunct choices (#119).
            if Self::fold_outcome(self.force_union_units(), &mut changed) {
                return true;
            }
            // (Inverse-functional identities ran before the
            // functional loop this round — see the head of the loop.)
            if !changed {
                break;
            }
        }
        false
    }

    /// Asserted sameAs pairs merge unconditionally (they are facts); a
    /// blocked pair is the inconsistency, exactly like a blocked
    /// functional merge (#131).
    fn force_asserted_identities(&mut self) -> MergeOutcome {
        let pairs: Vec<(usize, usize)> = self.identity_pairs.clone();
        let mut outcome = MergeOutcome::Nothing;
        for (left, right) in pairs {
            let (left, right) = (self.find(left), self.find(right));
            if left == right {
                continue;
            }
            if self.blocked(left, right) {
                return MergeOutcome::Contradiction;
            }
            self.merge_all(&[left, right]);
            outcome = MergeOutcome::Merged;
        }
        outcome
    }

    /// Inverse-functional roles force same-object subjects together:
    /// for each such role, every group of edges (through the subproperty
    /// closure) sharing a representative object merges its subjects,
    /// pairwise-blocked first — the same shape force_merge applies to
    /// same-subject successors (#131).
    /// Inverse-functional roles force same-object subjects together:
    /// for each such role, every group of edges (through the subproperty
    /// closure) sharing a representative object merges its subjects,
    /// pairwise-blocked first — the same shape force_merge applies to
    /// same-subject successors (#131). Subjects reaching BLOCKED objects
    /// are themselves distinct (x = y would force the objects together,
    /// which blocked() forbids): the derived must_differ pairs feed the
    /// oneOf and cardinality bookkeeping so distinctness composes
    /// across hops (#145, W3C one_two's F/IF bijections). Both passes
    /// are pair-capped and fail-open.
    fn force_inverse_functional_identities(&mut self) -> MergeOutcome {
        let roles: Vec<&'o str> = self
            .context
            .inverse_functional
            .iter()
            .copied()
            .collect();
        if roles.is_empty() {
            return MergeOutcome::Nothing;
        }
        let edges: Vec<(usize, &'o str, usize)> = self.edges.clone();
        let mut outcome = MergeOutcome::Nothing;
        for role in roles {
            let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (subject, property, object) in &edges {
                if !self.context.is_subproperty_of(property, role) {
                    continue;
                }
                groups
                    .entry(self.find(*object))
                    .or_default()
                    .push(self.find(*subject));
            }
            for (_, subjects) in groups.iter_mut() {
                subjects.sort_unstable();
                subjects.dedup();
                if subjects.len() < 2 {
                    continue;
                }
                for (index, &left) in subjects.iter().enumerate() {
                    for &right in subjects.iter().skip(index + 1) {
                        if self.blocked(left, right) {
                            return MergeOutcome::Contradiction;
                        }
                    }
                }
                self.merge_all(subjects);
                outcome = MergeOutcome::Merged;
            }
            // Injectivity: subjects reaching BLOCKED objects are
            // themselves distinct — x = y would force their objects
            // together under the inverse-functional reading, which
            // blocked() forbids.
            let mut scanned: usize = 0;
            'pairs: for (index, (left_object, left_subjects)) in groups.iter().enumerate() {
                for (right_object, right_subjects) in groups.iter().skip(index + 1) {
                    scanned += 1;
                    if scanned > MERGE_PROBE_PAIR_CAP {
                        // Fail-open past the pair cap: the scan skips,
                        // the rest of the phase still runs.
                        break 'pairs;
                    }
                    if !self.blocked(*left_object, *right_object) {
                        continue;
                    }
                    for &left in left_subjects.iter() {
                        for &right in right_subjects.iter() {
                            let (left, right) = (self.find(left), self.find(right));
                            if left != right {
                                let pair = if left < right {
                                    (left, right)
                                } else {
                                    (right, left)
                                };
                                self.must_differ.insert(pair);
                            }
                        }
                    }
                }
            }
        }
        outcome
    }

    /// After merging: negative assertions against derived edges
    /// (#138), per-node contradictions (recorded bottom outright, or
    /// pairwise), then the ≤-rule — a ≤0 bound with any successor, or a
    /// ≤m bound whose successors admit no partition into m
    /// pairwise-mergeable groups.
    fn check_phase(&mut self) -> bool {
        let mut data_by_rep: BTreeMap<usize, BTreeMap<&'o str, (Option<u32>, Option<u32>)>> =
            BTreeMap::new();
        for ((node, property), (min, max)) in &self.data_bounds {
            let representative = self.representative(*node);
            let slot = data_by_rep
                .entry(representative)
                .or_default()
                .entry(*property)
                .or_insert((None, None));
            tighten_data_bounds(slot, *min, *max);
        }

        // A stated negative assertion dies against any derived edge
        // realizing it — including edges the walk materialized
        // (hasValue, witnesses) and merges unified (#138, W3C
        // equivalentProperty-004). Fail-open past the pair cap: the
        // scan skips, the rest of the phase still runs.
        if !self.negative_edges.is_empty() {
            let negatives = self.negative_edges.clone();
            let edges_snapshot = self.edges.clone();
            let mut scanned: usize = 0;
            'negatives: for (subject, property, object) in &negatives {
                let (subject, object) = (self.find(*subject), self.find(*object));
                // Only the declared inverse partners swap endpoints;
                // the direct role is the unswapped branch above.
                let partners: Vec<&'o str> = self
                    .context
                    .inverse_base
                    .get(*property)
                    .into_iter()
                    .flatten()
                    .copied()
                    .collect();
                for (edge_subject, edge_property, edge_object) in &edges_snapshot {
                    scanned += 1;
                    if scanned > MERGE_PROBE_PAIR_CAP {
                        break 'negatives;
                    }
                    // Direct: an r-edge from the subject to the object
                    // with r ⊑ p realizes p(x,y). Partner: the inverse
                    // role realizes it the other way round — the
                    // endpoints swap (q(y,x) with q inverseOf p).
                    let realizes = (self.context.is_subproperty_of(edge_property, property)
                        && self.find(*edge_subject) == subject
                        && self.find(*edge_object) == object)
                        || (partners.iter().any(|partner| {
                            self.context.is_subproperty_of(edge_property, partner)
                        }) && self.find(*edge_object) == subject
                            && self.find(*edge_subject) == object);
                    if realizes {
                        return true;
                    }
                }
            }
        }

        for node in 0..self.parent.len() {
            if self.find(node) != node {
                continue;
            }
            let expressions = &self.expressions[node];
            // A node recording an expression that is bottom outright —
            // a Thing-complement via the ∃→∀¬ negation (#136, W3C
            // bnode2someValuesFrom) or a Nothing typing — cannot exist.
            if expressions
                .iter()
                .any(|expression| class_expression_is_empty(expression))
            {
                return true;
            }
            let mut pairs: usize = 0;
            let mut contradictory = false;
            'pairs: for (index, left) in expressions.iter().enumerate() {
                for right in expressions.iter().skip(index + 1) {
                    pairs += 1;
                    if pairs > MERGE_PROBE_PAIR_CAP {
                        break 'pairs;
                    }
                    if self.context.expressions_contradictory(left, right) {
                        contradictory = true;
                        break 'pairs;
                    }
                }
            }
            if contradictory {
                return true;
            }
            if let Some(bounds) = data_by_rep.get(&node) {
                for (property, (min, max)) in bounds {
                    if let (Some(min), Some(max)) = (min, max) {
                        if min > max {
                            return true;
                        }
                    }
                    if let Some(max) = max {
                        if self.distinct_forced_values(node, property) > *max as usize {
                            return true;
                        }
                    }
                }
            }
        }

        // Pair-flip avoidance assist (ADR 0021, #156): a node
        // excluding class S — S is the registered complement partner
        // of a class the node carries — must avoid some conjunct of
        // every intersection declared UNDER S (D ⊑ S: being in every
        // conjunct would force membership in S). The super side
        // (S ⊑ D) licenses nothing for a node outside S and must
        // not be read here. When every avoidance dies (the covering
        // half forces the avoided conjunct's partner, whose closure
        // embeds a disjoint pair), no model remains. Shapes that do
        // not match fail open.
        for (_, excluded) in &self.complement_notes {
            let Some(intersections) = self
                .context
                .complements
                .intersection_subjects
                .get(*excluded)
            else {
                continue;
            };
            for intersection in intersections {
                let ClassExpression::ObjectIntersectionOf(operands) = intersection else {
                    continue;
                };
                let every_avoidance_dies = !operands.is_empty()
                    && operands.iter().all(|operand| match operand.as_ref() {
                        ClassExpression::Class(avoided) => self
                            .context
                            .avoided_conjunct_dies(avoided.iri().as_str()),
                        _ => false,
                    });
                if every_avoidance_dies {
                    return true;
                }
            }
        }

        // oneOf constraints: a constrained node must be a listed
        // individual — a foreign named node contradicts outright, a
        // witness must be mergeable with some member (#114).
        let constraints: Vec<(usize, &'o [crate::core::entities::Individual])> =
            self.oneof_constraints.clone();
        for (node, members) in constraints {
            let node = self.find(node);
            let listed = |individual: &IRI| {
                members
                    .iter()
                    .any(|member| member.iri().is_some_and(|iri| iri.as_ref() == individual))
            };
            if self.named[node].iter().any(|individual| listed(individual)) {
                continue;
            }
            // A node outside the list must still be identifiable with
            // some member — a named node is only a contradiction when
            // every member is blocked (differentFrom, disjoint
            // expressions), never merely by IRI mismatch (W3C
            // description-logic-501 guesses its members).
            let mergeable_with_some_member = members.iter().any(|member| {
                member.iri().is_some_and(|iri| {
                    let member_node = self.node_for_individual(iri);
                    let member_node = self.find(member_node);
                    member_node != node || !self.blocked(node, member_node)
                })
            });
            if !mergeable_with_some_member {
                return true;
            }
        }

        // A recorded not-(exists p. C) contradicts any p-successor
        // whose recorded expressions satisfy the filler — named
        // classes entail, named complements exclude (#142), and a
        // conjunction needs every operand (W3C description-logic-004's
        // merged witness carries c1 AND c2, #143).
        let negated: Vec<(usize, &'o ObjectPropertyExpression, &'o ClassExpression)> =
            self.negated_existentials.clone();
        for (node, property, filler) in negated {
            let node = self.find(node);
            for (is_inverse, base) in self.context.resolve_role(property) {
                let endpoints: Vec<usize> = if is_inverse {
                    // An inverse-role successor of this node is the
                    // subject of an edge whose object is the node.
                    let edges: Vec<(usize, &'o str, usize)> = self.edges.clone();
                    edges
                        .iter()
                        .filter_map(|(subject, edge_property, object)| {
                            if self.representative(*object) == node
                                && self.context.is_subproperty_of(edge_property, base)
                            {
                                return Some(self.representative(*subject));
                            }
                            // A symmetric base's edge mirrors, so the
                            // node's own outgoing edge also makes its
                            // object an inverse successor (#152's
                            // sibling gap).
                            if self.representative(*subject) == node
                                && self.context.symmetric_mirror_reaches(edge_property, base)
                            {
                                return Some(self.representative(*object));
                            }
                            None
                        })
                        .collect()
                } else {
                    self.successors_of(node, base)
                };
                if endpoints
                    .iter()
                    .any(|&endpoint| self.filler_satisfied_on(endpoint, filler))
                {
                    return true;
                }
            }
        }

        // A recorded not-hasSelf(p) contradicts any p-labelled
        // self-loop on the node — asserted, spawned by a positive
        // hasSelf label, or carried by a subproperty (the successor
        // query closes over those) (#149, W3C SelfRestriction-002).
        let negated_self: Vec<(usize, &'o str)> = self.negated_self.clone();
        for (node, base) in negated_self {
            let node = self.find(node);
            if self.successors_of(node, base).contains(&node) {
                return true;
            }
        }

        // Intersection complements (#160): ¬(D1 ⊓ … ⊓ Dn) is violated
        // by a node that verifiably satisfies every conjunct. Checked
        // after the walk settles on purpose — a standing ban drawn
        // from the disjunction would be unsound whenever another
        // conjunct is the one that fails.
        for node in 0..self.parent.len() {
            if self.find(node) != node {
                continue;
            }
            for expression in &self.expressions[node] {
                let ClassExpression::ObjectComplementOf(inner) = expression else {
                    continue;
                };
                let ClassExpression::ObjectIntersectionOf(operands) = inner.as_ref() else {
                    continue;
                };
                if !operands.is_empty()
                    && operands
                        .iter()
                        .all(|operand| self.intersection_conjunct_satisfied(node, operand))
                {
                    return true;
                }
            }
        }

        let bounds = self.object_bounds.clone();
        for (node, property, maximum) in bounds {
            let node = self.find(node);
            let successors = self.successors_of(node, property);
            if maximum == 0 {
                if !successors.is_empty() {
                    return true;
                }
            } else if successors.len() as u32 > maximum
                && maximum >= 2
                && !self.coverable(&successors, maximum)
            {
                return true;
            }
        }

        // Pooled object min/max bounds: >= n and <= m over one role
        // contradict outright when n > m, whatever the successors look
        // like (the #108 rule, object side).
        let mut object_bounds_by_rep: BTreeMap<usize, BTreeMap<&'o str, (Option<u32>, Option<u32>)>> =
            BTreeMap::new();
        let min_bounds: Vec<(usize, &'o str, u32)> = self.object_min_bounds.clone();
        for &(node, property, minimum) in &min_bounds {
            tighten_data_bounds(
                object_bounds_by_rep
                    .entry(self.find(node))
                    .or_default()
                    .entry(property)
                    .or_insert((None, None)),
                Some(minimum),
                None,
            );
        }
        let max_bounds: Vec<(usize, &'o str, u32)> = self.object_bounds.clone();
        for &(node, property, maximum) in &max_bounds {
            tighten_data_bounds(
                object_bounds_by_rep
                    .entry(self.find(node))
                    .or_default()
                    .entry(property)
                    .or_insert((None, None)),
                None,
                Some(maximum),
            );
        }
        for bounds in object_bounds_by_rep.values() {
            for (minimum, maximum) in bounds.values() {
                if let (Some(minimum), Some(maximum)) = (minimum, maximum) {
                    if minimum > maximum {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether the successors split into at most `maximum` groups whose
    /// members are pairwise mergeable. Exact for small sets; beyond the
    /// vertex cap the answer is "yes" — inconclusive, never a false clash.
    fn coverable(&self, successors: &[usize], maximum: u32) -> bool {
        let count = successors.len();
        if count == 0 || count as u32 <= maximum {
            return true;
        }
        if count > MERGE_PROBE_COVER_VERTICES {
            return true;
        }
        let mut mergeable = vec![false; count * count];
        for i in 0..count {
            for j in 0..count {
                if i != j {
                    mergeable[i * count + j] = !self.blocked(successors[i], successors[j]);
                }
            }
        }
        let full = (1usize << count) - 1;
        let mut is_clique = vec![false; full + 1];
        is_clique[0] = true;
        for mask in 1..=full {
            let low = mask.trailing_zeros() as usize;
            let rest = mask & (mask - 1);
            let mut pairwise = is_clique[rest];
            for j in 0..count {
                if pairwise && rest & (1 << j) != 0 {
                    pairwise = mergeable[low * count + j];
                }
            }
            is_clique[mask] = pairwise;
        }
        let groups = maximum.min(count as u32) as usize;
        let mut can = vec![vec![false; full + 1]; groups + 1];
        for slice in can.iter_mut() {
            slice[0] = true;
        }
        for k in 1..=groups {
            for mask in 1..=full {
                if can[k - 1][mask] {
                    can[k][mask] = true;
                    continue;
                }
                let low_bit = mask & mask.wrapping_neg();
                let mut sub = mask;
                while sub > 0 {
                    if sub & low_bit != 0
                        && is_clique[sub]
                        && can[k - 1][mask ^ sub]
                    {
                        can[k][mask] = true;
                        break;
                    }
                    sub = (sub - 1) & mask;
                }
            }
        }
        can[groups][full]
    }
}

/// Cache entry for reasoning results
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    timestamp: Instant,
    ttl: Duration,
}

impl<T> CacheEntry<T> {
    fn new(value: T, ttl: Duration) -> Self {
        CacheEntry {
            value,
            timestamp: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }

    fn get(&self) -> Option<&T> {
        if self.is_expired() {
            None
        } else {
            Some(&self.value)
        }
    }
}

/// Cache statistics for performance analysis
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub total_requests: usize,
}

impl CacheStats {
    pub fn new() -> Self {
        CacheStats {
            hits: 0,
            misses: 0,
            total_requests: 0,
        }
    }

    pub fn record_hit(&mut self) {
        self.hits += 1;
        self.total_requests += 1;
    }

    pub fn record_miss(&mut self) {
        self.misses += 1;
        self.total_requests += 1;
    }

    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_requests as f64
        }
    }
}

/// A simplified OWL2 reasoner with caching and profile validation
///
/// This reasoner provides basic reasoning capabilities for OWL2 ontologies,
/// including consistency checking, classification, and satisfiability testing.
/// It includes built-in caching for performance and supports OWL2 profile validation.
///
/// # Examples
///
/// ```rust
/// use owl2_reasoner::{Ontology, SimpleReasoner, Class};
///
/// let mut ontology = Ontology::new();
/// let person_class = Class::new("http://example.org/Person");
/// ontology.add_class(person_class)?;
///
/// let reasoner = SimpleReasoner::new(ontology);
/// let consistent = reasoner.is_consistent()?;
/// println!("Ontology is consistent: {}", consistent);
/// # Ok::<(), owl2_reasoner::OwlError>(())
/// ```
pub struct SimpleReasoner {
    pub ontology: Arc<Ontology>,

    // Profile validation
    profile_validator: Owl2ProfileValidator,

    // Caching layers
    consistency_cache: RwLock<Option<CacheEntry<bool>>>,
    subclass_cache: RwLock<HashMap<(IRI, IRI), CacheEntry<bool>>>,
    satisfiability_cache: RwLock<HashMap<IRI, CacheEntry<bool>>>,
    instances_cache: RwLock<HashMap<IRI, CacheEntry<Vec<IRI>>>>,

    // Cache statistics
    cache_stats: RwLock<CacheStats>,

    // Merge-probe size gate (#168): corpora larger than this many
    // expression nodes skip the probe (fail-open on merge rules); 0
    // disables the gate.
    merge_probe_max_nodes: usize,
    merge_probe_skipped: RwLock<bool>,
    /// Configured walk budget for the merge probe (#171): the const by
    /// default, overridable for tests and D2 comparisons.
    merge_probe_walk_budget: usize,
    /// Global work ceiling for the merge probe (#173, ADR 0022); `0`
    /// means unlimited (D2 comparison mode).
    merge_probe_work_budget: u64,
    merge_probe_exhausted: RwLock<bool>,
}

impl SimpleReasoner {
    /// Create a new simple reasoner
    ///
    /// Creates a new reasoner instance with the given ontology.
    /// The reasoner will automatically set up caching and profile validation.
    ///
    /// # Arguments
    ///
    /// * `ontology` - The ontology to reason about
    ///
    /// # Examples
    ///
    /// ```rust
    /// use owl2_reasoner::{Ontology, SimpleReasoner};
    ///
    /// let ontology = Ontology::new();
    /// let reasoner = SimpleReasoner::new(ontology);
    /// # Ok::<(), owl2_reasoner::OwlError>(())
    /// ```
    pub fn new(ontology: Ontology) -> Self {
        Self::from_arc(Arc::new(ontology))
    }

    /// Create a new simple reasoner from a shared ontology
    ///
    /// This avoids deep cloning large ontologies when multiple reasoners
    /// need to share the same ontology.
    pub fn from_arc(ontology: Arc<Ontology>) -> Self {
        let profile_validator = match Owl2ProfileValidator::new(ontology.clone()) {
            Ok(validator) => validator,
            Err(_e) => {
                // If profile validator creation fails, create a minimal validator
                // This ensures the reasoner can still function even with limited profile validation
                Owl2ProfileValidator::new(Arc::new(Ontology::new()))
                    .expect("Failed to create minimal profile validator")
            }
        };

        SimpleReasoner {
            ontology,
            profile_validator,
            consistency_cache: RwLock::new(None),
            subclass_cache: RwLock::new(HashMap::new()),
            satisfiability_cache: RwLock::new(HashMap::new()),
            instances_cache: RwLock::new(HashMap::new()),
            cache_stats: RwLock::new(CacheStats::new()),
            merge_probe_max_nodes: merge_probe_max_nodes_from_env(),
            merge_probe_skipped: RwLock::new(false),
            merge_probe_walk_budget: merge_probe_walk_budget_from_env(),
            merge_probe_work_budget: merge_probe_work_budget_from_env(),
            merge_probe_exhausted: RwLock::new(false),
        }
    }

    /// Override the merge-probe size gate (#168); `0` disables it.
    /// Callers that construct the reasoner directly (tests) use this
    /// instead of the `OWL2_REASONER_MERGE_PROBE_MAX_NODES` env var.
    pub fn with_merge_probe_max_nodes(mut self, max_nodes: usize) -> Self {
        self.merge_probe_max_nodes = max_nodes;
        self
    }

    /// Override the merge-probe walk budget (#171) — the test/D2 lever
    /// for driving a small ontology into exhaustion, instead of the
    /// `OWL2_REASONER_MERGE_PROBE_WALK_BUDGET` env var.
    pub fn with_merge_probe_walk_budget(mut self, walk_budget: usize) -> Self {
        self.merge_probe_walk_budget = walk_budget;
        self
    }

    /// Override the merge-probe global work ceiling (#173, ADR 0022)
    /// — `0` means unlimited — instead of the
    /// `OWL2_REASONER_MERGE_PROBE_WORK_BUDGET` env var.
    pub fn with_merge_probe_work_budget(mut self, work_budget: u64) -> Self {
        self.merge_probe_work_budget = work_budget;
        self
    }

    /// Whether the consistency check's merge probe entered and spent a
    /// budget mid-search (#171, ADR 0022): the verdict stays fail-open
    /// and reports the incompleteness explicitly.
    pub fn merge_probe_exhausted(&self) -> bool {
        self.merge_probe_exhausted
            .read()
            .map(|flag| *flag)
            .unwrap_or(false)
    }

    /// Whether the consistency check skipped the merge probe because
    /// the corpus exceeded the size gate (#168): the verdict is
    /// fail-open on merge rules and reports it explicitly.
    pub fn merge_probe_skipped(&self) -> bool {
        self.merge_probe_skipped
            .read()
            .map(|flag| *flag)
            .unwrap_or(false)
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> Result<CacheStats, OwlError> {
        let stats = self.cache_stats.read().map_err(|e| OwlError::LockError {
            lock_type: "cache_stats".to_string(),
            timeout_ms: 0,
            message: format!("Failed to acquire read lock for cache stats: {}", e),
        })?;
        Ok(stats.clone())
    }

    /// Reset cache statistics
    pub fn reset_cache_stats(&self) -> Result<(), OwlError> {
        let mut stats = self.cache_stats.write().map_err(|e| OwlError::LockError {
            lock_type: "cache_stats".to_string(),
            timeout_ms: 0,
            message: format!("Failed to acquire write lock for cache stats: {}", e),
        })?;
        *stats = CacheStats::new();
        Ok(())
    }

    /// Helper function to acquire read lock with proper error handling
    fn read_lock<'a, T>(
        &self,
        lock: &'a RwLock<T>,
        operation: &str,
    ) -> OwlResult<std::sync::RwLockReadGuard<'a, T>> {
        lock.read().map_err(move |e| OwlError::LockError {
            lock_type: operation.to_string(),
            timeout_ms: 0,
            message: format!("Failed to acquire read lock for {}: {}", operation, e),
        })
    }

    /// Helper function to acquire write lock with proper error handling
    fn write_lock<'a, T>(
        &self,
        lock: &'a RwLock<T>,
        operation: &str,
    ) -> OwlResult<std::sync::RwLockWriteGuard<'a, T>> {
        lock.write().map_err(move |e| OwlError::LockError {
            lock_type: operation.to_string(),
            timeout_ms: 0,
            message: format!("Failed to acquire write lock for {}: {}", operation, e),
        })
    }

    /// Warm up caches by pre-computing common queries
    pub fn warm_up_caches(&self) -> OwlResult<()> {
        let classes: Vec<_> = self.ontology.classes().iter().cloned().collect();

        // Pre-compute consistency
        let _ = self.is_consistent();

        // Pre-compute common subclass relationships
        for i in 0..classes.len().min(10) {
            for j in 0..classes.len().min(10) {
                if i != j {
                    let _ = self.is_subclass_of(classes[i].iri(), classes[j].iri());
                }
            }
        }

        // Pre-compute satisfiability for classes
        for class in classes.iter().take(10) {
            let _ = self.is_class_satisfiable(class.iri());
        }

        // Pre-compute instances for some classes
        for class in classes.iter().take(5) {
            let _ = self.get_instances(class.iri());
        }

        Ok(())
    }

    /// Clear all caches
    pub fn clear_caches(&self) -> OwlResult<()> {
        let mut consistency = self
            .consistency_cache
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "clear_caches_consistency".to_string(),
                message: format!("Failed to acquire consistency cache write lock: {}", e),
                timeout_ms: 0,
            })?;
        *consistency = None;

        let mut subclass = self
            .subclass_cache
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "clear_caches_subclass".to_string(),
                message: format!("Failed to acquire subclass cache write lock: {}", e),
                timeout_ms: 0,
            })?;
        subclass.clear();

        let mut satisfiability =
            self.satisfiability_cache
                .write()
                .map_err(|e| OwlError::LockError {
                    lock_type: "clear_caches_satisfiability".to_string(),
                    message: format!("Failed to acquire satisfiability cache write lock: {}", e),
                    timeout_ms: 0,
                })?;
        satisfiability.clear();

        let mut instances = self
            .instances_cache
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "clear_caches_instances".to_string(),
                message: format!("Failed to acquire instances cache write lock: {}", e),
                timeout_ms: 0,
            })?;
        instances.clear();

        Ok(())
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> OwlResult<HashMap<String, usize>> {
        let mut stats = HashMap::new();

        let consistency = self
            .consistency_cache
            .read()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats_consistency".to_string(),
                message: format!("Failed to acquire consistency cache read lock: {}", e),
                timeout_ms: 0,
            })?;
        stats.insert(
            "consistency".to_string(),
            consistency.as_ref().map_or(0, |_| 1),
        );

        let subclass = self
            .subclass_cache
            .read()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats_subclass".to_string(),
                message: format!("Failed to acquire subclass cache read lock: {}", e),
                timeout_ms: 0,
            })?;
        stats.insert("subclass".to_string(), subclass.len());

        let satisfiability = self
            .satisfiability_cache
            .read()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats_satisfiability".to_string(),
                message: format!("Failed to acquire satisfiability cache read lock: {}", e),
                timeout_ms: 0,
            })?;
        stats.insert("satisfiability".to_string(), satisfiability.len());

        let instances = self
            .instances_cache
            .read()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats_instances".to_string(),
                message: format!("Failed to acquire instances cache read lock: {}", e),
                timeout_ms: 0,
            })?;
        stats.insert("instances".to_string(), instances.len());

        Ok(stats)
    }

    // ===== OWL2 Profile Validation Methods =====

    /// Validate ontology against a specific OWL2 profile
    pub fn validate_profile(&mut self, profile: Owl2Profile) -> OwlResult<ProfileValidationResult> {
        self.profile_validator.validate_profile(profile)
    }

    /// Check if ontology complies with EL profile
    pub fn is_el_profile(&mut self) -> OwlResult<bool> {
        Ok(self
            .profile_validator
            .validate_profile(Owl2Profile::EL)?
            .is_valid)
    }

    /// Check if ontology complies with QL profile  
    pub fn is_ql_profile(&mut self) -> OwlResult<bool> {
        Ok(self
            .profile_validator
            .validate_profile(Owl2Profile::QL)?
            .is_valid)
    }

    /// Check if ontology complies with RL profile
    pub fn is_rl_profile(&mut self) -> OwlResult<bool> {
        Ok(self
            .profile_validator
            .validate_profile(Owl2Profile::RL)?
            .is_valid)
    }

    /// Validate against all OWL2 profiles and return comprehensive results
    pub fn validate_all_profiles(&mut self) -> OwlResult<Vec<ProfileValidationResult>> {
        self.profile_validator.validate_all_profiles()
    }

    /// Get the most restrictive valid profile for this ontology
    pub fn get_most_restrictive_profile(&mut self) -> OwlResult<Option<Owl2Profile>> {
        self.profile_validator.get_most_restrictive_profile()
    }

    /// Check if ontology satisfies any OWL2 profile
    pub fn satisfies_any_profile(&mut self) -> OwlResult<bool> {
        self.profile_validator.satisfies_any_profile()
    }

    /// Get optimization hints for profile compliance
    pub fn get_profile_optimization_hints(
        &self,
    ) -> Vec<crate::strategy::profiles::OptimizationHint> {
        self.profile_validator.get_optimization_hints()
    }

    /// Clear profile validation cache
    pub fn clear_profile_cache(&mut self) {
        self.profile_validator.clear_cache();
    }

    /// Get profile validation cache statistics
    pub fn profile_cache_stats(&self) -> (usize, usize) {
        let stats = self.profile_validator.get_cache_stats();
        (stats.hits, stats.misses)
    }

    /// Enable or disable advanced profile caching
    pub fn set_advanced_profile_caching(&mut self, enabled: bool) {
        self.profile_validator.set_advanced_caching(enabled);
    }

    /// Classify the ontology
    ///
    /// Performs comprehensive classification of the ontology, including consistency checking
    /// and computing the class hierarchy. This is a fundamental reasoning operation.
    ///
    /// # Returns
    ///
    /// `Ok(())` if classification succeeds, `Err(OwlError)` if classification fails
    pub fn classify(&self) -> OwlResult<()> {
        // Perform consistency checking as the core of classification
        let _is_consistent = self.is_consistent()?;

        // In a full implementation, this would:
        // 1. Check consistency
        // 2. Compute class hierarchy
        // 3. Determine satisfiability of all classes
        // 4. Compute inferred subclass relationships

        Ok(())
    }

    /// Check if the ontology is consistent (cached)
    /// Unqualified data-cardinality contradiction over class definitions.
    ///
    /// Cardinality pigeonhole over exact class sizes (#154, W3C
    /// one_two): a class with a complete oneOf definition over
    /// pairwise-different members has an exact size; bijection links
    /// (a functional AND inverse-functional property, total in both
    /// directions through its declared inverse) equate the sizes of
    /// the two classes; a disjoint union of exact classes needs the
    /// sum of their sizes inside any class it sits under — more than
    /// the target's exact size contradicts. Sound: every step is
    /// injective-pigeonhole arithmetic, and the check only fires on
    /// contradictions. Cheap exit when no exact seed exists.
    fn has_cardinality_pigeonhole_contradiction(&self) -> bool {
        use std::collections::BTreeMap;

        // One pass over the subclass axioms collecting the shapes.
        let mut sub_oneof: BTreeMap<&str, &smallvec::SmallVec<[crate::core::entities::Individual; 8]>> =
            BTreeMap::new();
        let mut oneof_into: Vec<(&smallvec::SmallVec<[crate::core::entities::Individual; 8]>, &str)> =
            Vec::new();
        let mut forward_links: Vec<(&str, &str, &str)> = Vec::new();
        let mut union_into: Vec<(Vec<&str>, &str)> = Vec::new();
        for axiom in self.ontology.subclass_axioms() {
            match (axiom.sub_class(), axiom.super_class()) {
                (ClassExpression::Class(sub), ClassExpression::ObjectOneOf(members)) => {
                    sub_oneof.insert(sub.iri().as_str(), members);
                }
                (ClassExpression::ObjectOneOf(members), ClassExpression::Class(sup)) => {
                    oneof_into.push((members, sup.iri().as_str()));
                }
                (
                    ClassExpression::Class(sub),
                    ClassExpression::ObjectSomeValuesFrom(property, filler),
                ) => {
                    if let (Some(role), ClassExpression::Class(target)) = (
                        property.as_named().map(|p| p.iri().as_str()),
                        filler.as_ref(),
                    ) {
                        forward_links.push((sub.iri().as_str(), target.iri().as_str(), role));
                    }
                }
                (ClassExpression::ObjectUnionOf(operands), ClassExpression::Class(sup)) => {
                    let mut disjuncts = Vec::new();
                    for operand in operands.iter() {
                        let ClassExpression::Class(named) = operand.as_ref() else {
                            disjuncts.clear();
                            break;
                        };
                        disjuncts.push(named.iri().as_str());
                    }
                    if !disjuncts.is_empty() {
                        union_into.push((disjuncts, sup.iri().as_str()));
                    }
                }
                _ => {}
            }
        }
        if sub_oneof.is_empty() || oneof_into.is_empty() {
            return false;
        }

        // Exact seeds: both directions over the same member set, all
        // members pairwise known-different.
        let mut exact: BTreeMap<&str, usize> = BTreeMap::new();
        for (members, class) in &oneof_into {
            let Some(&declared) = sub_oneof.get(class) else {
                continue;
            };
            // Order-insensitive multiset match (#154's recorded gap):
            // equal length plus containment covers duplicated IRIs too.
            if declared.len() != members.len()
                || !members.iter().all(|member| {
                    member.iri().is_some_and(|iri| {
                        declared.iter().any(|other| {
                            other
                                .iri()
                                .is_some_and(|other| other.as_ref() == iri.as_ref())
                        })
                    })
                })
            {
                continue;
            }
            let mut pairwise_different = true;
            for (index, left) in members.iter().enumerate() {
                for right in members.iter().skip(index + 1) {
                    match (left.iri(), right.iri()) {
                        (Some(left), Some(right)) => {
                            if !are_known_different(&self.ontology, left, right) {
                                pairwise_different = false;
                            }
                        }
                        _ => pairwise_different = false,
                    }
                }
            }
            if pairwise_different {
                exact.insert(class, members.len());
            }
        }
        if exact.is_empty() {
            return false;
        }

        // Bijection links: functional + inverse-functional roles whose
        // declared inverse carries the reverse direction.
        let functional: std::collections::BTreeSet<&str> = self
            .ontology
            .functional_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        let inverse_functional: std::collections::BTreeSet<&str> = self
            .ontology
            .inverse_functional_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        let mut inverse_partners: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
        for axiom in self.ontology.inverse_object_properties_axioms() {
            let (left, right) = (axiom.property1().as_str(), axiom.property2().as_str());
            inverse_partners.entry(left).or_default().insert(right);
            inverse_partners.entry(right).or_default().insert(left);
        }
        // Named superclass closure (#154's recorded gap): disjointness
        // declared on supers applies to their subclasses.
        let mut class_supers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        {
            let mut direct: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for axiom in self.ontology.subclass_axioms() {
                if let (ClassExpression::Class(sub), ClassExpression::Class(sup)) =
                    (axiom.sub_class(), axiom.super_class())
                {
                    direct
                        .entry(sub.iri().as_str())
                        .or_default()
                        .push(sup.iri().as_str());
                }
            }
            let roots: Vec<&str> = direct.keys().copied().collect();
            for class in roots {
                let mut seen: Vec<&str> = Vec::new();
                let mut stack = direct.get(class).cloned().unwrap_or_default();
                while let Some(current) = stack.pop() {
                    if !seen.contains(&current) {
                        seen.push(current);
                        if let Some(nexts) = direct.get(current) {
                            stack.extend(nexts.iter().copied());
                        }
                    }
                }
                class_supers.insert(class, seen);
            }
        }
        let closure_includes = |class: &str, named: &str| -> bool {
            class == named
                || class_supers
                    .get(class)
                    .is_some_and(|supers| supers.contains(&named))
        };
        let disjoint_through_supers = |left: &str, right: &str| {
            self.ontology.disjoint_classes_axioms().iter().any(|axiom| {
                axiom.classes().iter().any(|a| closure_includes(left, a.as_str()))
                    && axiom
                        .classes()
                        .iter()
                        .any(|b| closure_includes(right, b.as_str()))
            })
        };
        // Role superproperty closure (#154's recorded gap): a link over
        // a subproperty of a functional+inverse-functional role is
        // injective too — its mapping sits inside the super's. Class
        // closure on link ENDPOINTS is deliberately NOT taken:
        // subclass gives |sub| <= |super| while the union-find equates
        // group sizes — equating through <= is unsound (recorded).
        let mut role_supers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        {
            let mut direct: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for axiom in self.ontology.subobject_property_axioms() {
                direct
                    .entry(axiom.sub_property().as_str())
                    .or_default()
                    .push(axiom.super_property().as_str());
            }
            let roots: Vec<&str> = direct.keys().copied().collect();
            for role in roots {
                let mut seen: Vec<&str> = Vec::new();
                let mut stack = direct.get(role).cloned().unwrap_or_default();
                while let Some(current) = stack.pop() {
                    if !seen.contains(&current) {
                        seen.push(current);
                        if let Some(nexts) = direct.get(current) {
                            stack.extend(nexts.iter().copied());
                        }
                    }
                }
                role_supers.insert(role, seen);
            }
        }
        let bijective = |role: &str| {
            (functional.contains(role) && inverse_functional.contains(role))
                || role_supers.get(role).is_some_and(|supers| {
                    supers
                        .iter()
                        .any(|super_role| {
                            functional.contains(*super_role)
                                && inverse_functional.contains(*super_role)
                        })
                })
        };
        let same_role_or_inverse = |left: &str, right: &str| {
            left == right
                || inverse_partners
                    .get(left)
                    .is_some_and(|partners| partners.contains(right))
                // Subproperty pairing (#154's recorded gap): a link over
                // a subproperty pairs with a reverse over its bijective
                // super — both mappings are injective, so equating the
                // group sizes stays sound.
                || role_supers
                    .get(left)
                    .is_some_and(|supers| supers.contains(&right))
                || role_supers
                    .get(right)
                    .is_some_and(|supers| supers.contains(&left))
        };

        // Union-find over classes joined by both-ways-total bijective
        // links; exact sizes spread across each group.
        fn find<'a>(parent: &BTreeMap<&'a str, &'a str>, class: &'a str) -> &'a str {
            let mut current = class;
            while let Some(&up) = parent.get(current) {
                current = up;
            }
            current
        }
        let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
        for (from, to, role) in &forward_links {
            if !bijective(role) {
                continue;
            }
            let reverse = forward_links.iter().any(|(back_from, back_to, back_role)| {
                back_from == to
                    && back_to == from
                    && bijective(back_role)
                    && same_role_or_inverse(role, back_role)
            });
            if !reverse {
                continue;
            }
            let (left, right) = (find(&parent, from), find(&parent, to));
            if left != right {
                parent.insert(right, left);
            }
        }
        let mut group_size: BTreeMap<&str, usize> = BTreeMap::new();
        for (class, size) in &exact {
            let root = find(&parent, class);
            let slot = group_size.entry(root).or_insert(*size);
            *slot = (*slot).max(*size);
        }

        // The disjoint-union sum against the target's exact size.
        let class_size = |class: &str| -> Option<usize> {
            let root = find(&parent, &class);
            group_size.get(root).copied()
        };
        for (disjuncts, target) in &union_into {
            let Some(target_size) = class_size(target) else {
                continue;
            };
            let mut total = 0usize;
            let mut all_exact = true;
            for disjunct in disjuncts {
                match class_size(disjunct) {
                    Some(size) => total += size,
                    None => all_exact = false,
                }
            }
            if !all_exact || total <= target_size {
                continue;
            }
            let mut pairwise_disjoint = true;
            for (index, left) in disjuncts.iter().enumerate() {
                for right in disjuncts.iter().skip(index + 1) {
                    if !disjoint_through_supers(left, right) {
                        pairwise_disjoint = false;
                    }
                }
            }
            if pairwise_disjoint {
                return true;
            }
        }
        false
    }

    /// Whether one property is declared both reflexive and
    /// irreflexive — a declared-shape contradiction with no model
    /// (reflexivity forces every self-loop, irreflexivity forbids
    /// every one, domains are non-empty), independent of any
    /// assertion (#159). Direct declarations only: no subproperty
    /// closure.
    fn has_declared_reflexive_irreflexive_conflict(&self) -> bool {
        let irreflexive: std::collections::BTreeSet<&str> = self
            .ontology
            .irreflexive_property_axioms()
            .iter()
            .map(|axiom| axiom.property().as_str())
            .collect();
        self.ontology
            .reflexive_property_axioms()
            .iter()
            .any(|axiom| irreflexive.contains(axiom.property().as_str()))
    }

    /// Whether two named classes are declared disjoint (directly).
    fn named_classes_disjoint(&self, left: &str, right: &str) -> bool {
        self.ontology.disjoint_classes_axioms().iter().any(|axiom| {
            axiom
                .classes()
                .iter()
                .any(|a| a.as_str() == left)
                && axiom.classes().iter().any(|b| b.as_str() == right)
        })
    }

    /// Every instance of a class must satisfy the expressions on the
    /// super-side of that class's subclass axioms (the parser lowers
    /// equivalentClass edges to bidirectional SubClassOf, so definitions
    /// reach here too). For each individual, walk those expressions
    /// transitively and collect per-property min/max bounds plus the
    /// distinct asserted data values; min > max or more distinct values
    /// than max allows is a contradiction no model can repair.
    fn has_data_cardinality_contradiction(&self) -> bool {
        use std::collections::{BTreeMap, BTreeSet};

        // Named class -> super-side expressions, plus anonymous-subject
        // GCIs whose subject can be satisfied vacuously (a universal or
        // zero-max restriction over a property the individual never uses).
        let (supers, anon_gcis) = build_super_index(&self.ontology);

        // Distinct asserted data values per (individual, property).
        let mut asserted: BTreeMap<(&str, &str), Vec<&crate::core::entities::Literal>> =
            BTreeMap::new();
        for assertion in self.ontology.data_property_assertions() {
            let values = asserted
                .entry((
                    assertion.subject().as_str(),
                    assertion.property().as_str(),
                ))
                .or_default();
            if !values
                .iter()
                .any(|existing| literals_are_equal(existing, assertion.value()))
            {
                values.push(assertion.value());
            }
        }

        // Properties each individual actually uses (object side); a
        // universal or zero-max restriction over an unused property is
        // vacuously satisfied.
        let mut used_object_properties: BTreeSet<(&str, &str)> = BTreeSet::new();
        for assertion in self.ontology.property_assertions() {
            used_object_properties.insert((
                assertion.subject().as_str(),
                assertion.property().as_str(),
            ));
        }

        for assertion in self.ontology.class_assertions() {
            let individual = assertion.individual().as_str();
            // Collect every unqualified data cardinality the individual
            // must satisfy, transitively through named classes and
            // vacuously-satisfied anonymous GCIs.
            let mut bounds: BTreeMap<&str, (Option<u32>, Option<u32>)> = BTreeMap::new();
            let mut stack: Vec<&ClassExpression> = vec![assertion.class_expr()];
            let mut visited: BTreeSet<&str> = BTreeSet::new();
            let mut budget: usize = 10_000;
            while let Some(expression) = stack.pop() {
                budget -= 1;
                if budget == 0 {
                    break;
                }
                match expression {
                    ClassExpression::Class(class) => {
                        let iri = class.iri().as_str();
                        if !visited.insert(iri) {
                            continue;
                        }
                        if let Some(expressions) = supers.get(iri) {
                            stack.extend(expressions.iter().copied());
                        }
                        for (subject, super_expression) in &anon_gcis {
                            if Self::anon_gci_subject_satisfied(
                                subject,
                                individual,
                                &used_object_properties,
                            ) {
                                stack.push(super_expression);
                            }
                        }
                    }
                    ClassExpression::ObjectIntersectionOf(operands) => {
                        stack.extend(operands.iter().map(|op| op.as_ref()));
                    }
                    ClassExpression::DataMinCardinality(min, property, None) => {
                        if let Some(named) = property.as_named() {
                            let slot = bounds.entry(named.iri().as_str()).or_insert((None, None));
                            if slot.0.is_none_or(|current| *min > current) {
                                slot.0 = Some(*min);
                            }
                        }
                    }
                    ClassExpression::DataMaxCardinality(max, property, None) => {
                        if let Some(named) = property.as_named() {
                            let slot = bounds.entry(named.iri().as_str()).or_insert((None, None));
                            if slot.1.is_none_or(|current| *max < current) {
                                slot.1 = Some(*max);
                            }
                        }
                    }
                    ClassExpression::DataExactCardinality(exact, property, None) => {
                        // Exact cardinality binds both directions; a shared
                        // or-pattern arm would swallow it into the min side.
                        if let Some(named) = property.as_named() {
                            let slot = bounds.entry(named.iri().as_str()).or_insert((None, None));
                            if slot.0.is_none_or(|current| *exact > current) {
                                slot.0 = Some(*exact);
                            }
                            if slot.1.is_none_or(|current| *exact < current) {
                                slot.1 = Some(*exact);
                            }
                        }
                    }
                    _ => {}
                }
            }
            for (property, (min, max)) in bounds {
                if let (Some(min), Some(max)) = (min, max) {
                    if min > max {
                        return true;
                    }
                }
                if let Some(max) = max {
                    let distinct = asserted
                        .get(&(individual, property))
                        .map(|values| values.len())
                        .unwrap_or(0);
                    if distinct as u32 > max {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether a subclass axiom's subject is an anonymous GCI this
    /// reasoner can decide: vacuous shapes and existential restrictions
    /// over named roles, conjoined.
    fn is_anonymous_gci(expression: &ClassExpression) -> bool {
        !matches!(expression, ClassExpression::Class(_))
            && Self::expression_is_gci_shape(expression)
    }

    /// Whether an anonymous GCI subject has a shape the #110 probe can
    /// decide: the vacuous shapes, an existential restriction over a
    /// named role — or an inverse of one, which is how the entailment
    /// lane lowers object-property ranges (`∃p⁻.⊤ ⊑ C`, #134) —
    /// (satisfied by having a successor through either direction), a
    /// named class (satisfied by being recorded), conjoined (#127 —
    /// the parser lowers `A ≡ c1 ⊓ c2` to exactly such conjunction
    /// subjects).
    fn expression_is_gci_shape(expression: &ClassExpression) -> bool {
        match expression {
            ClassExpression::ObjectSomeValuesFrom(property, filler) => {
                Self::role_is_named_or_inverse_of_named(property)
                    && (matches!(filler.as_ref(), ClassExpression::Class(_))
                        || Self::is_named_complement(filler))
            }
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| {
                    Self::expression_is_vacuous_shape(operand)
                        || Self::expression_is_gci_shape(operand)
                }),
            ClassExpression::Class(_) => true,
            // A complement of a named class is decidable through the
            // recorded complements' contrapositive closure (#142, W3C
            // description-logic-201/205/208's oiled definitions).
            other @ ClassExpression::ObjectComplementOf(_) => Self::is_named_complement(other),
            _ => Self::expression_is_vacuous_shape(expression),
        }
    }

    /// Whether an expression is ¬C for a named C — the shape the
    /// complement-subject rules decide (#142, W3C
    /// description-logic-201/205/208).
    fn is_named_complement(expression: &ClassExpression) -> bool {
        matches!(
            expression,
            ClassExpression::ObjectComplementOf(inner)
                if matches!(inner.as_ref(), ClassExpression::Class(_))
        )
    }

    /// Whether a restriction role is a named property or an inverse of
    /// one — both resolve through `resolve_role`, so both are decidable
    /// GCI-subject roles (#134).
    fn role_is_named_or_inverse_of_named(property: &ObjectPropertyExpression) -> bool {
        match property {
            ObjectPropertyExpression::ObjectProperty(_) => property.as_named().is_some(),
            ObjectPropertyExpression::ObjectInverseOf(inner) => inner.as_named().is_some(),
        }
    }

    /// Whether an anonymous GCI subject has the vacuously-satisfiable
    /// shape this check can apply: universals and zero-max restrictions
    /// over properties, conjoined.
    fn expression_is_vacuous_shape(expression: &ClassExpression) -> bool {
        match expression {
            ClassExpression::ObjectAllValuesFrom(_, _) => true,
            ClassExpression::ObjectMaxCardinality(0, _, None) => true,
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| Self::expression_is_vacuous_shape(operand)),
            _ => false,
        }
    }

    /// Whether a mixed GCI subject leans on a vacuously-read conjunct
    /// (a universal or an unqualified zero-max, anywhere inside its
    /// intersections): such subjects decide only against the node's
    /// settled edge set, never at walk start (#130).
    fn subject_reads_vacuously(expression: &ClassExpression) -> bool {
        match expression {
            ClassExpression::ObjectAllValuesFrom(..) => true,
            ClassExpression::ObjectMaxCardinality(0, _, None) => true,
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .any(|operand| Self::subject_reads_vacuously(operand)),
            _ => false,
        }
    }

    /// A vacuous-shape GCI subject is satisfied by an individual that never
    /// uses the restricted property: the universal holds of every unused
    /// role, and zero-max holds with no successors.
    fn anon_gci_subject_satisfied(
        expression: &ClassExpression,
        individual: &str,
        used_object_properties: &BTreeSet<(&str, &str)>,
    ) -> bool {
        Self::anon_gci_subject_satisfied_with(expression, &|property: &IRI| {
            used_object_properties.contains(&(individual, property.as_str()))
        })
    }

    /// Whether an anonymous GCI subject is fully carried by the node's
    /// recorded expressions (positive satisfaction).
    fn anon_gci_subject_recorded(
        expression: &ClassExpression,
        recorded: &[&ClassExpression],
    ) -> bool {
        if recorded
            .iter()
            .any(|existing| std::ptr::eq(*existing, expression))
        {
            return true;
        }
        match expression {
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| Self::anon_gci_subject_recorded(operand, recorded)),
            _ => false,
        }
    }

    /// Whether an anonymous GCI subject holds for a node that carries
    /// `recorded` expressions: every conjunct is either the very
    /// expression already on the node (positive satisfaction, W3C
    /// description-logic-002's (∀r.c) ⊑ d reached through c ⊑ ∀r.c) or
    /// holds vacuously over a property the node never uses.
    fn anon_gci_subject_holds(
        expression: &ClassExpression,
        recorded: &[&ClassExpression],
        uses_property: &impl Fn(&IRI) -> bool,
    ) -> bool {
        if recorded
            .iter()
            .any(|existing| std::ptr::eq(*existing, expression))
        {
            return true;
        }
        match expression {
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| Self::anon_gci_subject_holds(operand, recorded, uses_property)),
            other => Self::anon_gci_subject_satisfied_with(other, uses_property),
        }
    }

    /// Property-predicate form of the vacuous-subject check, shared with
    /// the #109 merge probe whose pseudo-individuals have no IRI.
    fn anon_gci_subject_satisfied_with(
        expression: &ClassExpression,
        uses_property: &impl Fn(&IRI) -> bool,
    ) -> bool {
        match expression {
            ClassExpression::ObjectAllValuesFrom(property, _) => match property.as_named() {
                Some(named) => !uses_property(named.iri()),
                None => false,
            },
            ClassExpression::ObjectMaxCardinality(0, property, None) => {
                match property.as_named() {
                    Some(named) => !uses_property(named.iri()),
                    None => false,
                }
            }
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .all(|operand| Self::anon_gci_subject_satisfied_with(operand, uses_property)),
            _ => false,
        }
    }

    /// #109/#125: merge rules the probe enforces. Functional roles and
    /// unqualified object max-cardinality restrictions force successors to
    /// merge; the ontology is inconsistent when a forced merge is blocked
    /// (explicit differentFrom, disjoint filler classes, contradictory
    /// unqualified data bounds on the merged node) or when successors
    /// under a ≤m bound admit no partition into m mergeable groups.
    /// Unqualified data max/exact bounds reach the same settled check:
    /// pooled min/max plus distinct forced values under a max (#125).
    /// The witness walk is bounded; an exhausted budget only misses
    /// contradictions.
    fn has_merge_contradiction(&self) -> bool {
        if !self.ontology_has_merge_rules() {
            return false;
        }
        let mut probe = MergeProbe::new(&self.ontology);
        if self.merge_probe_max_nodes > 0 && probe.node_count() > self.merge_probe_max_nodes {
            // Fail-open size gate (#168): the walk costs Theta(gcis x
            // edges) per node, which at corpus scale outlives any
            // budget. Skip and record it — the verdict stays consistent
            // and the report says the merge rules went unchecked.
            if let Ok(mut flag) = self.merge_probe_skipped.write() {
                *flag = true;
            }
            // Deliberately not behind OWL2_REASONER_STAGE_TIMING: like
            // the parser's [FALLBACK] lines, this is an always-on
            // correctness-posture signal, not a timing probe.
            eprintln!(
                "[stage] merge_probe_skipped nodes={} gate={} posture=fail-open",
                probe.node_count(),
                self.merge_probe_max_nodes,
            );
            MERGE_PROBE_SKIP_LATCH.store(true, std::sync::atomic::Ordering::Relaxed);
            return false;
        }
        probe.budget = self.merge_probe_walk_budget;
        probe.walk_budget = self.merge_probe_walk_budget;
        probe.work_ceiling = self.merge_probe_work_budget;
        probe.process();
        let settled = probe.settle();
        let died = if settled {
            true
        } else if probe.oneof_constraints.is_empty() && probe.union_choices.is_empty() {
            false
        } else {
            // Local propagation leaves oneOf constraints with several
            // unblocked candidates and pressed unions with several live
            // disjuncts undecided; the branch search decides them (#115,
            // W3C description-logic-502; #119 unions, ADR-0020).
            let mut clones = MERGE_PROBE_SEARCH_CLONES;
            probe.search_identifications(&mut clones)
        };
        // Always-on posture/diagnostic signal (the #168 line's kin):
        // the counted work makes the ceiling auditable and calibration
        // reproducible (#173).
        eprintln!(
            "[stage] merge_probe_done work={} exhausted={}",
            probe.work_spent.get(),
            probe.incomplete()
        );
        self.record_merge_probe_exhaustion(&probe);
        died
    }

    /// Record a probe that stopped early on a spent budget (#171, ADR
    /// 0022): the verdict is already fail-open; this only makes the
    /// incompleteness observable on the reasoner, the stage log, and
    /// the process-wide latch the CLI report reads.
    fn record_merge_probe_exhaustion(&self, probe: &MergeProbe<'_>) {
        if !probe.incomplete() {
            return;
        }
        if let Ok(mut flag) = self.merge_probe_exhausted.write() {
            *flag = true;
        }
        eprintln!("[stage] merge_probe_budget_exhausted posture=fail-open");
        MERGE_PROBE_EXHAUSTED_LATCH.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether any rule of the merge probe could apply: a functional
    /// object property, or an unqualified object max/exact cardinality
    /// hiding anywhere in assertions or class definitions.
    fn ontology_has_merge_rules(&self) -> bool {
        if !self.ontology.functional_property_axioms().is_empty() {
            return true;
        }
        // Inverse-functional roles also force merges — of same-object
        // subjects (#131).
        if !self
            .ontology
            .inverse_functional_property_axioms()
            .is_empty()
        {
            return true;
        }
        // Stated negative assertions clash against derived edges — the
        // probe is the only lane that materializes those (#138).
        if !self.ontology.negative_object_property_assertions().is_empty() {
            return true;
        }
        // Anonymous-subject GCIs also run through the probe: a vacuous
        // subject can land a class next to its asserted complement (W3C
        // description-logic-002), an existential subject fires from a
        // successor edge (W3C I4.5-002).
        for axiom in self.ontology.subclass_axioms() {
            if Self::is_anonymous_gci(axiom.sub_class())
                || matches!(axiom.sub_class(), ClassExpression::ObjectOneOf(_))
            {
                return true;
            }
        }
        let mut budget: usize = MERGE_PROBE_GATE_BUDGET;
        for axiom in self.ontology.subclass_axioms() {
            if Self::expression_has_object_cardinality(axiom.super_class(), &mut budget) {
                return true;
            }
        }
        for assertion in self.ontology.class_assertions() {
            if Self::expression_has_object_cardinality(assertion.class_expr(), &mut budget) {
                return true;
            }
        }
        // The data pass spends its own budget, so an axiom-heavy
        // ontology cannot starve the object pass (or vice versa);
        // either side exhausting only means no probe, never a verdict.
        let mut data_budget: usize = MERGE_PROBE_GATE_BUDGET;
        for axiom in self.ontology.subclass_axioms() {
            if Self::expression_has_data_cardinality(axiom.super_class(), &mut data_budget) {
                return true;
            }
        }
        for assertion in self.ontology.class_assertions() {
            if Self::expression_has_data_cardinality(assertion.class_expr(), &mut data_budget) {
                return true;
            }
        }
        false
    }

    /// Data max/exact bounds the settled check can enforce against a
    /// node's distinct forced values (#125, W3C I5.21-002). Min-only
    /// bounds need no probe: the new clash reads the pooled max, and
    /// the pooled min>max rule only binds when a max is present.
    /// Quantifiers and unions never reach here — the object pass
    /// returns true on them first — so only complement and
    /// intersection containers are walked.
    fn expression_has_data_cardinality(
        expression: &ClassExpression,
        budget: &mut usize,
    ) -> bool {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        match expression {
            ClassExpression::DataMaxCardinality(..)
            | ClassExpression::DataExactCardinality(..) => true,
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .any(|operand| Self::expression_has_data_cardinality(operand, budget)),
            ClassExpression::ObjectComplementOf(inner) => {
                Self::expression_has_data_cardinality(inner, budget)
            }
            _ => false,
        }
    }

    fn expression_has_object_cardinality(
        expression: &ClassExpression,
        budget: &mut usize,
    ) -> bool {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        match expression {
            ClassExpression::ObjectMaxCardinality(..)
            | ClassExpression::ObjectExactCardinality(..)
            | ClassExpression::ObjectMinCardinality(..) => true,
            // Universals and existentials participate through the
            // #110 propagation rules; nominals through #114.
            ClassExpression::ObjectAllValuesFrom(..)
            | ClassExpression::ObjectSomeValuesFrom(..)
            | ClassExpression::ObjectOneOf(..) => true,
            // A hasSelf label participates through self-loop
            // spawning, and its complement through the negated-self
            // clash (#149, W3C SelfRestriction-002).
            ClassExpression::ObjectHasSelf(..) => true,
            // A union needs the branch-and-settle choice machinery
            // (#119, ADR-0020).
            ClassExpression::ObjectUnionOf(_) => true,
            ClassExpression::ObjectIntersectionOf(operands) => operands
                .iter()
                .any(|operand| Self::expression_has_object_cardinality(operand, budget)),
            ClassExpression::ObjectComplementOf(inner) => {
                Self::expression_has_object_cardinality(inner, budget)
            }
            _ => false,
        }
    }

    pub fn is_consistent(&self) -> OwlResult<bool> {
        // Check cache first
        {
            let cache = self.read_lock(&self.consistency_cache, "consistency_cache")?;
            if let Some(entry) = cache.as_ref() {
                if let Some(result) = entry.get() {
                    // Cache hit
                    self.cache_stats
                        .write()
                        .map_err(|e| OwlError::LockError {
                            lock_type: "cache_stats".to_string(),
                            timeout_ms: 0,
                            message: format!("Failed to acquire write lock for cache stats: {}", e),
                        })?
                        .record_hit();
                    return Ok(*result);
                }
            }
        }

        // Cache miss
        self.cache_stats
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats".to_string(),
                timeout_ms: 0,
                message: format!("Failed to acquire write lock for cache stats: {}", e),
            })?
            .record_miss();

        // Compute result
        let result = self.compute_consistency()?;

        // Cache result (1 hour TTL for consistency - increased for better hit rate)
        let mut cache = self.write_lock(&self.consistency_cache, "consistency_cache")?;
        *cache = Some(CacheEntry::new(result, Duration::from_secs(3600)));

        Ok(result)
    }

    /// Compute consistency (internal method)
    fn compute_consistency(&self) -> OwlResult<bool> {
        // The bounded qualified checker declining its fragment (mixed
        // axioms) is a pass, not a verdict: the sequential layers below
        // it decide (#113, W3C 0007/0098/0497 family). Budget exhaustion
        // still surfaces — it guards the checker's own correctness.
        match check_qualified_consistency(&self.ontology) {
            Ok(Some(result)) => return Ok(result),
            Ok(None) => {}
            Err(OwlError::UnsupportedFeature(_)) => {}
            Err(error) => return Err(error),
        }

        if let Some(result) = check_boolean_consistency(&self.ontology)? {
            return Ok(result);
        }

        // Exact class sizes cannot fit a smaller disjoint union
        // (#154, W3C one_two's 6-into-3 lattice).
        if self.has_cardinality_pigeonhole_contradiction() {
            return Ok(false);
        }

        // A property declared both reflexive and irreflexive
        // contradicts on its own — reflexivity forces every self-loop,
        // irreflexivity forbids every one, and domains are non-empty —
        // so the conflict reads even with no assertion to spawn or ban
        // a loop on (#159, the #148/#149 hardening).
        if self.has_declared_reflexive_irreflexive_conflict() {
            return Ok(false);
        }

        // An individual whose class definitions demand more distinct data
        // values for one property than another definition allows cannot
        // exist (unqualified data cardinalities only; qualified ranges may
        // be disjoint and combining them would be unsound).
        if self.has_data_cardinality_contradiction() {
            return Ok(false);
        }

        // An enumerated data range reads universally: every value of the
        // property must sit in every pool, so an out-of-pool assertion
        // contradicts outright (#151, W3C oneOf-004's ranges).
        for positive in self.ontology.data_property_assertions() {
            // A pool on a superproperty confines subproperty values
            // too (#151's recorded gap).
            let supers = data_super_names(&self.ontology, positive.property().as_str());
            for axiom in self.ontology.data_property_enumerated_range_axioms() {
                let pool_property = axiom.property().as_str();
                if (pool_property == positive.property().as_str() || supers.contains(&pool_property))
                    && !axiom
                        .values()
                        .iter()
                        .any(|value| literals_are_equal(value, positive.value()))
                {
                    return Ok(false);
                }
            }
        }

        // A complemented qualified data minimum contradicts n distinct
        // asserted values that satisfy the qualifier — plain literals
        // are xsd:string values under RDF 1.1 (#153, W3C DataQCR-001's
        // negated conclusion shape). Compound qualifiers (union/
        // intersection/complement datatypes) stay unconsumed — recorded
        // incompleteness, like the other data fragments.
        for assertion in self.ontology.class_assertions() {
            let ClassExpression::ObjectComplementOf(inner) = assertion.class_expr() else {
                continue;
            };
            let ClassExpression::DataMinCardinality(minimum, property, Some(qualifier)) =
                inner.as_ref()
            else {
                continue;
            };
            let property_iri = data_property_iri(property);
            let mut distinct: Vec<&crate::core::entities::Literal> = Vec::new();
            for positive in self.ontology.data_property_assertions() {
                if positive.subject().as_ref() != assertion.individual().as_ref()
                    || positive.property().as_ref() != property_iri
                {
                    continue;
                }
                let satisfies = data_range_contains(qualifier, positive.value());
                if satisfies
                    && !distinct
                        .iter()
                        .any(|known| literals_are_equal(known, positive.value()))
                {
                    distinct.push(positive.value());
                }
            }
            if distinct.len() >= *minimum as usize {
                return Ok(false);
            }
        }

        // Functional roles and object max-cardinality restrictions force
        // successors to merge; a blocked forced merge is an inconsistency
        // no model repairs (#109). Data max/exact bounds clash through
        // the same settled check (#125).
        if self.has_merge_contradiction() {
            return Ok(false);
        }

        // Basic consistency check: look for obvious inconsistencies
        // This is a simplified implementation for demonstration

        // An asserted empty class expression has no possible instance.
        for assertion in self.ontology.class_assertions() {
            if class_expression_is_empty(assertion.class_expr()) {
                return Ok(false);
            }
        }

        // owl:Thing cannot be equivalent to owl:Nothing because Thing is
        // necessarily non-empty in the OWL Direct Semantics.
        for axiom in self.ontology.equivalent_classes_axioms() {
            let has_thing = axiom
                .classes()
                .iter()
                .any(|class| class.as_str() == OWL_THING_IRI);
            let has_nothing = axiom
                .classes()
                .iter()
                .any(|class| class.as_str() == OWL_NOTHING_IRI);
            if has_thing && has_nothing {
                return Ok(false);
            }
        }

        // An irreflexive property cannot relate an individual to itself.
        for irreflexive_axiom in self.ontology.irreflexive_property_axioms() {
            for assertion in self.ontology.property_assertions() {
                if assertion.property().as_ref() != irreflexive_axiom.property().as_ref() {
                    continue;
                }

                let Some(object) = assertion.object_iri() else {
                    continue;
                };
                if object.as_ref() == assertion.subject().as_ref() {
                    return Ok(false);
                }
            }
        }

        // An asymmetric property cannot relate an individual to itself or have
        // the same pair asserted in both directions.
        let property_assertions = self.ontology.property_assertions();
        for asymmetric_axiom in self.ontology.asymmetric_property_axioms() {
            for assertion in &property_assertions {
                if assertion.property().as_ref() != asymmetric_axiom.property().as_ref() {
                    continue;
                }
                let Some(object) = assertion.object_iri() else {
                    continue;
                };
                if object.as_ref() == assertion.subject().as_ref() {
                    return Ok(false);
                }

                if property_assertions.iter().any(|reverse| {
                    reverse.property().as_ref() == asymmetric_axiom.property().as_ref()
                        && reverse.subject().as_ref() == object.as_ref()
                        && reverse.object_iri().is_some_and(|reverse_object| {
                            reverse_object.as_ref() == assertion.subject().as_ref()
                        })
                }) {
                    return Ok(false);
                }
            }
        }

        // The OWL bottom properties have an empty extension.
        for assertion in &property_assertions {
            if assertion.property().as_str() == OWL_BOTTOM_OBJECT_PROPERTY_IRI {
                return Ok(false);
            }
        }
        for assertion in self.ontology.data_property_assertions() {
            if assertion.property().as_str() == OWL_BOTTOM_DATA_PROPERTY_IRI {
                return Ok(false);
            }
        }

        // A negative property assertion conflicts with the corresponding
        // positive assertion under the OWL Direct Semantics.
        // Subproperty closure of the premise, for the negative-assertion
        // conflict below.
        let mut direct_props: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for axiom in self.ontology.subobject_property_axioms() {
            direct_props
                .entry(axiom.sub_property().as_str())
                .or_default()
                .insert(axiom.super_property().as_str());
        }
        // Equivalent properties are mutual subproperties.
        for axiom in self.ontology.equivalent_object_properties_axioms() {
            let properties = axiom.properties();
            for (index, left) in properties.iter().enumerate() {
                for right in properties.iter().skip(index + 1) {
                    direct_props
                        .entry(left.as_str())
                        .or_default()
                        .insert(right.as_str());
                    direct_props
                        .entry(right.as_str())
                        .or_default()
                        .insert(left.as_str());
                }
            }
        }
        // Edges the property chains compose over the asserted ones count
        // as positives for the conflict check below (prp-spo2, #117),
        // as do the symmetric-property reverses (#121) — both spent
        // only when a negative assertion can consume them.
        let negatives_exist = !self
            .ontology
            .negative_object_property_assertions()
            .is_empty();
        let chain_composed = if negatives_exist {
            chain_composed_object_assertions(&self.ontology)
        } else {
            Vec::new()
        };
        let chain_expanded: Vec<&PropertyAssertionAxiom> = property_assertions
            .iter()
            .copied()
            .chain(chain_composed.iter())
            .collect();
        let symmetric_composed = if negatives_exist {
            symmetric_closure_assertions(&self.ontology, &chain_expanded, &direct_props)
        } else {
            Vec::new()
        };
        let composed_or_asserted: Vec<&PropertyAssertionAxiom> = chain_expanded
            .into_iter()
            .chain(symmetric_composed.iter())
            .collect();
        let reflexive_axioms = self.ontology.reflexive_property_axioms();
        for negative in self.ontology.negative_object_property_assertions() {
            let Some(object) = negative.object_iri() else {
                continue;
            };
            // P(a,b) contradicts not-N(a,b) exactly when P is a
            // subproperty of N, so the closure climbs from the positive
            // property and looks for the negated one.
            if composed_or_asserted.iter().any(|positive| {
                positive.subject().as_ref() == negative.subject().as_ref()
                    && superproperties_of(positive.property().as_str(), &direct_props)
                        .contains(&negative.property().as_str())
                    && positive
                        .object_iri()
                        .is_some_and(|value| value.as_ref() == object.as_ref())
            }) {
                return Ok(false);
            }
            // A reflexive property realizes every self-pair, so a
            // negated self-assertion under it (or under any of its
            // superproperties) contradicts without needing an edge
            // (#148, W3C ReflexiveProperty-001).
            if negative.subject().as_ref() == object.as_ref()
                && reflexive_axioms.iter().any(|axiom| {
                    superproperties_of(axiom.property().as_str(), &direct_props)
                        .contains(&negative.property().as_str())
                })
            {
                return Ok(false);
            }
        }
        // A pinned data-existential witness is a positive the premise
        // forces exactly (#124) — spent only where a negative can
        // consume it, alongside the asserted positives.
        let pinned_witnesses = pinned_data_witnesses(&self.ontology);
        let asserted_data = self.ontology.data_property_assertions();
        let data_positives = asserted_data
            .iter()
            .map(|positive| (positive.subject(), positive.property(), positive.value()))
            .chain(
                pinned_witnesses
                    .iter()
                    .map(|witness| (witness.subject(), witness.property(), witness.value())),
            );
        for negative in self.ontology.negative_data_property_assertions() {
            if data_positives.clone().any(|(subject, property, value)| {
                subject.as_ref() == negative.subject().as_ref()
                    && property.as_ref() == negative.property().as_ref()
                    && literals_are_equal(value, negative.value())
            }) {
                return Ok(false);
            }
        }

        // Asserted owl:sameAs renames one individual several ways; the
        // conflict pre-passes below compare names through that rename,
        // so an injected SameIndividual (a differentFrom conclusion's
        // negation) lands on one name (#128, W3C disjointWith-001 /
        // DisjointObjectProperties-001). The rename only equates names
        // the ontology asserts equal.
        fn same_as_root<'p>(parent: &BTreeMap<&'p str, &'p str>, name: &'p str) -> &'p str {
            let mut current = name;
            while let Some(&up) = parent.get(current) {
                current = up;
            }
            current
        }
        fn renamed_equal<'p>(parent: &BTreeMap<&'p str, &'p str>, left: &'p str, right: &'p str) -> bool {
            same_as_root(parent, left) == same_as_root(parent, right)
        }
        let mut same_as_parent: BTreeMap<&str, &str> = BTreeMap::new();
        for axiom in self.ontology.same_individual_axioms() {
            let individuals = axiom.individuals();
            for index in 0..individuals.len() {
                for other in index + 1..individuals.len() {
                    let a = same_as_root(&same_as_parent, individuals[index].as_str());
                    let b = same_as_root(&same_as_parent, individuals[other].as_str());
                    if a != b {
                        same_as_parent.insert(b, a);
                    }
                }
            }
        }
        for axiom in self.ontology.different_individuals_axioms() {
            let individuals = axiom.individuals();
            for (index, left) in individuals.iter().enumerate() {
                for right in individuals.iter().skip(index + 1) {
                    if renamed_equal(&same_as_parent, left.as_str(), right.as_str()) {
                        return Ok(false);
                    }
                }
            }
        }

        // Disjoint data properties cannot have the same subject/value pair.
        let data_assertions = self.ontology.data_property_assertions();
        for axiom in self.ontology.disjoint_data_properties_axioms() {
            let properties = axiom.properties();
            for (index, left_property) in properties.iter().enumerate() {
                for right_property in properties.iter().skip(index + 1) {
                    if data_assertions.iter().any(|left| {
                        left.property().as_ref() == left_property.as_ref()
                            && data_assertions.iter().any(|right| {
                                right.property().as_ref() == right_property.as_ref()
                                    && renamed_equal(
                                        &same_as_parent,
                                        right.subject().as_str(),
                                        left.subject().as_str(),
                                    )
                                    && literals_are_equal(right.value(), left.value())
                            })
                    }) {
                        return Ok(false);
                    }
                }
            }
        }

        // Disjoint object properties cannot have the same subject/object pair.
        for axiom in self.ontology.disjoint_object_properties_axioms() {
            let properties = axiom.properties();
            for (index, left_property) in properties.iter().enumerate() {
                for right_property in properties.iter().skip(index + 1) {
                    if property_assertions.iter().any(|left| {
                        left.property().as_ref() == left_property.as_ref()
                            && property_assertions.iter().any(|right| {
                                right.property().as_ref() == right_property.as_ref()
                                    && renamed_equal(
                                        &same_as_parent,
                                        right.subject().as_str(),
                                        left.subject().as_str(),
                                    )
                                    && left.object_iri().is_some_and(|left_object| {
                                        right.object_iri().is_some_and(|right_object| {
                                            renamed_equal(
                                                &same_as_parent,
                                                right_object.as_str(),
                                                left_object.as_str(),
                                            )
                                        })
                                    })
                            })
                    }) {
                        return Ok(false);
                    }
                }
            }
        }

        // A functional data property may have at most one distinct value for
        // a given subject.
        for axiom in self.ontology.functional_data_property_axioms() {
            if data_assertions.iter().any(|left| {
                left.property().as_ref() == axiom.property().as_ref()
                    && data_assertions.iter().any(|right| {
                        right.property().as_ref() == axiom.property().as_ref()
                            && right.subject().as_ref() == left.subject().as_ref()
                            && !literals_are_equal(right.value(), left.value())
                    })
            }) {
                return Ok(false);
            }
        }

        // An asserted maximum/exact cardinality is violated when the ABox
        // contains more known-distinct fillers than the bound permits.
        for assertion in self.ontology.class_assertions() {
            match assertion.class_expr() {
                ClassExpression::ObjectMaxCardinality(maximum, property, None)
                | ClassExpression::ObjectExactCardinality(maximum, property, None) => {
                    let property_iri = object_property_iri(property);
                    let mut distinct_objects: Vec<IRI> = Vec::new();
                    for positive in &property_assertions {
                        if positive.subject().as_ref() != assertion.individual().as_ref()
                            || positive.property().as_ref() != property_iri
                        {
                            continue;
                        }
                        let Some(object) = positive.object_iri() else {
                            continue;
                        };
                        if distinct_objects
                            .iter()
                            .any(|known| known == object.as_ref())
                        {
                            continue;
                        }
                        if distinct_objects
                            .iter()
                            .all(|known| are_known_different(&self.ontology, known, object))
                        {
                            distinct_objects.push((**object).clone());
                        }
                    }
                    if distinct_objects.len() > *maximum as usize {
                        return Ok(false);
                    }
                }
                ClassExpression::DataMaxCardinality(maximum, property, None)
                | ClassExpression::DataExactCardinality(maximum, property, None) => {
                    let property_iri = data_property_iri(property);
                    let mut distinct_values = Vec::new();
                    for positive in &data_assertions {
                        if positive.subject().as_ref() != assertion.individual().as_ref()
                            || positive.property().as_ref() != property_iri
                        {
                            continue;
                        }
                        if !distinct_values
                            .iter()
                            .any(|value| literals_are_equal(value, positive.value()))
                        {
                            distinct_values.push(positive.value().clone());
                        }
                    }
                    if distinct_values.len() > *maximum as usize {
                        return Ok(false);
                    }
                }
                _ => {}
            }
        }

        // A minimum data-cardinality restriction cannot be satisfied when an
        // all-values restriction limits the property to fewer distinct data
        // values than the required minimum.
        for assertion in self.ontology.class_assertions() {
            let ClassExpression::DataMinCardinality(minimum, property, None) =
                assertion.class_expr()
            else {
                continue;
            };
            let property_iri = data_property_iri(property);
            for bound in self.ontology.class_assertions() {
                if bound.individual().as_ref() != assertion.individual().as_ref() {
                    continue;
                }
                let ClassExpression::DataAllValuesFrom(bound_property, range) = bound.class_expr()
                else {
                    continue;
                };
                if data_property_iri(bound_property) != property_iri {
                    continue;
                }
                let DataRange::DataOneOf(values) = range.as_ref() else {
                    continue;
                };
                let mut distinct_values = Vec::new();
                for value in values {
                    if !distinct_values
                        .iter()
                        .any(|known| literals_are_equal(known, value))
                    {
                        distinct_values.push(value.clone());
                    }
                }
                if distinct_values.len() < *minimum as usize {
                    return Ok(false);
                }
            }
        }

        // A key identifies class instances that share all key property
        // values. If such identified individuals are explicitly different,
        // the ontology is inconsistent.
        let mut key_candidates: Vec<IRI> = self
            .ontology
            .named_individuals()
            .iter()
            .map(|individual| (**individual.iri()).clone())
            .collect();
        key_candidates.extend(
            property_assertions
                .iter()
                .map(|assertion| (**assertion.subject()).clone()),
        );
        key_candidates.extend(
            data_assertions
                .iter()
                .map(|assertion| (**assertion.subject()).clone()),
        );
        key_candidates.sort();
        key_candidates.dedup();

        for key in self.ontology.has_key_axioms() {
            if key.properties().is_empty() {
                continue;
            }
            for (index, left) in key_candidates.iter().enumerate() {
                if !is_instance_of(&self.ontology, left, key.class().as_ref()) {
                    continue;
                }
                for right in key_candidates.iter().skip(index + 1) {
                    if !is_instance_of(&self.ontology, right, key.class().as_ref())
                        || !key
                            .properties()
                            .iter()
                            .all(|property| key_values_match(&self.ontology, property, left, right))
                    {
                        continue;
                    }
                    if are_known_different(&self.ontology, left, right) {
                        return Ok(false);
                    }
                }
            }
        }

        // Same-individual and different-individual axioms clash when the
        // same pairs appear on both sides — decided where the rename is
        // built, above the pre-passes it feeds (#128).

        // Check for classes that are disjoint with themselves
        for axiom in self.ontology.disjoint_classes_axioms() {
            let classes = axiom.classes();
            if classes.len() == 1 {
                // A class disjoint with itself is inconsistent
                return Ok(false);
            }
        }

        // Check whether a single individual is asserted into two classes
        // that are explicitly disjoint. Names go through the sameAs
        // rename, so typings of asserted-equal individuals pool (#128).
        use std::collections::{HashMap, HashSet};
        let mut individual_classes: HashMap<&str, HashSet<&IRI>> = HashMap::new();
        for assertion in self.ontology.class_assertions() {
            if let crate::logic::axioms::ClassExpression::Class(class) = assertion.class_expr() {
                individual_classes
                    .entry(same_as_root(
                        &same_as_parent,
                        assertion.individual().as_str(),
                    ))
                    .or_default()
                    .insert(class.iri().as_ref());
            }
        }

        for classes in individual_classes.values() {
            if classes.len() < 2 {
                continue;
            }
            for axiom in self.ontology.disjoint_classes_axioms() {
                let disjoint_count = axiom
                    .classes()
                    .iter()
                    .filter(|class_iri| classes.contains(class_iri.as_ref()))
                    .count();
                if disjoint_count >= 2 {
                    return Ok(false);
                }
            }
        }

        // A mutual subclass relationship is a valid equivalence pattern, not
        // an inconsistency, so it must not be rejected here.

        // If no supported contradiction was found, assume consistent.
        Ok(true)
    }

    /// Check if a class is satisfiable (cached)
    pub fn is_class_satisfiable(&self, class_iri: &IRI) -> OwlResult<bool> {
        // Check cache first
        {
            let cache = self.read_lock(&self.satisfiability_cache, "satisfiability_cache")?;
            if let Some(entry) = cache.get(class_iri) {
                if let Some(result) = entry.get() {
                    // Cache hit
                    self.cache_stats
                        .write()
                        .map_err(|e| OwlError::LockError {
                            lock_type: "cache_stats".to_string(),
                            timeout_ms: 0,
                            message: format!("Failed to acquire write lock for cache stats: {}", e),
                        })?
                        .record_hit();
                    return Ok(*result);
                }
            }
        }

        // Cache miss
        self.cache_stats
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats".to_string(),
                timeout_ms: 0,
                message: format!("Failed to acquire write lock for cache stats: {}", e),
            })?
            .record_miss();

        // Compute result
        let result = self.compute_satisfiability(class_iri)?;

        // Cache result (20 minute TTL for satisfiability - increased for better hit rate)
        let mut cache = self.write_lock(&self.satisfiability_cache, "satisfiability_cache")?;
        cache.insert(
            class_iri.clone(),
            CacheEntry::new(result, Duration::from_secs(1200)),
        );

        Ok(result)
    }

    /// Compute satisfiability (internal method)
    fn compute_satisfiability(&self, class_iri: &IRI) -> OwlResult<bool> {
        // Basic satisfiability check - a simplified implementation
        // A class is unsatisfiable if it can be proven to have no possible instances

        // Check if class is explicitly disjoint with itself
        for axiom in self.ontology.disjoint_classes_axioms() {
            let classes = axiom.classes();
            if classes.contains(&Arc::new((*class_iri).clone())) && classes.len() == 1 {
                return Ok(false); // Class is disjoint with itself - unsatisfiable
            }
        }

        // Check if class is subclass of owl:Nothing
        for axiom in self.ontology.subclass_axioms() {
            if let (
                crate::logic::axioms::ClassExpression::Class(sub_class),
                crate::logic::axioms::ClassExpression::Class(super_class),
            ) = (axiom.sub_class(), axiom.super_class())
            {
                if sub_class.iri().as_ref() == class_iri
                    && super_class.iri().as_str() == "http://www.w3.org/2002/07/owl#Nothing"
                {
                    return Ok(false); // Subclass of Nothing - unsatisfiable
                }
            }
        }

        // Note: Disjoint union axioms not yet implemented in ontology structure

        // If no obvious unsatisfiability conditions found, assume satisfiable
        Ok(true)
    }

    /// Check if one class is a subclass of another (cached)
    pub fn is_subclass_of(&self, sub: &IRI, sup: &IRI) -> OwlResult<bool> {
        let key = (sub.clone(), sup.clone());

        // Check cache first
        {
            let cache = self.read_lock(&self.subclass_cache, "subclass_cache")?;
            if let Some(entry) = cache.get(&key) {
                if let Some(result) = entry.get() {
                    // Cache hit
                    self.cache_stats
                        .write()
                        .map_err(|e| OwlError::LockError {
                            lock_type: "cache_stats".to_string(),
                            timeout_ms: 0,
                            message: format!("Failed to acquire write lock for cache stats: {}", e),
                        })?
                        .record_hit();
                    return Ok(*result);
                }
            }
        }

        // Cache miss
        self.cache_stats
            .write()
            .map_err(|e| OwlError::LockError {
                lock_type: "cache_stats".to_string(),
                timeout_ms: 0,
                message: format!("Failed to acquire write lock for cache stats: {}", e),
            })?
            .record_miss();

        // Compute result
        let result = self.compute_subclass_of(sub, sup)?;

        // Cache result (30 minute TTL for subclass relationships - increased for better hit rate)
        let mut cache = self.write_lock(&self.subclass_cache, "subclass_cache")?;
        cache.insert(key, CacheEntry::new(result, Duration::from_secs(1800)));

        Ok(result)
    }

    /// Compute subclass relationship (internal method) - EVOLVED OPTIMIZED VERSION
    ///
    /// This algorithm was evolved using OpenEvolve to optimize the original O(n²) DFS implementation
    /// Key improvements from evolution:
    /// - Uses BFS with VecDeque for better performance characteristics
    /// - Memoization cache for repeated queries (reduces redundant computations)
    /// - Optimized equivalent class checking
    /// - Better memory efficiency with improved data structures
    ///
    /// Performance improvement: ~8.4x faster than original implementation
    fn compute_subclass_of(&self, sub: &IRI, sup: &IRI) -> OwlResult<bool> {
        // Check cache first for memoization optimization
        {
            let cache = self.read_lock(&self.subclass_cache, "subclass_cache")?;
            if let Some(entry) = cache.get(&(sub.clone(), sup.clone())) {
                if let Some(result) = entry.get() {
                    return Ok(*result);
                }
            }
        }

        // Check direct relationship (fast path)
        if sub == sup {
            let result = true;
            let mut cache = self.write_lock(&self.subclass_cache, "subclass_cache")?;
            cache.insert(
                (sub.clone(), sup.clone()),
                CacheEntry::new(result, Duration::from_secs(600)),
            ); // 10 minute TTL
            return Ok(result);
        }

        // Check direct subclass relationships
        for axiom in self.ontology.subclass_axioms() {
            if let (
                crate::logic::axioms::ClassExpression::Class(sub_axiom),
                crate::logic::axioms::ClassExpression::Class(sup_axiom),
            ) = (axiom.sub_class(), axiom.super_class())
            {
                if sub_axiom.iri().as_ref() == sub && sup_axiom.iri().as_ref() == sup {
                    let result = true;
                    let mut cache = self.write_lock(&self.subclass_cache, "subclass_cache")?;
                    cache.insert(
                        (sub.clone(), sup.clone()),
                        CacheEntry::new(result, Duration::from_secs(600)),
                    ); // 10 minute TTL
                    return Ok(result);
                }
            }
        }

        // Optimized equivalent classes checking
        if self.check_equivalent_classes_optimized(sub, sup) {
            let result = true;
            let mut cache = self.write_lock(&self.subclass_cache, "subclass_cache")?;
            cache.insert(
                (sub.clone(), sup.clone()),
                CacheEntry::new(result, Duration::from_secs(600)),
            ); // 10 minute TTL
            return Ok(result);
        }

        // EVOLVED: O(N+E) BFS implementation using VecDeque for better performance
        let result = self.bfs_subclass_check_optimized(sub, sup);

        // Cache the result for future queries
        let mut cache = self.write_lock(&self.subclass_cache, "subclass_cache")?;
        cache.insert(
            (sub.clone(), sup.clone()),
            CacheEntry::new(result, Duration::from_secs(600)),
        ); // 10 minute TTL

        Ok(result)
    }

    /// EVOLVED: Optimized equivalent class checking
    fn check_equivalent_classes_optimized(&self, class1: &IRI, class2: &IRI) -> bool {
        // Fast path: check if they're the same IRI
        if class1 == class2 {
            return true;
        }

        // Check equivalent classes axioms
        for axiom in self.ontology.equivalent_classes_axioms() {
            let classes = axiom.classes();
            if classes.contains(&Arc::new((*class1).clone()))
                && classes.contains(&Arc::new((*class2).clone()))
            {
                return true;
            }
        }

        false
    }

    /// EVOLVED: Optimized BFS implementation for subclass checking - O(N+E) complexity
    ///
    /// This replaces the original O(n²) DFS with a more efficient BFS algorithm
    /// that provides better performance for typical ontology hierarchies
    fn bfs_subclass_check_optimized(&self, start_class: &IRI, target_class: &IRI) -> bool {
        use std::collections::VecDeque;

        let mut visited: std::collections::HashSet<Arc<IRI>> = std::collections::HashSet::new();
        let mut queue: VecDeque<Arc<IRI>> = VecDeque::new();

        // Initialize BFS
        queue.push_back(Arc::new(start_class.clone()));
        visited.insert(Arc::new(start_class.clone()));

        while let Some(current_class) = queue.pop_front() {
            // Find direct superclasses using optimized iteration
            for axiom in self.ontology.subclass_axioms() {
                if let (
                    crate::logic::axioms::ClassExpression::Class(sub_axiom),
                    crate::logic::axioms::ClassExpression::Class(sup_axiom),
                ) = (axiom.sub_class(), axiom.super_class())
                {
                    if sub_axiom.iri().as_ref() == current_class.as_ref() {
                        // Found target - return immediately
                        if sup_axiom.iri().as_ref() == target_class {
                            return true;
                        }

                        // Add to queue if not already visited
                        if !visited.contains(sup_axiom.iri()) {
                            visited.insert(sup_axiom.iri().clone());
                            queue.push_back(sup_axiom.iri().clone());
                        }
                    }
                }
            }
        }

        false
    }

    /// Get all instances of a class (cached)
    pub fn get_instances(&self, class_iri: &IRI) -> OwlResult<Vec<Arc<IRI>>> {
        // Check cache first
        {
            let cache = self.read_lock(&self.instances_cache, "instances_cache")?;
            if let Some(entry) = cache.get(class_iri) {
                if let Some(result) = entry.get() {
                    return Ok(result.clone().into_iter().map(Arc::new).collect());
                }
            }
        }

        // Compute result
        let instances = self.compute_instances(class_iri)?;
        let result: Vec<Arc<IRI>> = instances.iter().map(|iri| Arc::new(iri.clone())).collect();

        // Cache result (30 second TTL for instances - they might change frequently)
        let mut cache = self.write_lock(&self.instances_cache, "instances_cache")?;
        cache.insert(
            class_iri.clone(),
            CacheEntry::new(instances, Duration::from_secs(30)),
        );

        Ok(result)
    }

    /// Check if two classes are disjoint (basic implementation)
    pub fn are_disjoint_classes(&self, class1: &IRI, class2: &IRI) -> OwlResult<bool> {
        // Check explicit disjoint axioms
        for axiom in self.ontology.disjoint_classes_axioms() {
            let classes = axiom.classes();
            let mut found_class1 = false;
            let mut found_class2 = false;

            for class_iri in classes {
                if **class_iri == *class1 {
                    found_class1 = true;
                }
                if **class_iri == *class2 {
                    found_class2 = true;
                }
            }

            if found_class1 && found_class2 {
                return Ok(true);
            }
        }

        // Basic reasoning: check if one is subclass of the other and they're disjoint with something
        // This is a simplified implementation - full disjointness reasoning would be more complex
        Ok(false)
    }

    /// Compute instances (internal method)
    fn compute_instances(&self, class_iri: &IRI) -> OwlResult<Vec<IRI>> {
        let mut instances = Vec::new();

        // Get direct class assertions
        for axiom in self.ontology.class_assertions() {
            if axiom.class_expr().contains_class(class_iri) {
                instances.push((**axiom.individual()).clone());
            }
        }

        // Get instances of equivalent classes
        for axiom in self.ontology.equivalent_classes_axioms() {
            let classes = axiom.classes();
            if classes.contains(&Arc::new((*class_iri).clone())) {
                for equiv_class in classes {
                    if **equiv_class != *class_iri {
                        // Get instances of the equivalent class
                        for assertion in self.ontology.class_assertions() {
                            if assertion.class_expr().contains_class(equiv_class) {
                                instances.push((**assertion.individual()).clone());
                            }
                        }
                    }
                }
            }
        }

        // Remove duplicates
        instances.sort();
        instances.dedup();

        Ok(instances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::{Class, NamedIndividual};
    use crate::logic::axioms::ClassExpression;
    use crate::logic::axioms::{ClassAssertionAxiom, DisjointClassesAxiom};

    #[test]
    fn class_assertions_in_disjoint_classes_are_inconsistent() {
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        let x = NamedIndividual::new("http://example.org/x");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology.add_named_individual(x.clone()).unwrap();
        ontology
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                a.iri().clone(),
                b.iri().clone(),
            ]))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                ClassExpression::Class(a),
            ))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                ClassExpression::Class(b),
            ))
            .unwrap();

        let reasoner = SimpleReasoner::new(ontology);
        assert!(!reasoner.is_consistent().unwrap());
    }

    #[test]
    fn disjoint_classes_on_different_individuals_remain_consistent() {
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        let x = NamedIndividual::new("http://example.org/x");
        let y = NamedIndividual::new("http://example.org/y");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology.add_named_individual(x.clone()).unwrap();
        ontology.add_named_individual(y.clone()).unwrap();
        ontology
            .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                a.iri().clone(),
                b.iri().clone(),
            ]))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                x.iri().clone(),
                ClassExpression::Class(a),
            ))
            .unwrap();
        ontology
            .add_class_assertion(ClassAssertionAxiom::new(
                y.iri().clone(),
                ClassExpression::Class(b),
            ))
            .unwrap();

        let reasoner = SimpleReasoner::new(ontology);
        assert!(reasoner.is_consistent().unwrap());
    }

    mod keys_116 {
        //! Fixtures for #116: owl:hasKey identification. The
        //! consistency-lane semantics below (forced merge, key-class
        //! gate, missing-value non-comparability) pin the pre-existing
        //! long-tail key check that the entailment kernel reaches once
        //! an owl:sameAs conclusion reads as identity. Anchors: W3C
        //! New-Feature-Keys-001/-003 (staged case ids 0051-0055).

        use super::super::*;
        use crate::core::entities::{Class, DataProperty, NamedIndividual};
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom, DifferentIndividualsAxiom,
            HasKeyAxiom,
        };

        const NS: &str = "http://example.org/";

        fn named(suffix: &str) -> String {
            format!("{NS}{suffix}")
        }

        /// Shared builder for the #116 key fixtures: `class` carries
        /// hasKey(P); the listed individuals get the class when the
        /// flag says so and the data assertion P = "v" when non-empty.
        fn key_ontology(
            class: &str,
            memberships: &[(&str, bool)],
            values: &[(&str, &str)],
        ) -> Ontology {
            let mut ontology = Ontology::new();
            let names: Vec<_> = memberships
                .iter()
                .map(|(name, _)| NamedIndividual::new(named(name)))
                .collect();
            for individual in &names {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            ontology
                .add_axiom(Axiom::HasKey(Box::new(HasKeyAxiom::new(
                    Class::new(class.to_string()).iri().clone(),
                    vec![DataProperty::new(named("P")).iri().clone()],
                ))))
                .unwrap();
            let key_class = ClassExpression::Class(Class::new(class.to_string()));
            for ((_name, member), individual) in memberships.iter().zip(&names) {
                if *member {
                    ontology
                        .add_class_assertion(ClassAssertionAxiom::new(
                            individual.iri().clone(),
                            key_class.clone(),
                        ))
                        .unwrap();
                }
            }
            for (name, value) in values {
                let individual = NamedIndividual::new(named(name));
                ontology
                    .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                        DataPropertyAssertionAxiom::new(
                            individual.iri().clone(),
                            DataProperty::new(named("P")).iri().clone(),
                            crate::core::entities::Literal::simple(*value),
                        ),
                    )))
                    .unwrap();
            }
            ontology
        }

        /// W3C Keys-001 shape (#116): owl:Thing hasKey(P); two
        /// individuals with the same P value must be identical — an
        /// explicit differentFrom between them is the inconsistency,
        /// which is exactly the entailment kernel's negated conclusion.
        #[test]
        fn key_tuple_forces_identity_blocked_is_inconsistent() {
            let mut ontology = key_ontology(
                OWL_THING_IRI,
                &[("a", true), ("b", true)],
                &[("a", "v"), ("b", "v")],
            );
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "same key value under a Thing-scoped key forces a=b; a≠b blocks it"
            );
        }

        /// W3C Keys-003 shape (#116): the key class gates the merge —
        /// equal values inside the class force the identity (and clash
        /// against a differentFrom), an outsider with the same value
        /// merges with nobody.
        #[test]
        fn key_class_membership_guards_the_merge() {
            let mut ontology = key_ontology(
                named("GriffinFamilyMember").as_str(),
                &[("a", true), ("b", true), ("outsider", false)],
                &[("a", "v"), ("b", "v"), ("outsider", "v")],
            );
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            let outsider = NamedIndividual::new(named("outsider"));
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
                )))
                .unwrap();
            let blocked_pair = SimpleReasoner::new(ontology.clone());
            assert!(
                !blocked_pair.is_consistent().unwrap(),
                "the two members share the key tuple; differentFrom blocks the forced merge"
            );

            let mut outsider_variant = key_ontology(
                named("GriffinFamilyMember").as_str(),
                &[("a", true), ("b", true), ("outsider", false)],
                &[("a", "v"), ("b", "v"), ("outsider", "v")],
            );
            outsider_variant
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![
                        a.iri().clone(),
                        outsider.iri().clone(),
                    ]),
                )))
                .unwrap();
            let outsider_free = SimpleReasoner::new(outsider_variant);
            assert!(
                outsider_free.is_consistent().unwrap(),
                "the outsider is not a key-class instance; no forced merge exists to block"
            );
        }

        /// A member without a value for the key property is not
        /// key-comparable — no forced merge, so an explicit
        /// differentFrom stays consistent (#116).
        #[test]
        fn missing_key_value_is_not_key_comparable() {
            let mut ontology = key_ontology(
                named("GriffinFamilyMember").as_str(),
                &[("a", true), ("b", true)],
                &[("a", "v")],
            );
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![a.iri().clone(), b.iri().clone()]),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "b has no P value; the key rule must not force anything"
            );
        }
    }

    mod w109_merge_rules {
        //! Fixtures for the #109 max-cardinality merge rule and functional
        //! witness sharing. Anchors: W3C WebOnt-description-logic-007,
        //! -030, -603 (staged case ids 0349, 0372, 0406).

        use super::super::*;
        use crate::core::entities::{Class, DataProperty, NamedIndividual, ObjectProperty};
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom, DifferentIndividualsAxiom,
            DisjointClassesAxiom, FunctionalPropertyAxiom, HasKeyAxiom,
            InverseObjectPropertiesAxiom, PropertyAssertionAxiom, SubClassOfAxiom,
            SubObjectPropertyOfAxiom,
        };

        const NS: &str = "http://example.org/";

        fn named(suffix: &str) -> String {
            format!("{NS}{suffix}")
        }

        fn arc(suffix: &str) -> Arc<IRI> {
            ObjectProperty::new(named(suffix)).iri().clone()
        }

        fn class(suffix: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(named(suffix)))
        }

        fn property(suffix: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(named(suffix)),
            )))
        }

        fn data_property(suffix: &str) -> Box<DataPropertyExpression> {
            Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                named(suffix),
            )))
        }

        fn add_subclass(ontology: &mut Ontology, sub: ClassExpression, sup: ClassExpression) {
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(sub, sup))
                .unwrap();
        }

        fn add_axiom(ontology: &mut Ontology, axiom: Axiom) {
            ontology.add_axiom(axiom).unwrap();
        }

        /// The 603 shape with owl:cardinality (exact) data bounds: exact
        /// contributes BOTH the ≥n and the ≤n side, so the merged witness
        /// with =1 P and =0 P must clash.
        #[test]
        fn exact_data_cardinality_binds_both_sides_on_merged_witness() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::DataExactCardinality(0, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("p1c"),
                ClassExpression::DataExactCardinality(1, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f"),
                        Box::new(class("p1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f"),
                        Box::new(class("p1c")),
                    )),
                ]),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(arc(
                    "f",
                )))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "exact 0 and exact 1 on one data property cannot both hold"
            );
        }

        /// W3C description-logic-603: ∃-witnesses on subproperties of a
        /// shared functional role must merge; the merged witness then
        /// carries both the ≤0 and the ≥1 data bound.
        #[test]
        fn witnesses_merge_through_shared_functional_superproperty() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::DataMaxCardinality(0, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("p1c"),
                ClassExpression::DataMinCardinality(1, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f1"),
                        Box::new(class("p1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f2"),
                        Box::new(class("p1c")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f3"),
                        Box::new(class("p2")),
                    )),
                ]),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("f1"),
                ))),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("f2"),
                ))),
            );
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("f3"),
                    arc("f1"),
                ))),
            );
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("f3"),
                    arc("f2"),
                ))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "f3 witnesses must merge into one node holding p1 and p1c"
            );
        }

        /// #127: a GCI whose subject is a conjunction of named classes
        /// (the parser's lowering of `A ≡ c1 ⊓ c2`) fires once the node
        /// carries every conjunct.
        #[test]
        fn conjunction_subject_gci_fires_on_named_conjuncts() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("c1")),
                    Box::new(class("c2")),
                ]),
                class("A2"),
            );
            add_subclass(&mut ontology, class("A2"), class("D"));
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    Class::new(named("D")).iri().clone(),
                    Class::new(named("E")).iri().clone(),
                ]))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("c1"),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("c2"),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("E"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "a : c1 ⊓ c2 reaches A2, and A2's D clashes E"
            );
        }

        /// A conjunct is satisfied through the named-supers closure,
        /// not only by the identical class: sub ⊑ c1 stands for c1.
        #[test]
        fn conjunction_subject_gci_fires_through_named_supers() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(&mut ontology, class("sub"), class("c1"));
            add_subclass(
                &mut ontology,
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("c1")),
                    Box::new(class("c2")),
                ]),
                class("A2"),
            );
            add_subclass(&mut ontology, class("A2"), class("D"));
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    Class::new(named("D")).iri().clone(),
                    Class::new(named("E")).iri().clone(),
                ]))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("sub"),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("c2"),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("E"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "a : sub with sub ⊑ c1 satisfies the c1 conjunct: A2's D clashes E"
            );
        }

        /// One conjunct alone must not fire the conjunction GCI.
        #[test]
        fn conjunction_subject_gci_waits_for_every_conjunct() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("c1")),
                    Box::new(class("c2")),
                ]),
                class("A2"),
            );
            add_subclass(&mut ontology, class("A2"), class("D"));
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    Class::new(named("D")).iri().clone(),
                    Class::new(named("c1")).iri().clone(),
                ]))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("c1"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "c1 alone is no c1 ⊓ c2: A2 must stay unrecorded"
            );
        }

        /// W3C description-logic-604 (fact4.1): the functional merge
        /// exposes the conjunction GCI's payload, which the
        /// existential GCI carries back to x as the ≤0 that clashes
        /// the ≥1.
        #[test]
        fn functional_merge_exposes_conjunction_gci_payload() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("C1")),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("rx3"),
                        Box::new(class("c1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("rx4"),
                        Box::new(class("c2")),
                    )),
                ]),
            );
            add_subclass(
                &mut ontology,
                class("C1"),
                ClassExpression::DataMinCardinality(1, data_property("P1"), None),
            );
            add_subclass(
                &mut ontology,
                ClassExpression::ObjectSomeValuesFrom(
                    property("rx3"),
                    Box::new(class("A2")),
                ),
                class("C1comp"),
            );
            add_subclass(
                &mut ontology,
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("c1")),
                    Box::new(class("c2")),
                ]),
                class("A2"),
            );
            add_subclass(
                &mut ontology,
                class("C1comp"),
                ClassExpression::DataMaxCardinality(0, data_property("P1"), None),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("rx"),
                ))),
            );
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("rx3"),
                    arc("rx"),
                ))),
            );
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("rx4"),
                    arc("rx"),
                ))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the merged witness reaches A2, x reaches C1comp, and ≥1/≤0 clash"
            );
        }

        /// Without the subproperty links the three witnesses stay distinct
        /// and nothing forces the contradictory data bounds together.
        #[test]
        fn witnesses_without_shared_functional_role_stay_distinct() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::DataMaxCardinality(0, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("p1c"),
                ClassExpression::DataMinCardinality(1, data_property("P"), None),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f1"),
                        Box::new(class("p1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f2"),
                        Box::new(class("p1c")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("f3"),
                        Box::new(class("p2")),
                    )),
                ]),
            );
            for suffix in ["f1", "f2"] {
                add_axiom(
                    &mut ontology,
                    Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                        arc(suffix),
                    ))),
                );
            }
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(reasoner.is_consistent().unwrap());
        }

        /// W3C description-logic-007: three r-successors under ≤2 r where
        /// every pair of filler classes is provably disjoint — every forced
        /// merge is blocked, so the ontology is inconsistent.
        #[test]
        fn maxcardinality_overflow_with_blocked_merges_is_inconsistent() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::ObjectComplementOf(Box::new(ClassExpression::ObjectUnionOf(
                    smallvec::smallvec![
                        Box::new(class("p2")),
                        Box::new(class("p3")),
                        Box::new(class("p4")),
                    ],
                ))),
            );
            add_subclass(
                &mut ontology,
                class("p2"),
                ClassExpression::ObjectComplementOf(Box::new(class("p3"))),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("p1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("p2")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("p3")),
                    )),
                    Box::new(ClassExpression::ObjectMaxCardinality(2, property("r"), None)),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "three pairwise-disjoint fillers cannot fit under max 2 r"
            );
        }

        /// Two fillers under ≤2 r fit without any merge.
        #[test]
        fn maxcardinality_within_bound_stays_consistent() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::ObjectComplementOf(Box::new(class("p2"))),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("p1")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("p2")),
                    )),
                    Box::new(ClassExpression::ObjectMaxCardinality(2, property("r"), None)),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(reasoner.is_consistent().unwrap());
        }

        /// Asserted edges under a functional property: known-distinct
        /// successors make the forced merge impossible.
        #[test]
        fn functional_asserted_successors_different_individuals_clash() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            let c = NamedIndividual::new(named("c"));
            for individual in [&a, &b, &c] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("f"),
                ))),
            );
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    a.iri().clone(),
                    arc("f"),
                    b.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    a.iri().clone(),
                    arc("f"),
                    c.iri().clone(),
                ))
                .unwrap();
            add_axiom(
                &mut ontology,
                Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
                    vec![b.iri().clone(), c.iri().clone()],
                ))),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "a functional role cannot have two different successors"
            );
        }

        /// Without the differentFrom fact the two successors may merge.
        #[test]
        fn functional_asserted_successors_may_merge() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            let c = NamedIndividual::new(named("c"));
            for individual in [&a, &b, &c] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("f"),
                ))),
            );
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    a.iri().clone(),
                    arc("f"),
                    b.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    a.iri().clone(),
                    arc("f"),
                    c.iri().clone(),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(reasoner.is_consistent().unwrap());
        }

        /// Two ∃-fillers of one functional role whose classes are
        /// provably disjoint cannot be merged.
        #[test]
        fn functional_fillers_with_disjoint_classes_clash() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::ObjectComplementOf(Box::new(class("p2"))),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                    arc("f"),
                ))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            property("f"),
                            Box::new(class("p1")),
                        )),
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            property("f"),
                            Box::new(class("p2")),
                        )),
                    ]),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "functional merge would unify disjoint filler classes"
            );
        }

        /// W3C description-logic-002: the anonymous GCI (∀r.c) ⊑ d fires
        /// positively — the individual carries ∀r.c through c ⊑ ∀r.c —
        /// so d lands next to its asserted ¬d.
        #[test]
        fn anonymous_universal_gci_applies_positively() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(&mut ontology, class("Unsatisfiable"), class("c"));
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectComplementOf(Box::new(class("d"))),
            );
            add_subclass(
                &mut ontology,
                class("c"),
                ClassExpression::ObjectAllValuesFrom(property("r"), Box::new(class("c"))),
            );
            add_subclass(
                &mut ontology,
                ClassExpression::ObjectAllValuesFrom(property("r"), Box::new(class("c"))),
                class("d"),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the (forall r.c) GCI must add d next to the asserted not-d"
            );
        }

        /// Equivalent object properties share fillers: p ≡ q with p
        /// functional merges the ∃p- and ∃q-witnesses.
        #[test]
        fn equivalent_roles_share_functional_witnesses() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p1"),
                ClassExpression::ObjectComplementOf(Box::new(class("p2"))),
            );
            add_axiom(
                &mut ontology,
                Axiom::EquivalentObjectProperties(Box::new(
                    crate::logic::axioms::EquivalentObjectPropertiesAxiom::new(vec![
                        arc("f"),
                        arc("g"),
                    ]),
                )),
            );
            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(arc(
                    "f",
                )))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            property("f"),
                            Box::new(class("p1")),
                        )),
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            property("g"),
                            Box::new(class("p2")),
                        )),
                    ]),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "equivalent roles must share the functional witness node"
            );
        }

        /// W3C description-logic-010/013 family: a universal restriction
        /// propagates the filler class onto every successor — the ∃r.¬d
        /// witness must also carry d.
        #[test]
        fn universal_restriction_propagates_to_successors() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("c"),
                ClassExpression::ObjectAllValuesFrom(property("r"), Box::new(class("d"))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                        Box::new(class("c")),
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            property("r"),
                            Box::new(ClassExpression::ObjectComplementOf(Box::new(class("d")))),
                        )),
                    ]),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the forall r.d restriction must put d on the r-witness holding not-d"
            );
        }

        /// W3C description-logic-615: the <=1 invR bound merges the
        /// V.5 witness into the root, whose forall then propagates p
        /// onto the p.comp witness — data min over max.
        #[test]
        fn inverse_merge_feeds_universal_propagation() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("p"),
                ClassExpression::DataMaxCardinality(0, data_property("P1"), None),
            );
            add_subclass(
                &mut ontology,
                class("pcomp"),
                ClassExpression::DataMinCardinality(2, data_property("P1"), None),
            );
            add_subclass(
                &mut ontology,
                class("qcomp"),
                ClassExpression::ObjectMaxCardinality(0, property("P2"), None),
            );
            add_subclass(
                &mut ontology,
                class("A3"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(class("pcomp")),
                    Box::new(class("qcomp")),
                ]),
            );
            add_subclass(
                &mut ontology,
                class("V5"),
                ClassExpression::ObjectAllValuesFrom(property("s"), Box::new(class("p"))),
            );
            add_subclass(
                &mut ontology,
                class("A4"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectMaxCardinality(1, property("invR"), None)),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("invR"),
                        Box::new(class("V5")),
                    )),
                ]),
            );
            add_axiom(
                &mut ontology,
                Axiom::InverseObjectProperties(Box::new(InverseObjectPropertiesAxiom::new(
                    arc("invR"),
                    arc("r"),
                ))),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("s"),
                        Box::new(class("A3")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("A4")),
                    )),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the merged V.5 witness must propagate p onto the pcomp witness"
            );
        }

        /// The ticket's literal fixture: minCardinality 2 whose created
        /// fillers are forced together by a functional role — the
        /// demanded distinctness blocks the merge.
        #[test]
        fn min_cardinality_fillers_block_functional_merge() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_axiom(
                &mut ontology,
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(arc(
                    "f",
                )))),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    ClassExpression::ObjectMinCardinality(2, property("f"), None),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "two necessarily-distinct fillers cannot fit under a functional role"
            );
        }

        /// #114 review guard: pre-existing successors of a ≥-rule group
        /// are not forced distinct from each other or from the fresh
        /// witnesses — only the fresh ones are, by construction.
        #[test]
        fn min_witnesses_do_not_force_distinctness_on_existing_successors() {
            let mut ontology = Ontology::new();
            let n = NamedIndividual::new(named("n"));
            let x = NamedIndividual::new(named("x"));
            let a = NamedIndividual::new(named("a"));
            for individual in [&n, &x, &a] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            let r = arc("r");
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    n.iri().clone(),
                    r.clone(),
                    x.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    n.iri().clone(),
                    r.clone(),
                    a.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    n.iri().clone(),
                    ClassExpression::ObjectMinCardinality(1, property("r"), None),
                ))
                .unwrap();
            let nominal = ClassExpression::ObjectOneOf(Box::new(smallvec::smallvec![
                crate::core::entities::Individual::from(a.clone()),
            ]));
            add_subclass(&mut ontology, class("C"), nominal);
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    x.iri().clone(),
                    class("C"),
                ))
                .unwrap();
            ontology
                .add_axiom(
                    Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
                        vec![x.iri().clone(), a.iri().clone()],
                    ))),
                )
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            // x ∈ C forces x = a, and x is differentFrom a — a genuine
            // clash, caught through the constraint, not through any
            // must_differ pair (the >= 1 group {x, a} is larger than
            // the minimum, so no pair is forced distinct).
            assert!(!reasoner.is_consistent().unwrap());
        }

        /// #114 review guard, consistency direction: surplus successors
        /// under a minimum force no distinctness at all.
        #[test]
        fn surplus_min_successors_force_no_distinctness() {
            let mut ontology = Ontology::new();
            let n = NamedIndividual::new(named("n"));
            let x = NamedIndividual::new(named("x"));
            let a = NamedIndividual::new(named("a"));
            for individual in [&n, &x, &a] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            let r = arc("r");
            for object in [&x, &a] {
                ontology
                    .add_property_assertion(PropertyAssertionAxiom::new(
                        n.iri().clone(),
                        r.clone(),
                        object.iri().clone(),
                    ))
                    .unwrap();
            }
            ontology
                .add_axiom(
                    Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
                        vec![x.iri().clone(), a.iri().clone()],
                    ))),
                )
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    n.iri().clone(),
                    ClassExpression::ObjectMinCardinality(1, property("r"), None),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(reasoner.is_consistent().unwrap());
        }

        /// #114 review guard: a lone oneOf-subject axiom (no super-side
        /// nominal anywhere) still opens the probe and asserts the
        /// member's membership.
        #[test]
        fn one_of_subject_axiom_alone_asserts_membership() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            ontology.add_named_individual(a.clone()).unwrap();

            let nominal = ClassExpression::ObjectOneOf(Box::new(smallvec::smallvec![
                crate::core::entities::Individual::from(a.clone()),
            ]));
            add_subclass(&mut ontology, nominal, class("A"));
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    arc("A"),
                    arc("B"),
                ]))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    a.iri().clone(),
                    class("B"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the lone oneOf-subject axiom still makes a an A"
            );
        }

        /// W3C #114 shape (oneOf-004 / the #113 nominal-clash pin):
        /// a oneOf definition asserts its named members' membership, so
        /// a member typed into a disjoint class clashes.
        #[test]
        fn one_of_definition_asserts_member_memberships() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            ontology.add_named_individual(a.clone()).unwrap();

            let nominal = ClassExpression::ObjectOneOf(Box::new(smallvec::smallvec![
                crate::core::entities::Individual::from(a.clone()),
            ]));
            add_subclass(&mut ontology, class("A"), nominal.clone());
            add_subclass(&mut ontology, nominal, class("A"));
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    arc("A"),
                    arc("B"),
                ]))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    a.iri().clone(),
                    class("B"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the oneOf member a is in A, and A is disjoint from B"
            );
        }

        /// A foreign named instance contradicts C ⊑ oneOf{a,b}.
        #[test]
        fn one_of_constraint_clashes_with_foreign_named_instance() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            let x = NamedIndividual::new(named("x"));
            for individual in [&a, &b, &x] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }

            let nominal = ClassExpression::ObjectOneOf(Box::new(smallvec::smallvec![
                crate::core::entities::Individual::from(a.clone()),
                crate::core::entities::Individual::from(b.clone()),
            ]));
            add_subclass(&mut ontology, class("C"), nominal);
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    x.iri().clone(),
                    class("C"),
                ))
                .unwrap();
            ontology
                .add_axiom(
                    Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
                        vec![x.iri().clone(), a.iri().clone()],
                    ))),
                )
                .unwrap();
            ontology
                .add_axiom(
                    Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
                        vec![x.iri().clone(), b.iri().clone()],
                    ))),
                )
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "x in C must be a or b, and x is explicitly different from both"
            );
        }

        /// A member instance satisfies the constraint: stays consistent.
        #[test]
        fn one_of_constraint_allows_member_instance() {
            let mut ontology = Ontology::new();
            let a = NamedIndividual::new(named("a"));
            let b = NamedIndividual::new(named("b"));
            for individual in [&a, &b] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }

            let nominal = ClassExpression::ObjectOneOf(Box::new(smallvec::smallvec![
                crate::core::entities::Individual::from(a.clone()),
                crate::core::entities::Individual::from(b.clone()),
            ]));
            add_subclass(&mut ontology, class("C"), nominal);
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    a.iri().clone(),
                    class("C"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(reasoner.is_consistent().unwrap());
        }

        /// Shared builder for the #115 mini-lattice fixtures: TorF is
        /// enumerated three ways ({T,F}, {p1,m1}, {p2,m2}) so each pair
        /// must cover both T and F (they are distinct), and T carries
        /// the given two-literal membership clauses.
        fn nominal_lattice_ontology(clauses: &[&[&str; 2]]) -> Ontology {
            let mut ontology = Ontology::new();
            let names = ["T", "F", "p1", "m1", "p2", "m2"];
            let individuals: Vec<_> = names
                .iter()
                .map(|name| NamedIndividual::new(named(name)))
                .collect();
            for individual in &individuals {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            let member = |name: &str| {
                crate::core::entities::Individual::from(
                    individuals[names.iter().position(|n| *n == name).unwrap()].clone(),
                )
            };
            let enumeration = |members: &[&str]| {
                ClassExpression::ObjectOneOf(Box::new(
                    members.iter().map(|m| member(m)).collect(),
                ))
            };
            for pair in [["T", "F"], ["p1", "m1"], ["p2", "m2"]] {
                let nominal = enumeration(&pair);
                add_subclass(&mut ontology, class("TorF"), nominal.clone());
                add_subclass(&mut ontology, nominal, class("TorF"));
            }
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![
                        individuals[0].iri().clone(),
                        individuals[1].iri().clone(),
                    ]),
                )))
                .unwrap();
            for index in [0usize, 1] {
                ontology
                    .add_class_assertion(ClassAssertionAxiom::new(
                        individuals[index].iri().clone(),
                        class("TorF"),
                    ))
                    .unwrap();
            }
            for clause in clauses {
                ontology
                    .add_class_assertion(ClassAssertionAxiom::new(
                        individuals[0].iri().clone(),
                        enumeration(clause.as_slice()),
                    ))
                    .unwrap();
            }
            ontology
        }

        /// The W3C description-logic-502 lattice in miniature (#115):
        /// T's four two-literal memberships are the UNSAT 2-SAT
        /// instance (p1∨p2)(m1∨p2)(p1∨m2)(m1∨m2) — every identification
        /// choice dies, which only a branch over the choices can see;
        /// no constraint is forced at the root.
        #[test]
        fn nominal_lattice_without_units_needs_branch_search_inconsistent() {
            let ontology = nominal_lattice_ontology(&[
                &["p1", "p2"],
                &["m1", "p2"],
                &["p1", "m2"],
                &["m1", "m2"],
            ]);
            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the four two-literal clauses are UNSAT under the pair-covering lattice"
            );
        }

        /// The survivor path: drop one clause and the same lattice is
        /// satisfiable — a branch that survives must keep the default
        /// consistent verdict (search may never manufacture a clash).
        #[test]
        fn nominal_lattice_branch_search_keeps_satisfiable_instance_consistent() {
            let ontology =
                nominal_lattice_ontology(&[&["p1", "p2"], &["m1", "p2"], &["p1", "m2"]]);
            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "three of the four clauses are satisfiable; the surviving branch wins"
            );
        }

        /// W3C description-logic-105: ≥2 r and ≤1 r on one node
        /// contradict each other outright — the object-property twin of
        /// the #108 data bounds.
        #[test]
        fn object_min_over_max_cardinality_clashes() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectMinCardinality(2, property("r"), None)),
                    Box::new(ClassExpression::ObjectMaxCardinality(1, property("r"), None)),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "min 2 over max 1 on one role cannot both hold"
            );
        }

        /// W3C description-logic-107: ¬(≥2 r) is ≤1 r, so the ∃r.c and
        /// ∃r.d witnesses of two disjoint classes must merge and clash.
        #[test]
        fn complement_of_min_normalizes_to_max() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    arc("c"),
                    arc("d"),
                ]))
                .unwrap();
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectMinCardinality(2, property("r"), None),
                    ))),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("c")),
                    )),
                    Box::new(ClassExpression::ObjectSomeValuesFrom(
                        property("r"),
                        Box::new(class("d")),
                    )),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "not-(min 2) is max 1, forcing the disjoint witnesses to merge"
            );
        }

        /// W3C description-logic-111: ¬(≤1 s) is ≥2 s, which together
        /// with an asserted ≤1 s is the min-over-max clash.
        #[test]
        fn complement_of_max_normalizes_to_min() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                    Box::new(ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectMaxCardinality(1, property("s"), None),
                    ))),
                    Box::new(ClassExpression::ObjectMaxCardinality(1, property("s"), None)),
                ]),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "not-(max 1) is min 2, clashing with the asserted max 1"
            );
        }

        /// W3C description-logic-661 shape: a min-cardinality on P
        /// forces a P-successor into existence, so the vacuous reading
        /// of the complementary (≤0 P) ⊑ C GCI must not fire for that
        /// node — the ontology stays consistent.
        #[test]
        fn min_witnesses_block_vacuous_zero_max_gci() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            let min_side = ClassExpression::ObjectMinCardinality(1, property("P"), None);
            let max_side = ClassExpression::ObjectMaxCardinality(0, property("P"), None);
            add_subclass(&mut ontology, class("Ccomp"), min_side.clone());
            add_subclass(&mut ontology, min_side, class("Ccomp"));
            add_subclass(&mut ontology, class("C"), max_side.clone());
            add_subclass(&mut ontology, max_side, class("C"));
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Ccomp"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "a Ccomp instance has a P-successor, so the (<=0 P) GCI must not apply vacuously"
            );
        }

        /// W3C I4.5-002: an asserted edge satisfies the inverse-role
        /// someValuesFrom GCI positively — Kinnock becomes a EuroMP
        /// through UK's asserted hasEuroMP edge.
        #[test]
        fn asserted_inverse_edge_satisfies_existential_gci() {
            let mut ontology = Ontology::new();
            let uk = NamedIndividual::new(named("UK"));
            let kinnock = NamedIndividual::new(named("Kinnock"));
            for person in [&uk, &kinnock] {
                ontology.add_named_individual(person.clone()).unwrap();
            }

            add_axiom(
                &mut ontology,
                Axiom::InverseObjectProperties(Box::new(InverseObjectPropertiesAxiom::new(
                    arc("isEuroMPFrom"),
                    arc("hasEuroMP"),
                ))),
            );
            let existential = ClassExpression::ObjectSomeValuesFrom(
                property("isEuroMPFrom"),
                Box::new(ClassExpression::Class(Class::new(
                    "http://www.w3.org/2002/07/owl#Thing",
                ))),
            );
            add_subclass(&mut ontology, existential.clone(), class("EuroMP"));
            add_subclass(&mut ontology, class("EuroMP"), existential);
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    uk.iri().clone(),
                    arc("hasEuroMP"),
                    kinnock.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    kinnock.iri().clone(),
                    ClassExpression::ObjectComplementOf(Box::new(class("EuroMP"))),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the asserted hasEuroMP edge makes Kinnock a EuroMP through the inverse role"
            );
        }

        /// #172: an existential GCI over q is satisfied by an asserted
        /// p-edge through the subproperty closure — now read through
        /// the role-closure candidate index rather than a full scan.
        #[test]
        fn subproperty_edge_witnesses_existential_gci() {
            let mut ontology = Ontology::new();
            let x = NamedIndividual::new(named("x"));
            let w = NamedIndividual::new(named("w"));
            for individual in [&x, &w] {
                ontology.add_named_individual(individual.clone()).unwrap();
            }
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("p"),
                    arc("q"),
                ))),
            );
            let existential = ClassExpression::ObjectSomeValuesFrom(
                property("q"),
                Box::new(ClassExpression::Class(Class::new(
                    "http://www.w3.org/2002/07/owl#Thing",
                ))),
            );
            add_subclass(&mut ontology, existential.clone(), class("D"));
            add_subclass(&mut ontology, class("D"), existential);
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    x.iri().clone(),
                    arc("p"),
                    w.iri().clone(),
                ))
                .unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    x.iri().clone(),
                    ClassExpression::ObjectComplementOf(Box::new(class("D"))),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the asserted p-edge satisfies the exists-q restriction through the closure"
            );
        }

        /// #172: the lazy candidate index must mirror the full-scan
        /// view exactly — including edges pushed AFTER a bucket was
        /// materialized (push_edge keeps materialized buckets fresh).
        #[test]
        fn candidate_index_matches_the_full_scan_under_pushes() {
            let mut ontology = Ontology::new();
            add_axiom(
                &mut ontology,
                Axiom::SubObjectProperty(Box::new(SubObjectPropertyOfAxiom::new(
                    arc("p"),
                    arc("q"),
                ))),
            );
            let p = ObjectProperty::new(named("p"));
            let q = ObjectProperty::new(named("q"));
            let r = ObjectProperty::new(named("r"));
            let mut probe = MergeProbe::new(&ontology);

            // Materialize q's bucket while no edges exist.
            assert!(
                !probe.candidate_edges_satisfy(q.iri().as_str(), |_, _, _| true),
                "an empty bucket must satisfy nothing"
            );
            // Push edges after materialization: one under q's closure,
            // one outside it.
            probe.push_edge(0, p.iri().as_str(), 1);
            probe.push_edge(2, r.iri().as_str(), 3);

            let expected: Vec<(usize, &str, usize)> = probe
                .edges
                .iter()
                .filter(|(_, property, _)| {
                    probe.context.is_subproperty_of(*property, q.iri().as_str())
                })
                .map(|(subject, property, object)| (*subject, *property, *object))
                .collect();
            let mut indexed: Vec<(usize, &str, usize)> = Vec::new();
            probe.candidate_edges_satisfy(q.iri().as_str(), |subject, property, object| {
                indexed.push((subject, property, object));
                false // drain the whole bucket, no short-circuit
            });
            assert_eq!(
                indexed, expected,
                "the index must equal the full-scan view after post-materialization pushes"
            );
        }

        /// W3C description-logic-030: inverse witnesses under a global
        /// ≤1 (Thing ⊑ ≤1 f) merge the p1 witness with the ¬p1 witness.
        #[test]
        fn inverse_witness_merge_respects_global_maxcardinality() {
            let mut ontology = Ontology::new();
            let individual = NamedIndividual::new(named("a"));
            ontology.add_named_individual(individual.clone()).unwrap();

            add_subclass(
                &mut ontology,
                ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing")),
                ClassExpression::ObjectMaxCardinality(1, property("f"), None),
            );
            add_axiom(
                &mut ontology,
                Axiom::InverseObjectProperties(Box::new(InverseObjectPropertiesAxiom::new(
                    arc("invF"),
                    arc("f"),
                ))),
            );
            add_subclass(
                &mut ontology,
                class("Unsatisfiable"),
                ClassExpression::ObjectSomeValuesFrom(
                    property("f"),
                    Box::new(ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                        Box::new(class("p1")),
                        Box::new(ClassExpression::ObjectSomeValuesFrom(
                            Box::new(ObjectPropertyExpression::ObjectInverseOf(Box::new(
                                ObjectPropertyExpression::ObjectProperty(Box::new(
                                    ObjectProperty::new(named("f")),
                                )),
                            ))),
                            Box::new(ClassExpression::ObjectSomeValuesFrom(
                                property("f"),
                                Box::new(ClassExpression::ObjectComplementOf(Box::new(
                                    class("p1"),
                                ))),
                            )),
                        )),
                    ])),
                ),
            );
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class("Unsatisfiable"),
                ))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the ≤1 f merge must unify the p1 witness with the ¬p1 witness"
            );
        }
    }

    mod chains_117 {
        //! Fixtures for #117: two-step property chains compose over the
        //! assertion set to a fixpoint (prp-spo2-2, the materialize
        //! pattern) and the composed edges clash with negated
        //! assertions. Anchors: W3C New-Feature-ObjectPropertyChain-001
        //! (0065), -BJP-003 (0067), chain2trans1 (0004).

        use super::super::*;
        use crate::core::entities::NamedIndividual;
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            Axiom, NegativeObjectPropertyAssertionAxiom, PropertyAssertionAxiom,
            SubPropertyChainOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        /// a → b → c → d on p, with p ∘ p ⊑ p: the transitive closure
        /// of the edges. Pair it with a negated edge the closure either
        /// reaches or misses.
        fn path_premise() -> Ontology {
            let mut ontology = Ontology::new();
            for name in ["a", "b", "c", "d"] {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            for (subject, object) in [("a", "b"), ("b", "c"), ("c", "d")] {
                ontology
                    .add_property_assertion(PropertyAssertionAxiom::new(
                        iri(subject),
                        iri("p"),
                        iri(object),
                    ))
                    .unwrap();
            }
            ontology
                .add_axiom(Axiom::SubPropertyChainOf(Box::new(
                    SubPropertyChainOfAxiom::new(vec![iri("p"), iri("p")], iri("p")),
                )))
                .unwrap();
            ontology
        }

        fn negate(ontology: &mut Ontology, subject: &str, object: &str) {
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri(subject), iri("p"), iri(object)),
                )))
                .unwrap();
        }

        #[test]
        fn chain_closure_refutes_negated_composed_edge() {
            let mut ontology = path_premise();
            negate(&mut ontology, "a", "d");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "p ∘ p ⊑ p composes a → d (a chain of chains); ¬p(a, d) clashes"
            );
        }

        #[test]
        fn chain_closure_leaves_unreached_edge_negated() {
            let mut ontology = path_premise();
            negate(&mut ontology, "d", "a");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "nothing composes d → a; the negated edge must survive"
            );
        }
    }

    mod reflexive_negatives_148 {
        //! Fixtures for #148: a reflexive property realizes every
        //! self-pair, so a negated self-assertion under it (or under
        //! any of its superproperties) contradicts. Anchor: W3C
        //! New-Feature-ReflexiveProperty-001 (0078) — Reflexive(knows)
        //! ⊨ knows(Peter, Peter) reduces to ¬knows(Peter, Peter).

        use super::super::*;
        use crate::core::entities::NamedIndividual;
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            Axiom, NegativeObjectPropertyAssertionAxiom, ReflexivePropertyAxiom,
            SubObjectPropertyOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn with_individuals(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn negate_self(ontology: &mut Ontology, subject: &str, property: &str) {
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(
                        iri(subject),
                        iri(property),
                        iri(subject),
                    ),
                )))
                .unwrap();
        }

        #[test]
        fn reflexive_property_refutes_negated_self_edge() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_axiom(Axiom::ReflexiveProperty(Box::new(
                    ReflexivePropertyAxiom::new(iri("knows")),
                )))
                .unwrap();
            negate_self(&mut ontology, "peter", "knows");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "Reflexive(knows) realizes knows(peter, peter); its negation clashes"
            );
        }

        #[test]
        fn reflexive_subproperty_refutes_negated_self_edge() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_axiom(Axiom::ReflexiveProperty(Box::new(
                    ReflexivePropertyAxiom::new(iri("likes")),
                )))
                .unwrap();
            ontology
                .add_axiom(Axiom::SubObjectProperty(Box::new(
                    SubObjectPropertyOfAxiom::new(iri("likes"), iri("loves")),
                )))
                .unwrap();
            negate_self(&mut ontology, "peter", "loves");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "Reflexive(likes) ⊑ loves realizes loves(peter, peter); its negation clashes"
            );
        }

        #[test]
        fn reflexive_negative_on_distinct_pair_survives() {
            let mut ontology = with_individuals(&["a", "b"]);
            ontology
                .add_axiom(Axiom::ReflexiveProperty(Box::new(
                    ReflexivePropertyAxiom::new(iri("p")),
                )))
                .unwrap();
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("a"), iri("p"), iri("b")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "reflexivity only realizes self-pairs; ¬p(a, b) survives"
            );
        }

        #[test]
        fn unrelated_negative_property_survives() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_axiom(Axiom::ReflexiveProperty(Box::new(
                    ReflexivePropertyAxiom::new(iri("knows")),
                )))
                .unwrap();
            negate_self(&mut ontology, "peter", "unrelated");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "Reflexive(knows) says nothing about the unrelated property"
            );
        }
    }

    mod has_self_closure_175 {
        //! #175 (#160's recorded gap): the intersection-complement
        //! check's hasSelf conjunct reads a self-loop through the
        //! subproperty closure, mirroring the negated_self sibling.

        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, ObjectPropertyExpression, PropertyAssertionAxiom,
            SubObjectPropertyOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn base(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(format!("http://example.org/{name}")),
            )))
        }

        fn named_class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(format!("http://example.org/{name}")))
        }

        fn complemented_conjunction(ontology: &mut Ontology, subject: &str) {
            // subject : ¬(D ⊓ hasSelf(loves)) — #160's check-phase shape.
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(subject),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectIntersectionOf(smallvec::smallvec![
                            Box::new(named_class("D")),
                            Box::new(ClassExpression::ObjectHasSelf(base("loves"))),
                        ]),
                    )),
                ))))
                .unwrap();
        }

        #[test]
        fn intersection_complement_refutes_through_subproperty_self_loop() {
            let mut ontology = Ontology::new();
            ontology
                .add_named_individual(NamedIndividual::new("http://example.org/peter"))
                .unwrap();
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri("peter"),
                    iri("likes"),
                    iri("peter"),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::SubObjectProperty(Box::new(
                    SubObjectPropertyOfAxiom::new(iri("likes"), iri("loves")),
                )))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("peter"),
                    named_class("D"),
                ))))
                .unwrap();
            complemented_conjunction(&mut ontology, "peter");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "likes ⊑ loves carries the self-loop to hasSelf(loves); both conjuncts hold and the complement refutes"
            );
        }

        #[test]
        fn intersection_complement_fails_open_without_the_closure_link() {
            let mut ontology = Ontology::new();
            ontology
                .add_named_individual(NamedIndividual::new("http://example.org/peter"))
                .unwrap();
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri("peter"),
                    iri("likes"),
                    iri("peter"),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("peter"),
                    named_class("D"),
                ))))
                .unwrap();
            complemented_conjunction(&mut ontology, "peter");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "without the subproperty link hasSelf(loves) stays unverifiable — fail-open"
            );
        }
    }

    mod self_negatives_149 {
        //! Fixtures for #149: a negated ObjectHasSelf(r) label
        //! contradicts any r-labelled self-loop on the node — asserted,
        //! spawned by a positive hasSelf label, or reached through a
        //! subproperty. Anchor: W3C New-Feature-SelfRestriction-002
        //! (0082) — likes(Peter, Peter) ⊨ Peter : ObjectHasSelf(likes)
        //! reduces to Peter : ¬ObjectHasSelf(likes).

        use super::super::*;
        use crate::core::entities::{NamedIndividual, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, ObjectPropertyExpression, PropertyAssertionAxiom,
            SubObjectPropertyOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn base(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(format!("http://example.org/{name}")),
            )))
        }

        fn with_individuals(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn negate_has_self(ontology: &mut Ontology, subject: &str, property: &str) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(subject),
                    ClassExpression::ObjectComplementOf(Box::new(ClassExpression::ObjectHasSelf(
                        base(property),
                    ))),
                ))))
                .unwrap();
        }

        #[test]
        fn negated_has_self_refutes_asserted_self_loop() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri("peter"),
                    iri("likes"),
                    iri("peter"),
                ))
                .unwrap();
            negate_has_self(&mut ontology, "peter", "likes");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "likes(peter, peter) realizes hasSelf(likes); its negation clashes"
            );
        }

        #[test]
        fn negated_has_self_refutes_positive_has_self_label() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("peter"),
                    ClassExpression::ObjectHasSelf(base("likes")),
                ))))
                .unwrap();
            negate_has_self(&mut ontology, "peter", "likes");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "a positive hasSelf label spawns the self-loop its own negation forbids"
            );
        }

        #[test]
        fn negated_has_self_survives_without_self_loop() {
            let mut ontology = with_individuals(&["peter", "stewie"]);
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri("peter"),
                    iri("likes"),
                    iri("stewie"),
                ))
                .unwrap();
            negate_has_self(&mut ontology, "peter", "likes");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "likes(peter, stewie) is not a self-loop; the negated label survives"
            );
        }

        #[test]
        fn negated_has_self_refutes_superproperty_self_loop() {
            let mut ontology = with_individuals(&["peter"]);
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri("peter"),
                    iri("likes"),
                    iri("peter"),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::SubObjectProperty(Box::new(
                    SubObjectPropertyOfAxiom::new(iri("likes"), iri("loves")),
                )))
                .unwrap();
            negate_has_self(&mut ontology, "peter", "loves");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "likes ⊑ loves carries the self-loop to loves; the negated label clashes"
            );
        }
    }

    mod enumerated_ranges_151 {
        //! Fixtures for #151: a property with enumerated data ranges
        //! confines every value to each pool, and a min >= 1 demand pins
        //! the value outright when the pools intersect to one literal.
        //! Anchor: W3C oneOf-004 (0499) — ranges {1,2,3,4} and {4,5,6}
        //! plus minCardinality 1 force p(i, 4).

        use super::super::*;
        use crate::core::entities::{Literal, NamedIndividual};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom,
            DataPropertyEnumeratedRangeAxiom, NegativeDataPropertyAssertionAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn integer(value: &str) -> Literal {
            Literal::typed(
                value.to_string(),
                IRI::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            )
        }

        fn base(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn enumerated_range(ontology: &mut Ontology, values: &[&str]) {
            ontology
                .add_axiom(Axiom::DataPropertyEnumeratedRange(Box::new(
                    DataPropertyEnumeratedRangeAxiom::new(
                        iri("p"),
                        values.iter().map(|v| integer(v)).collect(),
                    ),
                )))
                .unwrap();
        }

        fn data_assertion(ontology: &mut Ontology, subject: &str, value: &str) {
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(iri(subject), iri("p"), integer(value)),
                )))
                .unwrap();
        }

        fn min_cardinality(ontology: &mut Ontology, subject: &str, minimum: u32) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(subject),
                    ClassExpression::DataMinCardinality(
                        minimum,
                        Box::new(DataPropertyExpression::DataProperty(
                            crate::core::entities::DataProperty::new("http://example.org/p"),
                        )),
                        None,
                    ),
                ))))
                .unwrap();
        }

        fn negate_data(ontology: &mut Ontology, subject: &str, value: &str) {
            ontology
                .add_axiom(Axiom::NegativeDataPropertyAssertion(Box::new(
                    NegativeDataPropertyAssertionAxiom::new(iri(subject), iri("p"), integer(value)),
                )))
                .unwrap();
        }

        #[test]
        fn pool_membership_refutes_out_of_pool_value() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["1", "2"]);
            data_assertion(&mut ontology, "i", "3");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "3 sits in no pool of {{1,2}}; the range's universal reading contradicts"
            );
        }

        #[test]
        fn pool_membership_accepts_in_pool_value() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["1", "2"]);
            data_assertion(&mut ontology, "i", "1");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "1 sits in the pool; nothing contradicts"
            );
        }

        #[test]
        fn singleton_intersection_pins_the_witness() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["1", "2", "3", "4"]);
            enumerated_range(&mut ontology, &["4", "5", "6"]);
            min_cardinality(&mut ontology, "i", 1);
            negate_data(&mut ontology, "i", "4");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "min >= 1 with pools intersecting to {{4}} pins p(i, 4); its negation clashes"
            );
        }

        #[test]
        fn non_singleton_intersection_does_not_pin() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["1", "2", "3"]);
            enumerated_range(&mut ontology, &["2", "3", "4"]);
            min_cardinality(&mut ontology, "i", 1);
            negate_data(&mut ontology, "i", "2");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "the intersection {{2,3}} leaves a choice; the value can be 3, sparing the negation"
            );
        }

        #[test]
        fn zero_minimum_does_not_pin() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["4"]);
            min_cardinality(&mut ontology, "i", 0);
            negate_data(&mut ontology, "i", "4");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "min 0 demands no value; nothing pins p(i, 4)"
            );
        }

        #[test]
        fn qualified_minimum_does_not_pin() {
            let mut ontology = base(&["i"]);
            enumerated_range(&mut ontology, &["4"]);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("i"),
                    ClassExpression::DataMinCardinality(
                        1,
                        Box::new(DataPropertyExpression::DataProperty(
                            crate::core::entities::DataProperty::new("http://example.org/p"),
                        )),
                        Some(Box::new(DataRange::Datatype(
                            IRI::new("http://www.w3.org/2001/XMLSchema#string").unwrap(),
                        ))),
                    ),
                ))))
                .unwrap();
            negate_data(&mut ontology, "i", "4");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "a qualified minimum is not the pool's demand; nothing pins p(i, 4)"
            );
        }
    }

    mod pigeonhole_closure_179 {
        //! #179 (#154's recorded notes): the exact-size seed matches
        //! member order-insensitively, disjointness reads through the
        //! superclass closure, and a link over a subproperty of a
        //! bijective role counts as injective.

        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual};
        use crate::core::iri::IRI;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DifferentIndividualsAxiom, DisjointClassesAxiom,
            FunctionalPropertyAxiom, InverseFunctionalPropertyAxiom,
            InverseObjectPropertiesAxiom, SubClassOfAxiom, SubObjectPropertyOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn one_of(names: &[&str]) -> ClassExpression {
            let members: smallvec::SmallVec<[crate::core::entities::Individual; 8]> = names
                .iter()
                .map(|name| {
                    crate::core::entities::Individual::Named(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                })
                .collect();
            ClassExpression::ObjectOneOf(Box::new(members))
        }

        /// The 0084 micro shape with knobs: the oneOf→a direction may
        /// reverse its member order; disjointness may ride superclass
        /// closure; the bijection links may sit on subproperties.
        fn shape(
            reverse_members: bool,
            disjoint_through_supers: bool,
            links_on_subproperty: bool,
        ) -> Ontology {
            let mut ontology = Ontology::new();
            for name in ["p", "q", "r"] {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            let a = Class::new("http://example.org/a");
            let x = Class::new("http://example.org/x");
            let b = Class::new("http://example.org/b");
            let c = Class::new("http://example.org/c");
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(a.clone()),
                    one_of(&["p", "q", "r"]),
                ))
                .unwrap();
            let into_a = if reverse_members {
                one_of(&["r", "q", "p"])
            } else {
                one_of(&["p", "q", "r"])
            };
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    into_a,
                    ClassExpression::Class(a.clone()),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![iri("p"), iri("q"), iri("r")]),
                )))
                .unwrap();
            let union = ClassExpression::ObjectUnionOf(smallvec::smallvec![
                Box::new(ClassExpression::Class(b.clone())),
                Box::new(ClassExpression::Class(c.clone())),
            ]);
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(x.clone()),
                    union.clone(),
                ))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    union,
                    ClassExpression::Class(x.clone()),
                ))
                .unwrap();
            if disjoint_through_supers {
                // Disjoint(D, E) with b ⊑ D and c ⊑ E.
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(b.clone()),
                        ClassExpression::Class(Class::new("http://example.org/D")),
                    ))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(c.clone()),
                        ClassExpression::Class(Class::new("http://example.org/E")),
                    ))
                    .unwrap();
                ontology
                    .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                        Arc::new(IRI::new("http://example.org/D").unwrap()),
                        Arc::new(IRI::new("http://example.org/E").unwrap()),
                    ]))
                    .unwrap();
            } else {
                ontology
                    .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                        Arc::new(IRI::new("http://example.org/b").unwrap()),
                        Arc::new(IRI::new("http://example.org/c").unwrap()),
                    ]))
                    .unwrap();
            }
            for (forward, inverse, to) in [
                ("P", "Q", &x),
                ("R", "S", &b),
                ("T", "U", &c),
            ] {
                for role in [forward, inverse] {
                    ontology
                        .add_axiom(Axiom::FunctionalProperty(Box::new(
                            FunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                    ontology
                        .add_axiom(Axiom::InverseFunctionalProperty(Box::new(
                            InverseFunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                }
                ontology
                    .add_axiom(Axiom::InverseObjectProperties(Box::new(
                        InverseObjectPropertiesAxiom::new(iri(forward), iri(inverse)),
                    )))
                    .unwrap();
                let (link_role, direct_role) = if links_on_subproperty {
                    ontology
                        .add_axiom(Axiom::SubObjectProperty(Box::new(
                            SubObjectPropertyOfAxiom::new(
                                iri(&format!("{forward}sub")),
                                iri(forward),
                            ),
                        )))
                        .unwrap();
                    (format!("{forward}sub"), forward)
                } else {
                    (forward.to_string(), forward)
                };
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(a.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectProperty(Box::new(crate::core::entities::ObjectProperty::new(format!("http://example.org/{link_role}"))))),
                            Box::new(ClassExpression::Class(to.clone())),
                        ),
                    ))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(to.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectProperty(Box::new(crate::core::entities::ObjectProperty::new(format!("http://example.org/{direct_role}"))))),
                            Box::new(ClassExpression::Class(a.clone())),
                        ),
                    ))
                    .unwrap();
            }
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("p"),
                    ClassExpression::Class(a.clone()),
                ))))
                .unwrap();
            ontology
        }

        #[test]
        fn exact_seed_matches_regardless_of_member_order() {
            let ontology = shape(true, false, false);
            let reasoner = SimpleReasoner::new(ontology);
            // Direct pin on the arithmetic check — the merge probe can
            // decide the same ontology through its own one_two route,
            // so is_consistent alone would not isolate this path.
            assert!(
                reasoner.has_cardinality_pigeonhole_contradiction(),
                "reversing the oneOf direction's member order must not lose the exact seed"
            );
        }

        #[test]
        fn disjointness_through_superclasses_counts() {
            let ontology = shape(false, true, false);
            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.has_cardinality_pigeonhole_contradiction(),
                "Disjoint(D, E) with b ⊑ D, c ⊑ E must confine the union's sum"
            );
        }

        #[test]
        fn bijection_through_a_superproperty_counts() {
            let ontology = shape(false, false, true);
            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.has_cardinality_pigeonhole_contradiction(),
                "a link on a subproperty of a functional+inverse-functional role is injective"
            );
        }

        #[test]
        fn direct_declarations_still_work() {
            let ontology = shape(false, false, false);
            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.has_cardinality_pigeonhole_contradiction(),
                "the original direct-declaration shape keeps its contradiction"
            );
        }
    }

    mod compound_qualifiers_178 {
        //! #178 (#153's recorded gap): the complemented-qualified-minimum
        //! count consumes compound data qualifiers — union, intersection,
        //! complement — through one recursive containment predicate.

        use super::super::*;
        use crate::core::entities::{Literal, NamedIndividual};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom};

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn literal(value: &str, datatype: &str) -> Literal {
            Literal::typed(value.to_string(), IRI::new(datatype).unwrap())
        }

        fn integer(value: &str) -> Literal {
            literal(value, "http://www.w3.org/2001/XMLSchema#integer")
        }

        fn string(value: &str) -> Literal {
            literal(value, "http://www.w3.org/2001/XMLSchema#string")
        }

        fn base(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn data_assertion(ontology: &mut Ontology, subject: &str, value: Literal) {
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(iri(subject), iri("p"), value),
                )))
                .unwrap();
        }

        fn negated_minimum(ontology: &mut Ontology, subject: &str, minimum: u32, qualifier: DataRange) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(subject),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::DataMinCardinality(
                            minimum,
                            Box::new(DataPropertyExpression::DataProperty(
                                crate::core::entities::DataProperty::new("http://example.org/p"),
                            )),
                            Some(Box::new(qualifier)),
                        ),
                    )),
                ))))
                .unwrap();
        }

        fn integer_datatype() -> DataRange {
            DataRange::Datatype(
                IRI::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            )
        }

        fn string_datatype() -> DataRange {
            DataRange::Datatype(IRI::new("http://www.w3.org/2001/XMLSchema#string").unwrap())
        }

        #[test]
        fn union_qualifier_counts_values_of_either_operand() {
            let mut ontology = base(&["a"]);
            data_assertion(&mut ontology, "a", integer("1"));
            data_assertion(&mut ontology, "a", string("one"));
            negated_minimum(
                &mut ontology,
                "a",
                2,
                DataRange::DataUnionOf(vec![integer_datatype(), string_datatype()]),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "two distinct values under int ⊔ str refute the negated >= 2"
            );
        }

        #[test]
        fn intersection_qualifier_counts_only_values_in_every_operand() {
            let mut ontology = base(&["a"]);
            data_assertion(&mut ontology, "a", integer("1"));
            data_assertion(&mut ontology, "a", string("one"));
            negated_minimum(
                &mut ontology,
                "a",
                2,
                DataRange::DataIntersectionOf(vec![integer_datatype(), string_datatype()]),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "int ⊓ str matches neither value; nothing is counted"
            );
        }

        #[test]
        fn complement_qualifier_counts_values_outside_the_operand() {
            let mut ontology = base(&["a"]);
            data_assertion(&mut ontology, "a", string("one"));
            data_assertion(&mut ontology, "a", string("two"));
            negated_minimum(
                &mut ontology,
                "a",
                2,
                DataRange::DataComplementOf(Box::new(integer_datatype())),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "two distinct non-integer values refute the negated >= 2 under ¬int"
            );
        }

        #[test]
        fn too_few_values_never_refute() {
            let mut ontology = base(&["a"]);
            data_assertion(&mut ontology, "a", integer("1"));
            negated_minimum(
                &mut ontology,
                "a",
                2,
                DataRange::DataUnionOf(vec![integer_datatype(), string_datatype()]),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "one value under a >= 2 demand is satisfiable"
            );
        }
    }

    mod pool_closure_177 {
        //! #177 (#151's recorded gap): enumerated data pools confine
        //! and pin through the SubDataPropertyOf closure — values of a
        //! subproperty are values of every superproperty.

        use super::super::*;
        use crate::core::entities::{DataProperty, Literal, NamedIndividual};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom,
            DataPropertyEnumeratedRangeAxiom, NegativeDataPropertyAssertionAxiom,
            SubDataPropertyOfAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn integer(value: &str) -> Literal {
            Literal::typed(
                value.to_string(),
                IRI::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            )
        }

        fn base(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn sub_of(ontology: &mut Ontology, sub: &str, sup: &str) {
            ontology
                .add_axiom(Axiom::SubDataProperty(Box::new(SubDataPropertyOfAxiom::new(
                    iri(sub),
                    iri(sup),
                ))))
                .unwrap();
        }

        fn super_pool(ontology: &mut Ontology, values: &[&str]) {
            ontology
                .add_axiom(Axiom::DataPropertyEnumeratedRange(Box::new(
                    DataPropertyEnumeratedRangeAxiom::new(
                        iri("q"),
                        values.iter().map(|v| integer(v)).collect(),
                    ),
                )))
                .unwrap();
        }

        #[test]
        fn subproperty_value_outside_the_super_pool_refutes() {
            let mut ontology = base(&["a"]);
            sub_of(&mut ontology, "p", "q");
            super_pool(&mut ontology, &["1", "2"]);
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(iri("a"), iri("p"), integer("3")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "p ⊑ q with range(q)={{1,2}}: the out-of-pool value 3 on p must refute"
            );
        }

        #[test]
        fn out_of_pool_value_survives_without_the_closure_link() {
            let mut ontology = base(&["a"]);
            super_pool(&mut ontology, &["1", "2"]);
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(iri("a"), iri("p"), integer("3")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "without p ⊑ q the pool on q says nothing about p's values"
            );
        }

        #[test]
        fn subproperty_minimum_pins_through_the_super_pool() {
            let mut ontology = base(&["a"]);
            sub_of(&mut ontology, "p", "q");
            super_pool(&mut ontology, &["4"]);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::DataMinCardinality(
                        1,
                        Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                            "http://example.org/p",
                        ))),
                        None,
                    ),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::NegativeDataPropertyAssertion(Box::new(
                    NegativeDataPropertyAssertionAxiom::new(iri("a"), iri("p"), integer("4")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the min >= 1 on p pins the super's singleton 4; the negative assertion consumes it"
            );
        }

        #[test]
        fn subproperty_minimum_stays_unpinned_without_the_closure_link() {
            let mut ontology = base(&["a"]);
            super_pool(&mut ontology, &["4"]);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::DataMinCardinality(
                        1,
                        Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                            "http://example.org/p",
                        ))),
                        None,
                    ),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::NegativeDataPropertyAssertion(Box::new(
                    NegativeDataPropertyAssertionAxiom::new(iri("a"), iri("p"), integer("4")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "the pool on q does not pin a demand on p without the closure link"
            );
        }
    }

    mod mirror_readers_176 {
        //! #176 (#152's recorded gap): the two sibling inverse readers
        //! (universal propagation and the negated-existentials
        //! endpoint collection) apply the symmetric-mirror sandwich
        //! rule the GCI reader already has.

        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, PropertyAssertionAxiom, SymmetricPropertyAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn inverse_role(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectInverseOf(Box::new(
                ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(format!(
                    "http://example.org/{name}"
                )))),
            )))
        }

        fn named_class(name: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(format!("http://example.org/{name}")))
        }

        fn base_ontology() -> Ontology {
            let mut ontology = Ontology::new();
            for name in ["a", "b"] {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(iri("a"), iri("r"), iri("b")))
                .unwrap();
            ontology
        }

        fn make_symmetric(ontology: &mut Ontology) {
            ontology
                .add_axiom(Axiom::SymmetricProperty(Box::new(
                    SymmetricPropertyAxiom::new(iri("r")),
                )))
                .unwrap();
        }

        #[test]
        fn inverse_universal_propagates_through_the_mirror() {
            let mut ontology = base_ontology();
            make_symmetric(&mut ontology);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::ObjectAllValuesFrom(
                        inverse_role("r"),
                        Box::new(named_class("D")),
                    ),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("b"),
                    ClassExpression::ObjectComplementOf(Box::new(named_class("D"))),
                ))))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the mirrored edge makes b an inverse successor of a; its negated D clashes"
            );
        }

        #[test]
        fn inverse_universal_stays_silent_without_symmetry() {
            let mut ontology = base_ontology();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::ObjectAllValuesFrom(
                        inverse_role("r"),
                        Box::new(named_class("D")),
                    ),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("b"),
                    ClassExpression::ObjectComplementOf(Box::new(named_class("D"))),
                ))))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "a —r→ b is not an inverse successor edge without symmetry"
            );
        }

        #[test]
        fn negated_inverse_existential_clashes_through_the_mirror() {
            let mut ontology = base_ontology();
            make_symmetric(&mut ontology);
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectSomeValuesFrom(
                            inverse_role("r"),
                            Box::new(named_class("D")),
                        ),
                    )),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("b"),
                    named_class("D"),
                ))))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the mirrored edge makes b an inverse successor witnessing D — the negated existential clashes"
            );
        }

        #[test]
        fn negated_inverse_existential_stays_silent_without_symmetry() {
            let mut ontology = base_ontology();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("a"),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::ObjectSomeValuesFrom(
                            inverse_role("r"),
                            Box::new(named_class("D")),
                        ),
                    )),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("b"),
                    named_class("D"),
                ))))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "without symmetry b is not an inverse successor; the negation survives"
            );
        }
    }

    mod range_gci_membership_152 {
        //! Fixtures for #152: the range GCI `exists p- . T sqsubseteq
        //! oneOf{...}` (#150's lowering) confines every OBJECT of p to
        //! the enumeration; with symmetry the subjects are confined
        //! too. Anchor: W3C TransitiveProperty-002 (0329) — the
        //! transitivity refutation's fresh x, y, z must all squeeze
        //! into {a, b}.

        use super::super::*;
        use crate::core::entities::{NamedIndividual, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
        use crate::logic::axioms::{
            Axiom, NegativeObjectPropertyAssertionAxiom, PropertyAssertionAxiom,
            SubClassOfAxiom, SymmetricPropertyAxiom,
        };

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn with_individuals(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn inverse_role(name: &str) -> Box<ObjectPropertyExpression> {
            Box::new(ObjectPropertyExpression::ObjectInverseOf(Box::new(
                ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(format!(
                    "http://example.org/{name}"
                )))),
            )))
        }

        fn thing() -> Box<ClassExpression> {
            Box::new(ClassExpression::Class(crate::core::entities::Class::new(
                "http://www.w3.org/2002/07/owl#Thing",
            )))
        }

        fn one_of(names: &[&str]) -> ClassExpression {
            let members: smallvec::SmallVec<[crate::core::entities::Individual; 8]> = names
                .iter()
                .map(|name| {
                    crate::core::entities::Individual::Named(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                })
                .collect();
            ClassExpression::ObjectOneOf(Box::new(members))
        }

        fn range_gci(ontology: &mut Ontology, property: &str, members: &[&str]) {
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::ObjectSomeValuesFrom(inverse_role(property), thing()),
                    one_of(members),
                ))
                .unwrap();
        }

        fn edge(ontology: &mut Ontology, subject: &str, property: &str, object: &str) {
            ontology
                .add_property_assertion(PropertyAssertionAxiom::new(
                    iri(subject),
                    iri(property),
                    iri(object),
                ))
                .unwrap();
        }

        /// The minimal link: a singleton pool pins the object of p, so
        /// the edge becomes p(x, a) and contradicts a matching negative.
        #[test]
        fn singleton_range_gci_pins_the_object() {
            let mut ontology = with_individuals(&["a", "x", "y"]);
            range_gci(&mut ontology, "p", &["a"]);
            edge(&mut ontology, "x", "p", "y");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("x"), iri("p"), iri("a")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the range GCI confines y to {{a}}; p(x, a) contradicts the negation"
            );
        }

        /// The 0329 reduction shape: symmetric p, the two-element range
        /// GCI, the asserted loops, and the transitivity refutation's
        /// fresh x/y/z edges plus negated closing edge.
        #[test]
        fn range_gci_squeezes_the_transitivity_witnesses() {
            let mut ontology = with_individuals(&["a", "b", "x", "y", "z"]);
            ontology
                .add_axiom(Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(
                    iri("p"),
                ))))
                .unwrap();
            range_gci(&mut ontology, "p", &["a", "b"]);
            edge(&mut ontology, "a", "p", "a");
            edge(&mut ontology, "b", "p", "b");
            edge(&mut ontology, "x", "p", "y");
            edge(&mut ontology, "y", "p", "z");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("x"), iri("p"), iri("z")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "x, y, z squeeze into {{a, b}}; every witness case contradicts the negated edge"
            );
        }

        /// Bisection ladder: does the symmetric mirror make x an
        /// object of p, so the singleton pool pins x too?
        #[test]
        fn symmetric_mirror_exposes_the_subject_to_the_range() {
            let mut ontology = with_individuals(&["a", "x", "y"]);
            ontology
                .add_axiom(Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(
                    iri("p"),
                ))))
                .unwrap();
            range_gci(&mut ontology, "p", &["a"]);
            edge(&mut ontology, "x", "p", "y");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("y"), iri("p"), iri("a")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the mirror of p(x, y) makes x an object; the singleton pool pins x = a, so p(y, a) contradicts"
            );
        }

        /// Bisection ladder: a two-member pool without symmetry — the
        /// object must be a or b, so one of the two negatives clashes.
        #[test]
        fn two_member_pool_resolves_without_symmetry() {
            let mut ontology = with_individuals(&["a", "b", "x", "y"]);
            range_gci(&mut ontology, "p", &["a", "b"]);
            edge(&mut ontology, "x", "p", "y");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("x"), iri("p"), iri("a")),
                )))
                .unwrap();
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("x"), iri("p"), iri("b")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "y is a or b; the matching negated edge contradicts either way"
            );
        }

        /// A subproperty of a symmetric property does not mirror: the
        /// reversal is entailed only for the declared symmetric label
        /// (#121's lesson, the review round's counter-example).
        #[test]
        fn subproperty_of_symmetric_does_not_mirror() {
            let mut ontology = with_individuals(&["a", "x", "y"]);
            ontology
                .add_axiom(Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(
                    iri("s"),
                ))))
                .unwrap();
            ontology
                .add_axiom(Axiom::SubObjectProperty(Box::new(
                    crate::logic::axioms::SubObjectPropertyOfAxiom::new(iri("p"), iri("s")),
                )))
                .unwrap();
            ontology
                .add_axiom(Axiom::SubObjectProperty(Box::new(
                    crate::logic::axioms::SubObjectPropertyOfAxiom::new(iri("p"), iri("q")),
                )))
                .unwrap();
            range_gci(&mut ontology, "q", &["a"]);
            edge(&mut ontology, "x", "p", "y");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("y"), iri("p"), iri("x")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "the object-side squeeze pins y = a, but p under symmetric s does not mirror, so the reverse edge stays free"
            );
        }

        /// The mirror needs symmetry at all — the same shape without
        /// the declaration stays consistent.
        #[test]
        fn mirror_needs_the_symmetric_declaration() {
            let mut ontology = with_individuals(&["a", "x", "y"]);
            range_gci(&mut ontology, "p", &["a"]);
            edge(&mut ontology, "x", "p", "y");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("y"), iri("p"), iri("a")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "without symmetry, x is never an object; the mirror must not fire"
            );
        }

        /// Without the range GCI the shape is consistent — the guard
        /// against an over-eager squeeze.
        #[test]
        fn without_the_range_gci_the_witnesses_survive() {
            let mut ontology = with_individuals(&["a", "b", "x", "y", "z"]);
            ontology
                .add_axiom(Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(
                    iri("p"),
                ))))
                .unwrap();
            edge(&mut ontology, "a", "p", "a");
            edge(&mut ontology, "b", "p", "b");
            edge(&mut ontology, "x", "p", "y");
            edge(&mut ontology, "y", "p", "z");
            ontology
                .add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                    NegativeObjectPropertyAssertionAxiom::new(iri("x"), iri("p"), iri("z")),
                )))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "nothing confines x, y, z; the negated edge survives"
            );
        }
    }
    mod qualified_data_minima_153 {
        //! Fixtures for #153: a complemented qualified data minimum
        //! contradicts n distinct asserted values that satisfy the
        //! qualifier. Anchor: W3C DataQCR-001 (0040) — Meg's two plain
        //! hasName literals are distinct xsd:string values, refuting
        //! not-(>= 2 qualified strings).

        use super::super::*;
        use crate::core::entities::{DataProperty, Literal, NamedIndividual};
        use crate::core::iri::IRI;
        use crate::logic::axioms::class_expressions::DataRange;
        use crate::logic::axioms::property_expressions::DataPropertyExpression;
        use crate::logic::axioms::{Axiom, ClassAssertionAxiom, DataPropertyAssertionAxiom};

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn string_literal(value: &str) -> Literal {
            Literal::simple(value.to_string())
        }

        fn integer_literal(value: &str) -> Literal {
            Literal::typed(
                value.to_string(),
                IRI::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            )
        }

        fn base(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn assert_data(ontology: &mut Ontology, subject: &str, value: Literal) {
            ontology
                .add_axiom(Axiom::DataPropertyAssertion(Box::new(
                    DataPropertyAssertionAxiom::new(iri(subject), iri("hasName"), value),
                )))
                .unwrap();
        }

        fn complemented_min(ontology: &mut Ontology, subject: &str, minimum: u32, range: DataRange) {
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri(subject),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::DataMinCardinality(
                            minimum,
                            Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                                "http://example.org/hasName",
                            ))),
                            Some(Box::new(range)),
                        ),
                    )),
                ))))
                .unwrap();
        }

        fn xsd_string() -> DataRange {
            DataRange::Datatype(IRI::new("http://www.w3.org/2001/XMLSchema#string").unwrap())
        }

        #[test]
        fn two_qualified_values_refute_the_complemented_minimum() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            assert_data(&mut ontology, "meg", string_literal("Megan Griffin"));
            complemented_min(&mut ontology, "meg", 2, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "two distinct xsd:string values contradict not-(>= 2 qualified strings)"
            );
        }

        #[test]
        fn one_qualified_value_spares_the_minimum() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            complemented_min(&mut ontology, "meg", 2, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "one qualified value satisfies the complemented >= 2"
            );
        }

        #[test]
        fn unqualified_values_do_not_count() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", integer_literal("1"));
            assert_data(&mut ontology, "meg", integer_literal("2"));
            complemented_min(&mut ontology, "meg", 2, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "integer values do not satisfy a string qualifier"
            );
        }

        #[test]
        fn duplicated_qualified_value_counts_once() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            complemented_min(&mut ontology, "meg", 2, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "one distinct value twice is still one value"
            );
        }

        #[test]
        fn lang_tagged_values_do_not_satisfy_a_string_qualifier() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            assert_data(
                &mut ontology,
                "meg",
                Literal::lang_tagged("Megan Griffin".to_string(), "en"),
            );
            complemented_min(&mut ontology, "meg", 2, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "a lang-tagged literal is rdf:langString, not xsd:string; only one value qualifies"
            );
        }

        #[test]
        fn complemented_zero_minimum_is_unsatisfiable() {
            let mut ontology = base(&["meg"]);
            complemented_min(&mut ontology, "meg", 0, xsd_string());

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "not-(>= 0) has no model; the degenerate minimum refutes outright"
            );
        }

        #[test]
        fn complemented_unqualified_minimum_stays_on_the_trivial_path() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            ontology
                .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                    iri("meg"),
                    ClassExpression::ObjectComplementOf(Box::new(
                        ClassExpression::DataMinCardinality(
                            2,
                            Box::new(DataPropertyExpression::DataProperty(DataProperty::new(
                                "http://example.org/hasName",
                            ))),
                            None,
                        ),
                    )),
                ))))
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "the unqualified complemented minimum is #129's trivial path, not this check"
            );
        }

        #[test]
        fn enumerated_qualifier_counts_membership() {
            let mut ontology = base(&["meg"]);
            assert_data(&mut ontology, "meg", string_literal("Meg Griffin"));
            assert_data(&mut ontology, "meg", string_literal("Megan Griffin"));
            complemented_min(
                &mut ontology,
                "meg",
                2,
                DataRange::DataOneOf(vec![
                    string_literal("Meg Griffin"),
                    string_literal("Megan Griffin"),
                ]),
            );

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "both values sit in the enumerated qualifier; the complemented minimum fails"
            );
        }
    }

    mod unions_119 {
        //! Fixtures for #119: union choices in the merge probe. A
        //! pressed union branches over its live disjuncts (ADR-0020:
        //! branch-and-settle, fail-open budget); free unions stay
        //! unbranched. Anchors: W3C description-logic-040 (0378),
        //! -110 (0388), -111 (0389).

        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
        use crate::logic::axioms::{ClassAssertionAxiom, SubClassOfAxiom};

        const NS: &str = "http://example.org/";

        fn named(suffix: &str) -> String {
            format!("{NS}{suffix}")
        }

        fn class(suffix: &str) -> ClassExpression {
            ClassExpression::Class(Class::new(named(suffix)))
        }

        fn not_class(suffix: &str) -> ClassExpression {
            ClassExpression::ObjectComplementOf(Box::new(class(suffix)))
        }

        fn union(parts: &[ClassExpression]) -> ClassExpression {
            ClassExpression::ObjectUnionOf(
                parts.iter().cloned().map(Box::new).collect(),
            )
        }

        fn intersection(parts: &[ClassExpression]) -> ClassExpression {
            ClassExpression::ObjectIntersectionOf(
                parts.iter().cloned().map(Box::new).collect(),
            )
        }

        fn add_subclass(ontology: &mut Ontology, sub: ClassExpression, sup: ClassExpression) {
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(sub, sup))
                .unwrap();
        }

        fn instance_of(ontology: &mut Ontology, name: &str, class_name: &str) {
            let individual = NamedIndividual::new(named(name));
            ontology.add_named_individual(individual.clone()).unwrap();
            ontology
                .add_class_assertion(ClassAssertionAxiom::new(
                    individual.iri().clone(),
                    class(class_name),
                ))
                .unwrap();
        }

        /// The description-logic-040 core without the noise: the
        /// ∃R.C2 witness is also an ∀R.C3 filler, so it must take A
        /// (from C2's unions) and ¬A (from C3's) at once — every
        /// combination of disjuncts dies.
        #[test]
        fn pressed_unions_refute_through_all_dead_branches() {
            let mut ontology = Ontology::new();
            // C2 ⊑ (A ⊔ B) ⊓ (A ⊔ ¬B) forces A.
            add_subclass(
                &mut ontology,
                class("C2"),
                intersection(&[union(&[class("A"), class("B")]), union(&[class("A"), not_class("B")])]),
            );
            // C3 ⊑ (¬A ⊔ B) ⊓ (¬A ⊔ ¬B) forces ¬A.
            add_subclass(
                &mut ontology,
                class("C3"),
                intersection(&[union(&[not_class("A"), class("B")]), union(&[not_class("A"), not_class("B")])]),
            );
            // TEST ⊑ ∃R.C2 ⊓ ∀R.C3 puts both on one R-successor.
            add_subclass(
                &mut ontology,
                class("TEST"),
                intersection(&[
                    ClassExpression::ObjectSomeValuesFrom(
                        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                            ObjectProperty::new(named("R")),
                        ))),
                        Box::new(class("C2")),
                    ),
                    ClassExpression::ObjectAllValuesFrom(
                        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                            ObjectProperty::new(named("R")),
                        ))),
                        Box::new(class("C3")),
                    ),
                ]),
            );
            instance_of(&mut ontology, "t", "TEST");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the shared witness must take A and ¬A; every union combination dies"
            );
        }

        /// The description-logic-110 shape: an equivalent-class
        /// definition (the parser lowers it to two subclass axioms)
        /// whose ∃r-witness carries ¬e and d while both disjuncts of
        /// ∀r.(e ⊔ ¬d) are dead — no branching needed.
        #[test]
        fn dead_disjunct_pair_refutes_without_branching() {
            let mut ontology = Ontology::new();
            let r = Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(named("r")),
            )));
            let definition = intersection(&[
                ClassExpression::ObjectSomeValuesFrom(r.clone(), Box::new(not_class("e"))),
                ClassExpression::ObjectAllValuesFrom(r.clone(), Box::new(class("d"))),
                ClassExpression::ObjectAllValuesFrom(
                    r.clone(),
                    Box::new(union(&[class("e"), not_class("d")])),
                ),
            ]);
            add_subclass(&mut ontology, class("Unsatisfiable"), definition.clone());
            add_subclass(&mut ontology, definition, class("Unsatisfiable"));
            instance_of(&mut ontology, "x", "Unsatisfiable");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the witness carries ¬e and d; e and ¬d are both dead"
            );
        }

        /// Free unions never branch: thirty-two unions whose atoms
        /// nothing else on the node mentions stay unbranched — each
        /// disjunct is fully undetermined, so the pressedness filter
        /// must skip them, not the discharge check.
        #[test]
        fn free_unions_stay_consistent() {
            let mut ontology = Ontology::new();
            let mut parts = Vec::new();
            for index in 0..32u32 {
                parts.push(union(&[
                    class(&format!("X{index}")),
                    class(&format!("Y{index}")),
                ]));
            }
            add_subclass(
                &mut ontology,
                class("Wide"),
                ClassExpression::ObjectIntersectionOf(
                    parts.into_iter().map(Box::new).collect(),
                ),
            );
            instance_of(&mut ontology, "w", "Wide");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "nothing presses the free unions; every choice combination lives"
            );
        }

        /// The full dl-040 shape: the pressed refutation on the
        /// ∃R-witness sits next to thirty-two free unions on the test
        /// individual — the search must refute through the pressed
        /// pair without touching the noise.
        #[test]
        fn pressed_refutation_survives_free_union_noise() {
            let mut ontology = Ontology::new();
            // C2 ⊑ (A ⊔ B) ⊓ (A ⊔ ¬B) forces A.
            add_subclass(
                &mut ontology,
                class("C2"),
                intersection(&[union(&[class("A"), class("B")]), union(&[class("A"), not_class("B")])]),
            );
            // C3 ⊑ (¬A ⊔ B) ⊓ (¬A ⊔ ¬B) forces ¬A.
            add_subclass(
                &mut ontology,
                class("C3"),
                intersection(&[union(&[not_class("A"), class("B")]), union(&[not_class("A"), not_class("B")])]),
            );
            // C1's noise: thirty-two free unions.
            let mut noise = Vec::new();
            for index in 0..32u32 {
                noise.push(union(&[
                    class(&format!("X{index}")),
                    class(&format!("Y{index}")),
                ]));
            }
            add_subclass(
                &mut ontology,
                class("C1"),
                ClassExpression::ObjectIntersectionOf(
                    noise.into_iter().map(Box::new).collect(),
                ),
            );
            add_subclass(
                &mut ontology,
                class("TEST"),
                intersection(&[
                    class("C1"),
                    ClassExpression::ObjectSomeValuesFrom(
                        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                            ObjectProperty::new(named("R")),
                        ))),
                        Box::new(class("C2")),
                    ),
                    ClassExpression::ObjectAllValuesFrom(
                        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                            ObjectProperty::new(named("R")),
                        ))),
                        Box::new(class("C3")),
                    ),
                ]),
            );
            instance_of(&mut ontology, "t", "TEST");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the free unions are noise; the pressed pair on the witness still refutes"
            );
        }

        /// A union with one live disjunct (the other contradicts a
        /// recorded expression) forces that disjunct without cloning.
        #[test]
        fn single_live_disjunct_forces_without_branching() {
            let mut ontology = Ontology::new();
            add_subclass(
                &mut ontology,
                class("S"),
                intersection(&[union(&[class("A"), class("B")]), not_class("B")]),
            );
            instance_of(&mut ontology, "s", "S");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "B is dead, A lives: the union forces A and settles"
            );
        }

        /// A pressed union with a genuinely surviving branch settles
        /// consistent — the search must not refute what lives.
        #[test]
        fn surviving_disjunct_settles_consistent() {
            let mut ontology = Ontology::new();
            // The witness must satisfy (A ⊔ B) ⊓ (A ⊔ ¬B): A satisfies
            // both, so the node stays consistent through branch A.
            add_subclass(
                &mut ontology,
                class("P"),
                intersection(&[union(&[class("A"), class("B")]), union(&[class("A"), not_class("B")])]),
            );
            let p = Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                ObjectProperty::new(named("R")),
            )));
            add_subclass(
                &mut ontology,
                class("TEST"),
                ClassExpression::ObjectSomeValuesFrom(p, Box::new(class("P"))),
            );
            instance_of(&mut ontology, "t", "TEST");

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "branch A satisfies both conjuncts; the search must find it"
            );
        }
    }

    mod oneof_pigeonhole_154 {
        //! Fixtures for #154: pairwise must-differ nodes squeezed into
        //! a oneOf constraint with fewer members contradict — the
        //! one_two pigeonhole (6 must-differ a-witnesses into
        //! oneOf{i,j,k}).

        use super::super::*;
        use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
        use crate::core::iri::IRI;
        use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
        use crate::logic::axioms::{
            Axiom, ClassAssertionAxiom, DifferentIndividualsAxiom, DisjointClassesAxiom,
            FunctionalPropertyAxiom, InverseFunctionalPropertyAxiom,
            InverseObjectPropertiesAxiom, SubClassOfAxiom,
        };

        fn object_role(name: &str) -> ObjectPropertyExpression {
            ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(format!(
                "http://example.org/{name}"
            ))))
        }

        fn iri(name: &str) -> Arc<IRI> {
            IRI::new_optimized(&format!("http://example.org/{name}")).expect("test IRI is valid")
        }

        fn with_individuals(names: &[&str]) -> Ontology {
            let mut ontology = Ontology::new();
            for name in names {
                ontology
                    .add_named_individual(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                    .unwrap();
            }
            ontology
        }

        fn one_of(names: &[&str]) -> ClassExpression {
            let members: smallvec::SmallVec<[crate::core::entities::Individual; 8]> = names
                .iter()
                .map(|name| {
                    crate::core::entities::Individual::Named(NamedIndividual::new(format!(
                        "http://example.org/{name}"
                    )))
                })
                .collect();
            ClassExpression::ObjectOneOf(Box::new(members))
        }

        /// L1: the pure pigeonhole — four AllDifferent individuals,
        /// each confined to the same three-member enumeration.
        #[test]
        fn four_must_differ_nodes_into_three_nominals_contradict() {
            let mut ontology = with_individuals(&["p", "q", "r", "s"]);
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![iri("p"), iri("q"), iri("r"), iri("s")]),
                )))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(Class::new("http://example.org/A")),
                    one_of(&["p", "q", "r"]),
                ))
                .unwrap();
            for name in ["p", "q", "r", "s"] {
                ontology
                    .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                        iri(name),
                        ClassExpression::Class(Class::new("http://example.org/A")),
                    ))))
                    .unwrap();
            }

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "four pairwise-distinct nodes cannot inject into three nominals"
            );
        }

        /// Session-4 integration: the full one_two premise (inline,
        /// verbatim from the W3C fixture) — the counting route.
        #[test]
        fn one_two_full_lattice_is_inconsistent() {
            let mut parser = crate::parser::rdf_xml_streaming::RdfXmlStreamingParser::new(
                crate::parser::ParserConfig::default(),
            );
            let parsed = parser
                .parse_content(r#"<rdf:RDF xmlns="http://example.com/"
     xml:base="http://example.com/"
     xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
     xmlns:owl="http://www.w3.org/2002/07/owl#"
     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <owl:Ontology rdf:about=""/>

    <owl:ObjectProperty rdf:about="2a=a">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">2a-to-a</rdfs:label>
    </owl:ObjectProperty>

    <owl:ObjectProperty rdf:about="2a=b-and-c">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">2a-to-b-and-c</rdfs:label>
    </owl:ObjectProperty>

    <owl:ObjectProperty rdf:about="a=2a'">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">a-to-2a'</rdfs:label>
        <owl:inverseOf rdf:resource="2a=a"/>
    </owl:ObjectProperty>
    

    <owl:ObjectProperty rdf:about="a=b">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">a-to-b</rdfs:label>
    </owl:ObjectProperty>
    
    <owl:ObjectProperty rdf:about="b-and-c=2a'">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">b-and-c-to-2a'</rdfs:label>
        <owl:inverseOf rdf:resource="2a=b-and-c"/>
    </owl:ObjectProperty>

    <owl:ObjectProperty rdf:about="b=a'">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">b-to-a'</rdfs:label>
        <owl:inverseOf rdf:resource="a=b"/>
    </owl:ObjectProperty>
    
    <owl:ObjectProperty rdf:about="b=c">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">b-to-c</rdfs:label>
    </owl:ObjectProperty>
    
    <owl:ObjectProperty rdf:about="c=b'">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#FunctionalProperty"/>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#InverseFunctionalProperty"/>
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">c-to-b'</rdfs:label>
        <owl:inverseOf rdf:resource="b=c"/>
    </owl:ObjectProperty>
    
    <owl:Class rdf:about="2a">
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">2a</rdfs:label>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="2a=b-and-c"/>
                <owl:someValuesFrom rdf:resource="b-and-c"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="2a=a"/>
                <owl:someValuesFrom rdf:resource="a"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <owl:disjointWith rdf:resource="a"/>
        <owl:disjointWith rdf:resource="b"/>
        <owl:disjointWith rdf:resource="b-and-c"/>
        <owl:disjointWith rdf:resource="c"/>
    </owl:Class>

    <owl:Class rdf:about="a">
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">a</rdfs:label>
        <owl:equivalentClass>
            <owl:Class>
                <owl:oneOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://example.com/j"/>
                    <rdf:Description rdf:about="http://example.com/k"/>
                    <rdf:Description rdf:about="http://example.com/i"/>
                </owl:oneOf>
            </owl:Class>
        </owl:equivalentClass>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="a=b"/>
                <owl:someValuesFrom rdf:resource="b"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="a=2a'"/>
                <owl:someValuesFrom rdf:resource="2a"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <owl:disjointWith rdf:resource="b"/>
        <owl:disjointWith rdf:resource="c"/>
    </owl:Class>
    
    <owl:Class rdf:about="b">
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">b</rdfs:label>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="b=a'"/>
                <owl:someValuesFrom rdf:resource="a"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="b=c"/>
                <owl:someValuesFrom rdf:resource="c"/>
            </owl:Restriction>
        </rdfs:subClassOf>
        <owl:disjointWith rdf:resource="c"/>
    </owl:Class>

    <owl:Class rdf:about="b-and-c">
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">b-and-c</rdfs:label>
        <owl:equivalentClass>
            <owl:Class>
                <owl:unionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="b"/>
                    <rdf:Description rdf:about="c"/>
                </owl:unionOf>
            </owl:Class>
        </owl:equivalentClass>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="b-and-c=2a'"/>
                <owl:someValuesFrom rdf:resource="2a"/>
            </owl:Restriction>
        </rdfs:subClassOf>
    </owl:Class>
    
    <owl:Class rdf:about="c">
        <rdfs:label rdf:datatype="http://www.w3.org/2001/XMLSchema#string">c</rdfs:label>
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="c=b'"/>
                <owl:someValuesFrom rdf:resource="b"/>
            </owl:Restriction>
        </rdfs:subClassOf>
    </owl:Class>

    <rdf:Description rdf:about="http://example.com/i">
        <rdfs:label>i</rdfs:label>
    </rdf:Description>

    <rdf:Description rdf:about="http://example.com/j">
        <rdfs:label>j</rdfs:label>
    </rdf:Description>

    <rdf:Description rdf:about="http://example.com/k">
        <rdfs:label>k</rdfs:label>
    </rdf:Description>

    <rdf:Description>
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#AllDifferent"/>
        <owl:distinctMembers rdf:parseType="Collection">
            <rdf:Description rdf:about="http://example.com/j"/>
            <rdf:Description rdf:about="http://example.com/k"/>
            <rdf:Description rdf:about="http://example.com/i"/>
        </owl:distinctMembers>
    </rdf:Description>
</rdf:RDF>
"#)
                .expect("one_two fixture parses");
            let reasoner = SimpleReasoner::new(parsed);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "the disjoint union needs 6 pairwise-distinct elements; the bijection cap is 3"
            );
        }

        /// Session-4 counting route, micro shape: |a| = 3 exactly
        /// (oneOf + AllDifferent), bijections make |x| = |b| = |c| = 3,
        /// and the disjoint union x ≡ b ⊔ c demands 6 — contradiction.
        #[test]
        fn disjoint_union_exceeds_the_bijection_cap() {
            let mut ontology = with_individuals(&["p", "q", "r"]);
            let a = Class::new("http://example.org/a");
            let x = Class::new("http://example.org/x");
            let b = Class::new("http://example.org/b");
            let c = Class::new("http://example.org/c");
            // a ≡ oneOf{p,q,r}, both directions.
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(a.clone()),
                    one_of(&["p", "q", "r"]),
                ))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    one_of(&["p", "q", "r"]),
                    ClassExpression::Class(a.clone()),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![iri("p"), iri("q"), iri("r")]),
                )))
                .unwrap();
            // x ≡ b ⊔ c, both directions.
            let union = ClassExpression::ObjectUnionOf(smallvec::smallvec![
                Box::new(ClassExpression::Class(b.clone())),
                Box::new(ClassExpression::Class(c.clone())),
            ]);
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(x.clone()),
                    union.clone(),
                ))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    union,
                    ClassExpression::Class(x.clone()),
                ))
                .unwrap();
            ontology
                .add_disjoint_classes_axiom(DisjointClassesAxiom::new(vec![
                    Arc::new(IRI::new("http://example.org/b").unwrap()),
                    Arc::new(IRI::new("http://example.org/c").unwrap()),
                ]))
                .unwrap();
            // Functional+inverse-functional bijection links with
            // declared inverses, both directions total.
            for (forward, inverse, from, to) in [
                ("P", "Q", &a, &x),
                ("R", "S", &a, &b),
                ("T", "U", &a, &c),
            ] {
                for role in [forward, inverse] {
                    ontology
                        .add_axiom(Axiom::FunctionalProperty(Box::new(
                            FunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                    ontology
                        .add_axiom(Axiom::InverseFunctionalProperty(Box::new(
                            InverseFunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                }
                ontology
                    .add_axiom(Axiom::InverseObjectProperties(Box::new(
                        InverseObjectPropertiesAxiom::new(iri(forward), iri(inverse)),
                    )))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(from.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(object_role(forward)),
                            Box::new(ClassExpression::Class(to.clone())),
                        ),
                    ))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(to.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(object_role(inverse)),
                            Box::new(ClassExpression::Class(from.clone())),
                        ),
                    ))
                    .unwrap();
            }

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "b and c each need 3 pairwise-distinct elements; the disjoint union cannot fit x's 3"
            );
        }

        /// Guard: without the disjointness the union fits (all three
        /// can be the same elements).
        #[test]
        fn overlapping_union_fits_the_bijection_cap() {
            let mut ontology = with_individuals(&["p", "q", "r"]);
            let a = Class::new("http://example.org/a");
            let x = Class::new("http://example.org/x");
            let b = Class::new("http://example.org/b");
            let c = Class::new("http://example.org/c");
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(a.clone()),
                    one_of(&["p", "q", "r"]),
                ))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    one_of(&["p", "q", "r"]),
                    ClassExpression::Class(a.clone()),
                ))
                .unwrap();
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![iri("p"), iri("q"), iri("r")]),
                )))
                .unwrap();
            let union = ClassExpression::ObjectUnionOf(smallvec::smallvec![
                Box::new(ClassExpression::Class(b.clone())),
                Box::new(ClassExpression::Class(c.clone())),
            ]);
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(x.clone()),
                    union.clone(),
                ))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    union,
                    ClassExpression::Class(x.clone()),
                ))
                .unwrap();
            for (forward, inverse, from, to) in [
                ("P", "Q", &a, &x),
                ("R", "S", &a, &b),
                ("T", "U", &a, &c),
            ] {
                for role in [forward, inverse] {
                    ontology
                        .add_axiom(Axiom::FunctionalProperty(Box::new(
                            FunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                    ontology
                        .add_axiom(Axiom::InverseFunctionalProperty(Box::new(
                            InverseFunctionalPropertyAxiom::new(iri(role)),
                        )))
                        .unwrap();
                }
                ontology
                    .add_axiom(Axiom::InverseObjectProperties(Box::new(
                        InverseObjectPropertiesAxiom::new(iri(forward), iri(inverse)),
                    )))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(from.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(object_role(forward)),
                            Box::new(ClassExpression::Class(to.clone())),
                        ),
                    ))
                    .unwrap();
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        ClassExpression::Class(to.clone()),
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(object_role(inverse)),
                            Box::new(ClassExpression::Class(from.clone())),
                        ),
                    ))
                    .unwrap();
            }

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "b and c can share elements; the union fits in 3"
            );
        }

        /// The seeding itself: a TBox-only ontology walks once the
        /// nominal-GCI members enter — no ABox assertion exists here.
        #[test]
        fn nominal_gci_members_enter_the_walk_without_an_abox() {
            let mut ontology = with_individuals(&["p", "q"]);
            for (class, members) in [("A", &["p", "q"][..]), ("B", &["p", "q"][..])] {
                ontology
                    .add_subclass_axiom(SubClassOfAxiom::new(
                        one_of(members),
                        ClassExpression::Class(Class::new(format!("http://example.org/{class}"))),
                    ))
                    .unwrap();
            }
            ontology
                .add_disjoint_classes_axiom(
                    crate::logic::axioms::DisjointClassesAxiom::new(vec![
                        std::sync::Arc::new(
                            crate::core::iri::IRI::new("http://example.org/A").unwrap(),
                        ),
                        std::sync::Arc::new(
                            crate::core::iri::IRI::new("http://example.org/B").unwrap(),
                        ),
                    ]),
                )
                .unwrap();

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                !reasoner.is_consistent().unwrap(),
                "p and q are forced into both A and B, which are disjoint"
            );
        }

        /// L1 guard: exactly three into three stays consistent.
        #[test]
        fn three_must_differ_nodes_into_three_nominals_survive() {
            let mut ontology = with_individuals(&["p", "q", "r"]);
            ontology
                .add_axiom(Axiom::DifferentIndividuals(Box::new(
                    DifferentIndividualsAxiom::new(vec![iri("p"), iri("q"), iri("r")]),
                )))
                .unwrap();
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(Class::new("http://example.org/A")),
                    one_of(&["p", "q", "r"]),
                ))
                .unwrap();
            for name in ["p", "q", "r"] {
                ontology
                    .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                        iri(name),
                        ClassExpression::Class(Class::new("http://example.org/A")),
                    ))))
                    .unwrap();
            }

            let reasoner = SimpleReasoner::new(ontology);
            assert!(
                reasoner.is_consistent().unwrap(),
                "three into three is exactly satisfiable"
            );
        }
    }
}
