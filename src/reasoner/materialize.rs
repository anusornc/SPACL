//! Deterministic OWL 2 RL closure materialization for the `materialize` CLI.
//!
//! Implements the instance-relevant OWL 2 RL fragment (rule ids follow
//! OWL 2 Profiles section 4.3): scm-sco, scm-eqc1/2, cax-sco, scm-spo,
//! scm-eqp1/2, prp-spo1, prp-dom, prp-rng, prp-symp, prp-trp, and the
//! data-property domain rule. Axioms outside the fragment are counted in
//! the stats and left inert — mirroring the derived-oracle procedure's
//! uncovered-axiom inventory — and are never silently reinterpreted.
//!
//! Output is N-Triples over fully rendered RDF terms, collected in sorted
//! order, so identical input and binary reproduce identical output bytes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::core::ontology::Ontology;
use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::types::PropertyAssertionObject;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUB_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUB_PROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROP: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_REFLEXIVE: &str = "http://www.w3.org/2002/07/owl#ReflexiveProperty";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";

/// A named class whose superclass is a class expression: the engine-side
/// mirror of the derivation core's v2 construct rules. Each variant records
/// the premises needed to derive membership in the named class (or, for the
/// someValuesFrom/allValuesFrom fillers, in the filler class).
///
/// The shapes mirror exactly what the derivation core's rule table can
/// derive: hasSelf, exact-1 and min-3 qualified cardinality restriction
/// nodes can be typed as intersection members (cls-hs / cls-qc1 / cls-mqc3
/// derive membership of the restriction itself); hasValue restrictions can
/// only serve as a direct superclass (cls-hv2); someValuesFrom derives the
/// subject's filler typing, not the class; intersection members beyond one
/// named class plus one restriction stay out of the fragment.
enum RestrictionPremise {
    /// (u p v) for the recorded value v
    HasValue(Term, Term),
    /// (u p u)
    HasSelf(Term),
    /// (u p v) with (v:D)
    ExactQualifiedOne(Term, Term),
    /// three distinct (u p v_i) with (v_i:D)
    MinQualifiedThree(Term, Term),
}

enum ExpressionRule {
    /// C ⊑ restriction: satisfying the premise derives u:C (hasValue) or
    /// u:C over the restriction node itself (hasSelf, cardinalities).
    Direct(Term, RestrictionPremise),
    /// C ⊑ d ⊓ restriction: (u:d) and the premise derive u:C.
    Intersection(Term, RestrictionPremise, Term),
    /// C ⊑ ∃p.D: (u p v) derives u:D (cls-svf1 derives the subject's
    /// filler typing, not membership in C).
    SomeValuesFiller(Term, Term),
    /// C ⊑ ∀p.D: (u:C) and (u p v) derive v:D.
    AllValuesFiller(Term, Term, Term),
}
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// A rendered N-Triples term: `<iri>`, `_:label`, or a literal form.
pub type Term = String;
/// A rendered N-Triples triple: subject, predicate, object.
pub type Triple = [Term; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterializeStats {
    pub base_triples: usize,
    pub derived_triples: usize,
    pub closure_triples: usize,
    pub out_of_fragment_axioms: usize,
}

fn iri_term(iri: &crate::core::iri::IRI) -> Term {
    let value = iri.as_str();
    // The parsers encode anonymous individuals as IRIs with a "_:" prefix;
    // in N-Triples those must serialize as blank-node labels, never as IRIs.
    if let Some(label) = value.strip_prefix("_:") {
        return bnode_term(label);
    }
    format!("<{value}>")
}

fn bnode_term(node_id: &str) -> Term {
    let sanitized: String = node_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    format!("_:{sanitized}")
}

fn escape_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn literal_term(literal: &crate::core::entities::Literal) -> Term {
    let escaped = escape_literal(literal.lexical_form());
    if let Some(language) = literal.language_tag() {
        format!("\"{escaped}\"@{language}")
    } else if literal.datatype().as_str() == XSD_STRING {
        format!("\"{escaped}\"")
    } else {
        format!("\"{escaped}\"^^<{}>", literal.datatype().as_str())
    }
}

fn named_class(expression: &ClassExpression) -> Option<&crate::core::iri::IRI> {
    match expression {
        ClassExpression::Class(class) => Some(class.iri()),
        _ => None,
    }
}

fn individual_term(individual: &crate::core::entities::Individual) -> Term {
    match individual {
        crate::core::entities::Individual::Named(named) => iri_term(named.iri()),
        crate::core::entities::Individual::Anonymous(anonymous) => {
            bnode_term(anonymous.node_id())
        }
    }
}

/// The restriction premises the fragment can evaluate, extracted from a
/// class expression; `None` for anything outside the fragment.
fn restriction_premise(expression: &ClassExpression) -> Option<RestrictionPremise> {
    let named_property = |pe: &crate::logic::axioms::property_expressions::ObjectPropertyExpression| {
        pe.as_named().map(|p| iri_term(p.iri()))
    };
    match expression {
        ClassExpression::ObjectHasValue(pe, individual) => {
            named_property(pe).map(|property| {
                RestrictionPremise::HasValue(property, individual_term(individual))
            })
        }
        ClassExpression::ObjectHasSelf(pe) => {
            named_property(pe).map(RestrictionPremise::HasSelf)
        }
        ClassExpression::ObjectExactCardinality(1, pe, Some(qualifier))
            if qualifier.as_named().is_some() =>
        {
            named_property(pe).map(|property| {
                RestrictionPremise::ExactQualifiedOne(
                    property,
                    iri_term(qualifier.as_named().unwrap().iri()),
                )
            })
        }
        ClassExpression::ObjectMinCardinality(3, pe, Some(qualifier))
            if qualifier.as_named().is_some() =>
        {
            named_property(pe).map(|property| {
                RestrictionPremise::MinQualifiedThree(
                    property,
                    iri_term(qualifier.as_named().unwrap().iri()),
                )
            })
        }
        _ => None,
    }
}

/// Materialize the RL closure of an ontology as sorted rendered N-Triples.
/// The individuals whose property assertions satisfy a restriction premise.
fn individuals_satisfying(
    premise: &RestrictionPremise,
    assertions: &BTreeSet<(Term, Term, Term)>,
    types: &BTreeMap<Term, BTreeSet<Term>>,
) -> Vec<Term> {
    let typed = |individual: &Term, class: &Term| {
        types
            .get(individual)
            .is_some_and(|classes| classes.contains(class))
    };
    match premise {
        RestrictionPremise::HasValue(property, value) => {
            let mut satisfied: Vec<Term> = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for (subject, prop, object) in assertions {
                if prop == property && object == value && seen.insert(subject.clone()) {
                    satisfied.push(subject.clone());
                }
            }
            satisfied
        }
        RestrictionPremise::HasSelf(property) => {
            let mut satisfied: Vec<Term> = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for (subject, prop, object) in assertions {
                if prop == property && subject == object && seen.insert(subject.clone()) {
                    satisfied.push(subject.clone());
                }
            }
            satisfied
        }
        RestrictionPremise::ExactQualifiedOne(property, filler) => {
            let mut satisfied: Vec<Term> = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for (subject, prop, object) in assertions {
                if prop == property && typed(object, filler) && seen.insert(subject.clone()) {
                    satisfied.push(subject.clone());
                }
            }
            satisfied
        }
        RestrictionPremise::MinQualifiedThree(property, filler) => {
            let mut witnesses: std::collections::BTreeMap<Term, std::collections::BTreeSet<Term>> =
                std::collections::BTreeMap::new();
            for (subject, prop, object) in assertions {
                if prop == property && typed(object, filler) {
                    witnesses
                        .entry(subject.clone())
                        .or_default()
                        .insert(object.clone());
                }
            }
            witnesses
                .into_iter()
                .filter(|(_, values)| values.len() >= 3)
                .map(|(subject, _)| subject)
                .collect()
        }
    }
}

/// The full fragment: every rule the engine implements.
pub fn materialize_rl(ontology: &Ontology) -> (BTreeSet<Triple>, MaterializeStats) {
    materialize_with_options(ontology, true, true)
}

/// Profile-scoped materialization: the rule set mirrors the derivation
/// core's per-profile v2 tables so verdict runs compare like with like.
/// QL (DL-Lite_R) can express neither role reflexivity nor individual
/// equality, so the QL scope drops prp-ref and the eq-* rules the way the
/// QL rule table does; every other profile keeps the full fragment.
pub fn materialize_for_profile(
    ontology: &Ontology,
    profile: &str,
) -> (BTreeSet<Triple>, MaterializeStats) {
    let is_ql = profile == "QL";
    materialize_with_options(ontology, !is_ql, !is_ql)
}

fn materialize_with_options(
    ontology: &Ontology,
    include_reflexive: bool,
    include_equality: bool,
) -> (BTreeSet<Triple>, MaterializeStats) {
    let mut base: BTreeSet<Triple> = BTreeSet::new();
    let mut out_of_fragment: usize = 0;

    // --- Class hierarchy -------------------------------------------------
    let mut subclass: BTreeMap<Term, BTreeSet<Term>> = BTreeMap::new();
    let mut expression_rules: Vec<ExpressionRule> = Vec::new();
    for axiom in ontology.subclass_axioms() {
        match (named_class(axiom.sub_class()), axiom.super_class()) {
            (Some(sub), sup) if named_class(sup).is_some() => {
                let (sub_t, sup_t) = (iri_term(sub), iri_term(named_class(sup).unwrap()));
                base.insert([sub_t.clone(), term(RDFS_SUB_CLASS), sup_t.clone()]);
                subclass.entry(sub_t).or_default().insert(sup_t);
            }
            (Some(sub), expression) => {
                let class = iri_term(sub);
                match restriction_premise(expression) {
                    Some(premise) => {
                        expression_rules.push(ExpressionRule::Direct(class, premise));
                    }
                    None => match expression {
                        ClassExpression::ObjectIntersectionOf(operands) if operands.len() == 2 => {
                            let named_member = operands
                                .iter()
                                .find_map(|op| op.as_named().map(|c| iri_term(c.iri())));
                            let premise = operands
                                .iter()
                                .find_map(|op| restriction_premise(op));
                            match (named_member, premise) {
                                (Some(member), Some(premise)) => {
                                    expression_rules.push(ExpressionRule::Intersection(
                                        class, premise, member,
                                    ));
                                }
                                _ => out_of_fragment += 1,
                            }
                        }
                        ClassExpression::ObjectSomeValuesFrom(pe, filler)
                            if pe.as_named().is_some() && filler.as_named().is_some() =>
                        {
                            let property = iri_term(pe.as_named().unwrap().iri());
                            let filler = iri_term(filler.as_named().unwrap().iri());
                            expression_rules.push(ExpressionRule::SomeValuesFiller(
                                property, filler,
                            ));
                        }
                        ClassExpression::ObjectAllValuesFrom(pe, filler)
                            if pe.as_named().is_some() && filler.as_named().is_some() =>
                        {
                            let property = iri_term(pe.as_named().unwrap().iri());
                            let filler = iri_term(filler.as_named().unwrap().iri());
                            expression_rules.push(ExpressionRule::AllValuesFiller(
                                class, property, filler,
                            ));
                        }
                        _ => out_of_fragment += 1,
                    },
                }
            }
            _ => out_of_fragment += 1,
        }
    }

    let mut equivalent_classes: BTreeSet<(Term, Term)> = BTreeSet::new();
    for axiom in ontology.equivalent_classes_axioms() {
        let members = axiom.classes();
        if members.len() < 2 {
            out_of_fragment += 1;
            continue;
        }
        for pair in members.windows(2) {
            let (left, right) = (iri_term(&pair[0]), iri_term(&pair[1]));
            base.insert([left.clone(), term(OWL_EQUIVALENT_CLASS), right.clone()]);
            equivalent_classes.insert((left.clone(), right.clone()));
            equivalent_classes.insert((right, left));
        }
    }

    // --- Property hierarchy and characteristics ---------------------------
    let mut subproperty: BTreeMap<Term, BTreeSet<Term>> = BTreeMap::new();
    for axiom in ontology.subobject_property_axioms() {
        let (sub_t, sup_t) = (iri_term(axiom.sub_property()), iri_term(axiom.super_property()));
        base.insert([sub_t.clone(), term(RDFS_SUB_PROP), sup_t.clone()]);
        subproperty.entry(sub_t).or_default().insert(sup_t);
    }

    let mut equivalent_properties: BTreeSet<(Term, Term)> = BTreeSet::new();
    for axiom in ontology.equivalent_object_properties_axioms() {
        let members = axiom.properties();
        if members.len() < 2 {
            out_of_fragment += 1;
            continue;
        }
        for pair in members.windows(2) {
            let (left, right) = (iri_term(&pair[0]), iri_term(&pair[1]));
            base.insert([left.clone(), term(OWL_EQUIVALENT_PROP), right.clone()]);
            equivalent_properties.insert((left.clone(), right.clone()));
            equivalent_properties.insert((right, left));
        }
    }

    let mut symmetric: HashSet<Term> = HashSet::new();
    for axiom in ontology.symmetric_property_axioms() {
        let property = iri_term(axiom.property());
        base.insert([property.clone(), term(RDF_TYPE), term(OWL_SYMMETRIC)]);
        symmetric.insert(property);
    }
    let mut inverse_of: BTreeMap<Term, Term> = BTreeMap::new();
    for axiom in ontology.inverse_object_properties_axioms() {
        let (left, right) = (iri_term(axiom.property1()), iri_term(axiom.property2()));
        base.insert([
            left.clone(),
            term("http://www.w3.org/2002/07/owl#inverseOf"),
            right.clone(),
        ]);
        inverse_of.insert(left.clone(), right.clone());
        inverse_of.insert(right, left);
    }
    let mut transitive: HashSet<Term> = HashSet::new();
    for axiom in ontology.transitive_property_axioms() {
        let property = iri_term(axiom.property());
        base.insert([property.clone(), term(RDF_TYPE), term(OWL_TRANSITIVE)]);
        transitive.insert(property);
    }
    let mut reflexive: HashSet<Term> = HashSet::new();
    for axiom in ontology.reflexive_property_axioms() {
        let property = iri_term(axiom.property());
        base.insert([property.clone(), term(RDF_TYPE), term(OWL_REFLEXIVE)]);
        reflexive.insert(property);
    }

    // --- Property chains (prp-spo2) and sameAs (eq-*) -----------------------
    // Chains of length two compose inside the fixpoint; longer chains stay
    // out of the fragment (the anchors declare only two-step chains).
    let mut property_chains: Vec<(Term, Term, Term)> = Vec::new();
    for axiom in ontology.sub_property_chain_axioms() {
        let properties = axiom.properties();
        if properties.len() == 2 {
            let first = iri_term(&properties[0]);
            let second = iri_term(&properties[1]);
            let super_property = iri_term(axiom.super_property());
            base.insert([
                super_property.clone(),
                term("http://www.w3.org/2002/07/owl#propertyChainAxiom"),
                first.clone(),
            ]);
            property_chains.push((super_property, first, second));
        } else {
            out_of_fragment += 1;
        }
    }
    let same_as_property = term("http://www.w3.org/2002/07/owl#sameAs");

    // --- Domain / range ---------------------------------------------------
    let mut object_domains: HashMap<Term, BTreeSet<Term>> = HashMap::new();
    for axiom in ontology.object_property_domain_axioms() {
        let (property, class) = (iri_term(axiom.property()), iri_term(axiom.domain()));
        base.insert([property.clone(), term(RDFS_DOMAIN), class.clone()]);
        object_domains.entry(property).or_default().insert(class);
    }
    let mut object_ranges: HashMap<Term, BTreeSet<Term>> = HashMap::new();
    for axiom in ontology.object_property_range_axioms() {
        let (property, class) = (iri_term(axiom.property()), iri_term(axiom.range()));
        base.insert([property.clone(), term(RDFS_RANGE), class.clone()]);
        object_ranges.entry(property).or_default().insert(class);
    }
    let mut data_domains: HashMap<Term, BTreeSet<Term>> = HashMap::new();
    for axiom in ontology.data_property_domain_axioms() {
        let (property, class) = (iri_term(axiom.property()), iri_term(axiom.domain()));
        base.insert([property.clone(), term(RDFS_DOMAIN), class.clone()]);
        data_domains.entry(property).or_default().insert(class);
    }

    // --- Instance facts ---------------------------------------------------
    let mut types: BTreeMap<Term, BTreeSet<Term>> = BTreeMap::new();
    for axiom in ontology.class_assertions() {
        let individual = iri_term(axiom.individual());
        match named_class(axiom.class_expr()) {
            Some(class) => {
                let class = iri_term(class);
                base.insert([individual.clone(), term(RDF_TYPE), class.clone()]);
                types.entry(individual).or_default().insert(class);
            }
            None => out_of_fragment += 1,
        }
    }

    let mut assertions: BTreeSet<(Term, Term, Term)> = BTreeSet::new();
    for axiom in ontology.property_assertions() {
        let subject = iri_term(axiom.subject());
        let property = iri_term(axiom.property());
        let object = match axiom.object() {
            PropertyAssertionObject::Named(iri) => iri_term(iri),
            PropertyAssertionObject::Anonymous(individual) => bnode_term(individual.node_id()),
        };
        base.insert([subject.clone(), property.clone(), object.clone()]);
        assertions.insert((subject, property, object));
    }

    let mut data_assertions: BTreeSet<(Term, Term, Term)> = BTreeSet::new();
    for axiom in ontology.data_property_assertions() {
        let subject = iri_term(axiom.subject());
        let property = iri_term(axiom.property());
        let value = literal_term(axiom.value());
        base.insert([subject.clone(), property.clone(), value.clone()]);
        data_assertions.insert((subject, property, value));
    }

    let base_count = base.len();

    // --- Fixpoint over the rule fragment -----------------------------------
    let mut closure = base;
    loop {
        let before = closure.len();

        // scm-eqc1/2 and scm-eqp1/2: equivalence implies subsumption.
        for (left, right) in &equivalent_classes {
            closure.insert([left.clone(), term(RDFS_SUB_CLASS), right.clone()]);
            subclass
                .entry(left.clone())
                .or_default()
                .insert(right.clone());
        }
        for (left, right) in &equivalent_properties {
            closure.insert([left.clone(), term(RDFS_SUB_PROP), right.clone()]);
            subproperty
                .entry(left.clone())
                .or_default()
                .insert(right.clone());
        }

        // scm-sco / scm-spo: transitive closure of the hierarchies.
        close_hierarchy(&subclass, RDFS_SUB_CLASS, &mut closure);
        close_hierarchy(&subproperty, RDFS_SUB_PROP, &mut closure);
        let subclass_closed = transitive_closure(&subclass);
        let subproperty_closed = transitive_closure(&subproperty);

        // cax-sco: propagate instance types up the (closed) class hierarchy.
        let mut additions: Vec<(Term, Term)> = Vec::new();
        for (individual, classes) in &types {
            for class in classes {
                if let Some(supers) = subclass_closed.get(class) {
                    for superclass in supers {
                        additions.push((individual.clone(), superclass.clone()));
                    }
                }
            }
        }
        for (individual, class) in additions {
            if types.entry(individual.clone()).or_default().insert(class.clone()) {
                closure.insert([individual, term(RDF_TYPE), class]);
            }
        }

        // prp-spo1: propagate assertions up the (closed) property hierarchy.
        let mut propagated: Vec<(Term, Term, Term)> = Vec::new();
        for (subject, property, object) in &assertions {
            if let Some(supers) = subproperty_closed.get(property) {
                for superproperty in supers {
                    propagated.push((subject.clone(), superproperty.clone(), object.clone()));
                }
            }
        }
        for (subject, superproperty, object) in propagated {
            if assertions.insert((subject.clone(), superproperty.clone(), object.clone())) {
                closure.insert([subject, superproperty, object]);
            }
        }

        // prp-symp / prp-trp: property characteristics.
        let mut symmetric_additions: Vec<(Term, Term, Term)> = Vec::new();
        for (subject, property, object) in &assertions {
            if symmetric.contains(property) {
                symmetric_additions.push((object.clone(), property.clone(), subject.clone()));
            }
        }
        for (subject, property, object) in symmetric_additions {
            if assertions.insert((subject.clone(), property.clone(), object.clone())) {
                closure.insert([subject, property, object]);
            }
        }
        // prp-inv1/2: assertions propagate across declared inverse roles.
        let mut inverse_additions: Vec<(Term, Term, Term)> = Vec::new();
        for (subject, property, object) in &assertions {
            if let Some(inverse) = inverse_of.get(property) {
                inverse_additions.push((object.clone(), inverse.clone(), subject.clone()));
            }
        }
        for (subject, property, object) in inverse_additions {
            if assertions.insert((subject.clone(), property.clone(), object.clone())) {
                closure.insert([subject, property, object]);
            }
        }
        let transitive_additions = transitive_assertions(&assertions, &transitive);
        for (subject, property, object) in transitive_additions {
            if assertions.insert((subject.clone(), property.clone(), object.clone())) {
                closure.insert([subject, property, object]);
            }
        }

        // prp-dom / prp-rng: typing through property use.
        let mut typed: Vec<(Term, Term)> = Vec::new();
        for (subject, property, object) in &assertions {
            if let Some(domains) = object_domains.get(property) {
                for class in domains {
                    typed.push((subject.clone(), class.clone()));
                }
            }
            if let Some(ranges) = object_ranges.get(property) {
                for class in ranges {
                    typed.push((object.clone(), class.clone()));
                }
            }
        }
        for (subject, property, _value) in &data_assertions {
            if let Some(domains) = data_domains.get(property) {
                for class in domains {
                    typed.push((subject.clone(), class.clone()));
                }
            }
        }
        for (individual, class) in typed {
            if types.entry(individual.clone()).or_default().insert(class.clone()) {
                closure.insert([individual, term(RDF_TYPE), class]);
            }
        }

        // prp-ref: reflexive roles add self-pairs over the declared
        // individuals (exactly the owl:NamedIndividual-declared set, the
        // same universe the derivation core's v2 rule uses).
        let declared_individuals: Vec<Term> = types
            .iter()
            .filter(|(_, classes)| classes.contains(&term(OWL_NAMED_INDIVIDUAL)))
            .map(|(individual, _)| individual.clone())
            .collect();
        let mut reflexive_additions: Vec<(Term, Term)> = Vec::new();
        if include_reflexive {
            for property in &reflexive {
                for individual in &declared_individuals {
                    reflexive_additions.push((individual.clone(), property.clone()));
                }
            }
        }
        for (individual, property) in reflexive_additions {
            if assertions.insert((individual.clone(), property.clone(), individual.clone())) {
                closure.insert([individual.clone(), property, individual]);
            }
        }

        // Construct rules over expression superclasses (engine mirror of the
        // derivation core's v2 cls-hs / cls-hv2 / cls-intro / cls-qc1 /
        // cls-mqc3 / cls-svf1 / cls-av1).
        let mut construct_typed: Vec<(Term, Term)> = Vec::new();
        for rule in &expression_rules {
            match rule {
                ExpressionRule::Direct(class, premise) => {
                    for individual in individuals_satisfying(premise, &assertions, &types) {
                        construct_typed.push((individual, class.clone()));
                    }
                }
                ExpressionRule::Intersection(class, premise, member) => {
                    for individual in individuals_satisfying(premise, &assertions, &types) {
                        if types
                            .get(&individual)
                            .is_some_and(|classes| classes.contains(member))
                        {
                            construct_typed.push((individual, class.clone()));
                        }
                    }
                }
                ExpressionRule::SomeValuesFiller(property, filler) => {
                    for (subject, prop, _object) in &assertions {
                        if prop == property {
                            construct_typed.push((subject.clone(), filler.clone()));
                        }
                    }
                }
                ExpressionRule::AllValuesFiller(class, property, filler) => {
                    for (subject, prop, object) in &assertions {
                        if prop == property
                            && types
                                .get(subject)
                                .is_some_and(|classes| classes.contains(class))
                        {
                            construct_typed.push((object.clone(), filler.clone()));
                        }
                    }
                }
            }
        }
        for (individual, class) in construct_typed {
            if types.entry(individual.clone()).or_default().insert(class.clone()) {
                closure.insert([individual, term(RDF_TYPE), class]);
            }
        }

        // prp-spo2-2: two-step chains compose over the (growing) assertion
        // set; the fixpoint generalizes to chains of chains. The second
        // step joins through a (property, subject) index — the big patched
        // graphs make a nested full scan per outer match infeasible.
        let mut by_property_subject: std::collections::HashMap<
            (&Term, &Term),
            Vec<&Term>,
        > = std::collections::HashMap::new();
        for (subject, property, object) in &assertions {
            by_property_subject
                .entry((property, subject))
                .or_default()
                .push(object);
        }
        let mut chained: Vec<(Term, Term, Term)> = Vec::new();
        for (super_property, first, second) in &property_chains {
            for (subject, property, middle) in &assertions {
                if property != first {
                    continue;
                }
                if let Some(objects) = by_property_subject.get(&(second, middle)) {
                    for object in objects {
                        chained.push((subject.clone(), super_property.clone(), (*object).clone()));
                    }
                }
            }
        }
        drop(by_property_subject);
        for (subject, super_property, object) in chained {
            if assertions.insert((subject.clone(), super_property.clone(), object.clone())) {
                closure.insert([subject, super_property, object]);
            }
        }

        // eq-sym / eq-trans / eq-rep: the sameAs closure replaces
        // individuals across assertions and types.
        let mut same_as: BTreeSet<(Term, Term)> = BTreeSet::new();
        if include_equality {
            for (subject, property, object) in &assertions {
                if property == &same_as_property {
                    same_as.insert((subject.clone(), object.clone()));
                    same_as.insert((object.clone(), subject.clone()));
                }
            }
        }
        // eq-trans over the (tiny) sameAs graph.
        loop {
            let mut transitive: Vec<(Term, Term)> = Vec::new();
            let pairs: Vec<(Term, Term)> = same_as.iter().cloned().collect();
            for (a, b) in &pairs {
                for (c, d) in &pairs {
                    if b == c && !same_as.contains(&(a.clone(), d.clone())) {
                        transitive.push((a.clone(), d.clone()));
                    }
                }
            }
            if transitive.is_empty() {
                break;
            }
            for pair in transitive {
                same_as.insert(pair);
            }
        }
        for (a, b) in &same_as {
            closure.insert([a.clone(), same_as_property.clone(), b.clone()]);
        }
        let mut replaced: Vec<(Term, Term, Term)> = Vec::new();
        let mut replaced_types: Vec<(Term, Term)> = Vec::new();
        for (a, b) in &same_as {
            for (subject, property, object) in &assertions {
                if property == &same_as_property {
                    continue;
                }
                if subject == a {
                    replaced.push((b.clone(), property.clone(), object.clone()));
                }
                if object == a {
                    replaced.push((subject.clone(), property.clone(), b.clone()));
                }
            }
            for (subject, classes) in &types {
                if subject == a {
                    for class in classes {
                        replaced_types.push((b.clone(), class.clone()));
                    }
                }
            }
        }
        for (subject, property, object) in replaced {
            if assertions.insert((subject.clone(), property.clone(), object.clone())) {
                closure.insert([subject, property, object]);
            }
        }
        for (individual, class) in replaced_types {
            if types.entry(individual.clone()).or_default().insert(class.clone()) {
                closure.insert([individual, term(RDF_TYPE), class]);
            }
        }

        if closure.len() == before {
            break;
        }
    }

    let stats = MaterializeStats {
        base_triples: base_count,
        derived_triples: closure.len() - base_count,
        closure_triples: closure.len(),
        out_of_fragment_axioms: out_of_fragment,
    };
    (closure, stats)
}

fn term(value: &str) -> Term {
    format!("<{value}>")
}

/// Insert every transitively derivable subsumption triple into the closure.
fn close_hierarchy(
    hierarchy: &BTreeMap<Term, BTreeSet<Term>>,
    predicate: &str,
    closure: &mut BTreeSet<Triple>,
) {
    for (node, supers) in transitive_closure(hierarchy) {
        for superclass in supers {
            closure.insert([node.clone(), term(predicate), superclass]);
        }
    }
}

/// Transitive closure of a subsumption graph, cycle-safe.
fn transitive_closure(hierarchy: &BTreeMap<Term, BTreeSet<Term>>) -> BTreeMap<Term, BTreeSet<Term>> {
    let mut closed: BTreeMap<Term, BTreeSet<Term>> = BTreeMap::new();
    for start in hierarchy.keys() {
        let mut visited: BTreeSet<Term> = BTreeSet::new();
        let mut stack: Vec<&Term> = vec![start];
        while let Some(current) = stack.pop() {
            if let Some(supers) = hierarchy.get(current) {
                for superclass in supers {
                    if visited.insert(superclass.clone()) {
                        stack.push(superclass);
                    }
                }
            }
        }
        closed.insert(start.clone(), visited);
    }
    closed
}

/// prp-trp: all (s, p, o) implied by transitivity of the declared properties.
fn transitive_assertions(
    assertions: &BTreeSet<(Term, Term, Term)>,
    transitive: &HashSet<Term>,
) -> Vec<(Term, Term, Term)> {
    let mut direct: BTreeMap<(Term, Term), BTreeSet<Term>> = BTreeMap::new();
    for (subject, property, object) in assertions {
        if transitive.contains(property) {
            direct
                .entry((property.clone(), subject.clone()))
                .or_default()
                .insert(object.clone());
        }
    }
    let mut additions: Vec<(Term, Term, Term)> = Vec::new();
    for ((property, subject), targets) in &direct {
        let mut reachable: BTreeSet<Term> = BTreeSet::new();
        let mut queue: Vec<Term> = targets.iter().cloned().collect();
        while let Some(current) = queue.pop() {
            if !reachable.insert(current.clone()) {
                continue;
            }
            if let Some(next) = direct.get(&(property.clone(), current)) {
                queue.extend(next.iter().cloned());
            }
        }
        for object in reachable {
            additions.push((subject.clone(), property.clone(), object));
        }
    }
    additions
}

/// Render a materialized closure to N-Triples text (already sorted).
pub fn write_ntriples(closure: &BTreeSet<Triple>) -> String {
    let mut out = String::with_capacity(closure.len() * 96);
    for [subject, predicate, object] in closure {
        out.push_str(subject);
        out.push(' ');
        out.push_str(predicate);
        out.push(' ');
        out.push_str(object);
        out.push_str(" .\n");
    }
    out
}
