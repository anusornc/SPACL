use std::collections::{BTreeSet, HashMap};

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression, ObjectPropertyExpression};

const OWL_NOTHING_IRI: &str = "http://www.w3.org/2002/07/owl#Nothing";
const OWL_THING_IRI: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_TOP_OBJECT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#topObjectProperty";
const OWL_BOTTOM_OBJECT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";

const MAX_MODEL_NODES: usize = 14;
const MAX_EXPRESSION_SCAN_NODES: usize = 10_000;
const MAX_EXPRESSION_DEPTH: usize = 128;
const MAX_PARTITION_STEPS: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Qualifier {
    Any,
    Top,
    Bottom,
    Named(String),
    All(Vec<Qualifier>),
}

#[derive(Debug, Clone)]
struct CardinalityAssertion {
    subject: String,
    property: String,
    qualifier: Qualifier,
    minimum: Option<u32>,
    maximum: Option<u32>,
}

#[derive(Debug, Default)]
struct QualifiedFacts {
    class_memberships: HashMap<String, BTreeSet<String>>,
    role_edges: Vec<(String, String, String)>,
    negative_role_edges: Vec<(String, String, String)>,
    different_pairs: BTreeSet<(String, String)>,
    disjoint_class_pairs: BTreeSet<(String, String)>,
    irreflexive_properties: BTreeSet<String>,
    cardinalities: Vec<CardinalityAssertion>,
}

/// Produces sound certificates for bounded object-cardinality/hasSelf ABox
/// checks. An incomplete candidate search returns unsupported, not UNSAT.
///
/// `Ok(None)` means the ontology contains no targeted construct. `Ok(Some(_))`
/// is a certificate for this bounded fragment. Targeted constructs mixed with
/// unsupported class expressions, role TBox, sameAs, data qualifiers, or class
/// TBox are rejected before legacy reasoners can guess a result.
/// Nonempty object nominals also enter this preflight for rejection, not for
/// nominal solving; programmatically constructed empty nominals keep the legacy
/// empty-class behavior.
pub(crate) fn check_qualified_consistency(ontology: &Ontology) -> OwlResult<Option<bool>> {
    if !ontology_has_targeted_construct(ontology)? {
        return Ok(None);
    }

    validate_expression_bounds(ontology)?;
    reject_unsupported_axioms(ontology)?;

    let facts = collect_facts(ontology)?;
    if has_class_clash(ontology, &facts) {
        return Ok(Some(false));
    }
    if has_role_clash(&facts) {
        return Ok(Some(false));
    }
    if has_cardinality_clash(&facts)? {
        return Ok(Some(false));
    }

    verify_candidate_model(facts).map(Some)
}

/// Bound the complete class-expression trees before any recursive conversion
/// or qualifier evaluation. The presence scan alone stops at cardinalities and
/// therefore cannot bound their potentially deeply nested qualifiers.
fn validate_expression_bounds(ontology: &Ontology) -> OwlResult<()> {
    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::SubClassOf(axiom) => {
                validate_expression_tree(axiom.sub_class())?;
                validate_expression_tree(axiom.super_class())?;
            }
            Axiom::ClassAssertion(axiom) => validate_expression_tree(axiom.class_expr())?,
            _ => {}
        }
    }
    Ok(())
}

fn validate_expression_tree(expression: &ClassExpression) -> OwlResult<()> {
    let mut stack = vec![(expression, 0)];
    let mut visited = 0_usize;
    while let Some((expression, depth)) = stack.pop() {
        if depth > MAX_EXPRESSION_DEPTH {
            return Err(OwlError::ResourceLimitExceeded {
                resource_type: "qualified_consistency_expression_depth".to_string(),
                limit: MAX_EXPRESSION_DEPTH,
                message: "class expression exceeds bounded recursive conversion depth".to_string(),
            });
        }
        visited += 1;
        match expression {
            ClassExpression::ObjectIntersectionOf(operands)
            | ClassExpression::ObjectUnionOf(operands) => {
                if visited
                    .saturating_add(stack.len())
                    .saturating_add(operands.len())
                    > MAX_EXPRESSION_SCAN_NODES
                {
                    return Err(OwlError::ResourceLimitExceeded {
                        resource_type: "qualified_consistency_expression_scan".to_string(),
                        limit: MAX_EXPRESSION_SCAN_NODES,
                        message: "complete expression tree exceeds node budget".to_string(),
                    });
                }
                stack.extend(operands.iter().map(|operand| (operand.as_ref(), depth + 1)));
            }
            ClassExpression::ObjectComplementOf(inner)
            | ClassExpression::ObjectSomeValuesFrom(_, inner)
            | ClassExpression::ObjectAllValuesFrom(_, inner)
            | ClassExpression::ObjectMinCardinality(_, _, Some(inner))
            | ClassExpression::ObjectMaxCardinality(_, _, Some(inner))
            | ClassExpression::ObjectExactCardinality(_, _, Some(inner)) => {
                stack.push((inner, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

fn ontology_has_targeted_construct(ontology: &Ontology) -> OwlResult<bool> {
    for axiom in ontology.axioms() {
        let has_targeted = match axiom.as_ref() {
            Axiom::SubClassOf(axiom) => {
                expression_has_targeted_construct(axiom.sub_class())?
                    || expression_has_targeted_construct(axiom.super_class())?
            }
            Axiom::ClassAssertion(axiom) => expression_has_targeted_construct(axiom.class_expr())?,
            Axiom::DataMinQualifiedCardinality(_)
            | Axiom::DataMaxQualifiedCardinality(_)
            | Axiom::DataExactQualifiedCardinality(_)
            | Axiom::ObjectMinQualifiedCardinality(_)
            | Axiom::ObjectMaxQualifiedCardinality(_)
            | Axiom::ObjectExactQualifiedCardinality(_) => true,
            _ => false,
        };
        if has_targeted {
            return Ok(true);
        }
    }
    Ok(false)
}

fn expression_has_targeted_construct(expression: &ClassExpression) -> OwlResult<bool> {
    let mut stack = vec![expression];
    let mut visited = 0_usize;

    while let Some(expression) = stack.pop() {
        visited += 1;
        if visited > MAX_EXPRESSION_SCAN_NODES {
            return Err(OwlError::ResourceLimitExceeded {
                resource_type: "qualified_consistency_expression_scan".to_string(),
                limit: MAX_EXPRESSION_SCAN_NODES,
                message: "targeted construct scan exceeded expression node budget".to_string(),
            });
        }
        match expression {
            ClassExpression::ObjectHasSelf(_) => return Ok(true),
            ClassExpression::ObjectOneOf(individuals) if !individuals.is_empty() => {
                return Ok(true);
            }
            ClassExpression::ObjectMinCardinality(_, _, Some(_))
            | ClassExpression::ObjectMaxCardinality(_, _, Some(_))
            | ClassExpression::ObjectExactCardinality(_, _, Some(_))
            | ClassExpression::DataMinCardinality(_, _, Some(_))
            | ClassExpression::DataMaxCardinality(_, _, Some(_))
            | ClassExpression::DataExactCardinality(_, _, Some(_)) => return Ok(true),
            ClassExpression::ObjectIntersectionOf(operands)
            | ClassExpression::ObjectUnionOf(operands) => {
                stack.extend(operands.iter().map(Box::as_ref));
            }
            ClassExpression::ObjectComplementOf(operand) => stack.push(operand),
            ClassExpression::ObjectSomeValuesFrom(_, filler)
            | ClassExpression::ObjectAllValuesFrom(_, filler) => stack.push(filler),
            ClassExpression::ObjectMinCardinality(_, _, None)
            | ClassExpression::ObjectMaxCardinality(_, _, None)
            | ClassExpression::ObjectExactCardinality(_, _, None)
            | ClassExpression::Class(_)
            | ClassExpression::ObjectOneOf(_)
            | ClassExpression::ObjectHasValue(_, _)
            | ClassExpression::DataSomeValuesFrom(_, _)
            | ClassExpression::DataAllValuesFrom(_, _)
            | ClassExpression::DataHasValue(_, _)
            | ClassExpression::DataMinCardinality(_, _, None)
            | ClassExpression::DataMaxCardinality(_, _, None)
            | ClassExpression::DataExactCardinality(_, _, None) => {}
        }
    }

    Ok(false)
}

fn reject_unsupported_axioms(ontology: &Ontology) -> OwlResult<()> {
    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::ClassAssertion(axiom) => validate_supported_class_assertion(axiom.class_expr())?,
            Axiom::PropertyAssertion(_) | Axiom::NegativeObjectPropertyAssertion(_) => {}
            Axiom::DifferentIndividuals(_) | Axiom::IrreflexiveProperty(_) => {}
            Axiom::DisjointClasses(_) | Axiom::AnnotationAssertion(_) => {}
            Axiom::SameIndividual(_) => {
                return unsupported("sameAs is outside bounded qualified consistency")
            }
            Axiom::SubClassOf(_) => {
                return unsupported("class TBox qualified cardinality reasoning is unsupported")
            }
            Axiom::DataMinQualifiedCardinality(_)
            | Axiom::DataMaxQualifiedCardinality(_)
            | Axiom::DataExactQualifiedCardinality(_)
            | Axiom::DataPropertyAssertion(_)
            | Axiom::NegativeDataPropertyAssertion(_) => {
                return unsupported("qualified data reasoning is unsupported")
            }
            Axiom::ObjectPropertyDomain(_)
            | Axiom::ObjectPropertyRange(_)
            | Axiom::SubObjectProperty(_)
            | Axiom::EquivalentObjectProperties(_)
            | Axiom::DisjointObjectProperties(_)
            | Axiom::InverseObjectProperties(_)
            | Axiom::FunctionalProperty(_)
            | Axiom::InverseFunctionalProperty(_)
            | Axiom::ReflexiveProperty(_)
            | Axiom::SymmetricProperty(_)
            | Axiom::AsymmetricProperty(_)
            | Axiom::TransitiveProperty(_)
            | Axiom::SubPropertyChainOf(_)
            | Axiom::ObjectMinQualifiedCardinality(_)
            | Axiom::ObjectMaxQualifiedCardinality(_)
            | Axiom::ObjectExactQualifiedCardinality(_) => {
                return unsupported("role TBox qualified reasoning is unsupported")
            }
            _ => {
                return unsupported("ontology mixes qualified reasoning with an unsupported axiom")
            }
        }
    }
    Ok(())
}

fn validate_supported_class_assertion(expression: &ClassExpression) -> OwlResult<()> {
    match expression {
        ClassExpression::Class(_) => Ok(()),
        ClassExpression::ObjectIntersectionOf(operands) => {
            for operand in operands {
                validate_supported_class_assertion(operand)?;
            }
            Ok(())
        }
        ClassExpression::ObjectHasSelf(property) => {
            named_object_property_iri(property)?;
            Ok(())
        }
        ClassExpression::ObjectMinCardinality(_, property, qualifier)
        | ClassExpression::ObjectMaxCardinality(_, property, qualifier)
        | ClassExpression::ObjectExactCardinality(_, property, qualifier) => {
            named_object_property_iri(property)?;
            validate_qualifier(qualifier.as_deref())
        }
        ClassExpression::DataMinCardinality(_, _, Some(_))
        | ClassExpression::DataMaxCardinality(_, _, Some(_))
        | ClassExpression::DataExactCardinality(_, _, Some(_)) => {
            unsupported("qualified data reasoning is unsupported")
        }
        _ => unsupported("class expression is outside bounded qualified consistency"),
    }
}

fn validate_qualifier(qualifier: Option<&ClassExpression>) -> OwlResult<()> {
    let Some(qualifier) = qualifier else {
        return Ok(());
    };
    qualifier_key(qualifier).map(|_| ())
}

fn collect_facts(ontology: &Ontology) -> OwlResult<QualifiedFacts> {
    let mut facts = QualifiedFacts::default();

    for axiom in ontology.class_assertions() {
        collect_class_assertion(&mut facts, axiom.individual().as_ref(), axiom.class_expr())?;
    }
    for axiom in ontology.property_assertions() {
        let Some(object) = axiom.object_iri() else {
            return unsupported("anonymous role assertion targets are unsupported");
        };
        if axiom.property().as_str() == OWL_TOP_OBJECT_PROPERTY_IRI {
            return unsupported("topObjectProperty is outside bounded qualified consistency");
        }
        facts.role_edges.push((
            axiom.subject().as_str().to_string(),
            axiom.property().as_str().to_string(),
            object.as_str().to_string(),
        ));
    }
    for axiom in ontology.negative_object_property_assertions() {
        let Some(object) = axiom.object_iri() else {
            return unsupported("anonymous negative role assertion targets are unsupported");
        };
        if axiom.property().as_str() == OWL_TOP_OBJECT_PROPERTY_IRI {
            return unsupported("topObjectProperty is outside bounded qualified consistency");
        }
        facts.negative_role_edges.push((
            axiom.subject().as_str().to_string(),
            axiom.property().as_str().to_string(),
            object.as_str().to_string(),
        ));
    }
    for axiom in ontology.different_individuals_axioms() {
        let individuals = axiom.individuals();
        for (index, left) in individuals.iter().enumerate() {
            for right in individuals.iter().skip(index + 1) {
                facts
                    .different_pairs
                    .insert(ordered_pair(left.as_ref(), right.as_ref()));
            }
        }
    }
    for axiom in ontology.disjoint_classes_axioms() {
        let classes = axiom.classes();
        for (index, left) in classes.iter().enumerate() {
            for right in classes.iter().skip(index + 1) {
                facts
                    .disjoint_class_pairs
                    .insert(ordered_pair(left.as_ref(), right.as_ref()));
            }
        }
    }
    for axiom in ontology.irreflexive_property_axioms() {
        if axiom.property().as_str() == OWL_TOP_OBJECT_PROPERTY_IRI {
            return unsupported("topObjectProperty is outside bounded qualified consistency");
        }
        facts
            .irreflexive_properties
            .insert(axiom.property().as_str().to_string());
    }

    Ok(facts)
}

fn model_nodes(facts: &QualifiedFacts) -> Vec<String> {
    let mut nodes = BTreeSet::new();
    nodes.extend(facts.class_memberships.keys().cloned());
    for (subject, _, object) in &facts.role_edges {
        nodes.insert(subject.clone());
        nodes.insert(object.clone());
    }
    for (subject, _, object) in &facts.negative_role_edges {
        nodes.insert(subject.clone());
        nodes.insert(object.clone());
    }
    for (left, right) in &facts.different_pairs {
        nodes.insert(left.clone());
        nodes.insert(right.clone());
    }
    for cardinality in &facts.cardinalities {
        nodes.insert(cardinality.subject.clone());
    }
    nodes.into_iter().collect()
}

fn collect_class_assertion(
    facts: &mut QualifiedFacts,
    individual: &IRI,
    expression: &ClassExpression,
) -> OwlResult<()> {
    match expression {
        ClassExpression::Class(class) => {
            facts
                .class_memberships
                .entry(individual.as_str().to_string())
                .or_default()
                .insert(class.iri().as_str().to_string());
        }
        ClassExpression::ObjectIntersectionOf(operands) => {
            for operand in operands {
                collect_class_assertion(facts, individual, operand)?;
            }
        }
        ClassExpression::ObjectHasSelf(property) => {
            let property = named_object_property_iri(property)?.to_string();
            facts.role_edges.push((
                individual.as_str().to_string(),
                property,
                individual.as_str().to_string(),
            ));
        }
        ClassExpression::ObjectMinCardinality(minimum, property, qualifier) => {
            facts.cardinalities.push(CardinalityAssertion {
                subject: individual.as_str().to_string(),
                property: named_object_property_iri(property)?.to_string(),
                qualifier: qualifier_key_optional(qualifier.as_deref())?,
                minimum: Some(*minimum),
                maximum: None,
            });
        }
        ClassExpression::ObjectMaxCardinality(maximum, property, qualifier) => {
            facts.cardinalities.push(CardinalityAssertion {
                subject: individual.as_str().to_string(),
                property: named_object_property_iri(property)?.to_string(),
                qualifier: qualifier_key_optional(qualifier.as_deref())?,
                minimum: None,
                maximum: Some(*maximum),
            });
        }
        ClassExpression::ObjectExactCardinality(exact, property, qualifier) => {
            facts.cardinalities.push(CardinalityAssertion {
                subject: individual.as_str().to_string(),
                property: named_object_property_iri(property)?.to_string(),
                qualifier: qualifier_key_optional(qualifier.as_deref())?,
                minimum: Some(*exact),
                maximum: Some(*exact),
            });
        }
        _ => {}
    }
    Ok(())
}

fn has_class_clash(ontology: &Ontology, facts: &QualifiedFacts) -> bool {
    for memberships in facts.class_memberships.values() {
        if memberships.contains(OWL_NOTHING_IRI) {
            return true;
        }
        if memberships.iter().any(|class| {
            facts
                .disjoint_class_pairs
                .contains(&ordered_pair_str(OWL_THING_IRI, class))
        }) {
            return true;
        }
        for axiom in ontology.disjoint_classes_axioms() {
            let count = axiom
                .classes()
                .iter()
                .filter(|class| memberships.contains(class.as_str()))
                .count();
            if count >= 2 {
                return true;
            }
        }
    }
    false
}

fn has_role_clash(facts: &QualifiedFacts) -> bool {
    facts.role_edges.iter().any(|(subject, property, object)| {
        property == OWL_BOTTOM_OBJECT_PROPERTY_IRI
            || (subject == object && facts.irreflexive_properties.contains(property))
            || facts
                .negative_role_edges
                .iter()
                .any(|(neg_subject, neg_property, neg_object)| {
                    subject == neg_subject && property == neg_property && object == neg_object
                })
    })
}

fn has_cardinality_clash(facts: &QualifiedFacts) -> OwlResult<bool> {
    for cardinality in &facts.cardinalities {
        if (cardinality.property == OWL_BOTTOM_OBJECT_PROPERTY_IRI
            || matches!(cardinality.qualifier, Qualifier::Bottom))
            && cardinality.minimum.is_some_and(|minimum| minimum > 0)
        {
            return Ok(true);
        }
        if cardinality.minimum.is_some_and(|minimum| minimum > 0)
            && qualifier_is_unsatisfiable(facts, &cardinality.qualifier)
        {
            return Ok(true);
        }
        if cardinality
            .minimum
            .zip(cardinality.maximum)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Ok(true);
        }
        if let Some(maximum) = cardinality.maximum {
            let targets = qualifying_targets(facts, cardinality);
            if known_distinctness_exceeds_maximum(facts, &targets, maximum as usize)? {
                return Ok(true);
            }
        }
    }

    reject_unsupported_cross_qualifier_bounds(facts)?;

    for (index, left) in facts.cardinalities.iter().enumerate() {
        for right in facts.cardinalities.iter().skip(index + 1) {
            if left.subject == right.subject
                && left.property == right.property
                && left.qualifier == right.qualifier
            {
                let minimum = left.minimum.into_iter().chain(right.minimum).max();
                let maximum = left.maximum.into_iter().chain(right.maximum).min();
                if minimum
                    .zip(maximum)
                    .is_some_and(|(minimum, maximum)| minimum > maximum)
                {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn verify_candidate_model(mut facts: QualifiedFacts) -> OwlResult<bool> {
    add_minimum_witnesses(&mut facts)?;
    let nodes = model_nodes(&facts);
    if nodes.len() > MAX_MODEL_NODES {
        return Err(OwlError::ResourceLimitExceeded {
            resource_type: "qualified_consistency_model_nodes".to_string(),
            limit: MAX_MODEL_NODES,
            message: format!(
                "{} candidate model nodes exceed bounded search",
                nodes.len()
            ),
        });
    }

    let mut node_index = HashMap::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        node_index.insert(node.clone(), index);
    }

    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by_key(|index| {
        std::cmp::Reverse(
            nodes
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != *index)
                .filter(|(_, other)| are_semantically_distinct(&facts, &nodes[*index], other))
                .count(),
        )
    });

    let mut partition = vec![None; nodes.len()];
    let mut steps = 0;
    let outcome = search_partition(
        &facts,
        &nodes,
        &node_index,
        &order,
        &mut partition,
        0,
        0,
        &mut steps,
    );

    match outcome {
        SearchOutcome::Found => Ok(true),
        SearchOutcome::Exhausted => {
            unsupported("bounded candidate model construction was incomplete")
        }
        SearchOutcome::BudgetExceeded => Err(OwlError::ResourceLimitExceeded {
            resource_type: "qualified_consistency_partition_steps".to_string(),
            limit: MAX_PARTITION_STEPS,
            message: "bounded candidate model search exceeded its step budget".to_string(),
        }),
    }
}

fn add_minimum_witnesses(facts: &mut QualifiedFacts) -> OwlResult<()> {
    let cardinalities = facts.cardinalities.clone();
    let base_nodes = model_nodes(facts).len();
    let required_witnesses = required_minimum_witnesses(&cardinalities)?;
    if base_nodes.saturating_add(required_witnesses) > MAX_MODEL_NODES {
        return Err(OwlError::ResourceLimitExceeded {
            resource_type: "qualified_consistency_model_nodes".to_string(),
            limit: MAX_MODEL_NODES,
            message: format!(
                "{} base nodes plus {} minimum witnesses exceed bounded search",
                base_nodes, required_witnesses
            ),
        });
    }

    let mut used_names: BTreeSet<String> = model_nodes(facts).into_iter().collect();
    let mut witness_count = 0_usize;
    for cardinality in cardinalities {
        let Some(minimum) = cardinality.minimum else {
            continue;
        };
        if minimum == 0 {
            continue;
        }
        for _ in 0..minimum {
            let witness = fresh_witness_name(&used_names, witness_count);
            witness_count += 1;
            used_names.insert(witness.clone());
            apply_qualifier_membership(facts, &witness, &cardinality.qualifier)?;
            facts.role_edges.push((
                cardinality.subject.clone(),
                cardinality.property.clone(),
                witness,
            ));
        }
    }
    Ok(())
}

fn required_minimum_witnesses(cardinalities: &[CardinalityAssertion]) -> OwlResult<usize> {
    cardinalities
        .iter()
        .try_fold(0_usize, |total, cardinality| {
            let Some(minimum) = cardinality.minimum else {
                return Ok(total);
            };
            total
                .checked_add(minimum as usize)
                .ok_or_else(|| OwlError::ResourceLimitExceeded {
                    resource_type: "qualified_consistency_minimum_witnesses".to_string(),
                    limit: MAX_MODEL_NODES,
                    message: "minimum witness count overflowed bounded verifier".to_string(),
                })
        })
}

fn fresh_witness_name(used_names: &BTreeSet<String>, start: usize) -> String {
    let mut index = start;
    loop {
        let candidate = format!("urn:tableauxx:qualified-consistency:witness:{index}");
        if !used_names.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn apply_qualifier_membership(
    facts: &mut QualifiedFacts,
    individual: &str,
    qualifier: &Qualifier,
) -> OwlResult<()> {
    match qualifier {
        Qualifier::Any | Qualifier::Top => Ok(()),
        Qualifier::Bottom => unsupported("cannot construct a witness for bottom qualifier"),
        Qualifier::Named(class) => {
            facts
                .class_memberships
                .entry(individual.to_string())
                .or_default()
                .insert(class.clone());
            Ok(())
        }
        Qualifier::All(qualifiers) => {
            for qualifier in qualifiers {
                apply_qualifier_membership(facts, individual, qualifier)?;
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchOutcome {
    Found,
    Exhausted,
    BudgetExceeded,
}

fn search_partition(
    facts: &QualifiedFacts,
    nodes: &[String],
    node_index: &HashMap<String, usize>,
    order: &[usize],
    partition: &mut [Option<usize>],
    position: usize,
    class_count: usize,
    steps: &mut usize,
) -> SearchOutcome {
    *steps += 1;
    if *steps > MAX_PARTITION_STEPS {
        return SearchOutcome::BudgetExceeded;
    }
    if position == order.len() {
        return if verify_partition(facts, nodes, node_index, partition) {
            SearchOutcome::Found
        } else {
            SearchOutcome::Exhausted
        };
    }

    let node = order[position];
    for class in 0..class_count {
        if can_assign_to_class(facts, nodes, partition, node, class) {
            partition[node] = Some(class);
            match search_partition(
                facts,
                nodes,
                node_index,
                order,
                partition,
                position + 1,
                class_count,
                steps,
            ) {
                SearchOutcome::Exhausted => {}
                outcome => return outcome,
            }
            partition[node] = None;
        }
    }

    partition[node] = Some(class_count);
    let outcome = search_partition(
        facts,
        nodes,
        node_index,
        order,
        partition,
        position + 1,
        class_count + 1,
        steps,
    );
    partition[node] = None;
    outcome
}

fn can_assign_to_class(
    facts: &QualifiedFacts,
    nodes: &[String],
    partition: &[Option<usize>],
    node: usize,
    class: usize,
) -> bool {
    partition.iter().enumerate().all(|(other, other_class)| {
        !other_class.is_some_and(|other_class| {
            other_class == class && !can_merge_nodes(facts, &nodes[node], &nodes[other])
        })
    })
}

fn can_merge_nodes(facts: &QualifiedFacts, left: &str, right: &str) -> bool {
    !are_semantically_distinct(facts, left, right)
}

fn verify_partition(
    facts: &QualifiedFacts,
    nodes: &[String],
    node_index: &HashMap<String, usize>,
    partition: &[Option<usize>],
) -> bool {
    if facts
        .different_pairs
        .iter()
        .any(|(left, right)| same_partition(node_index, partition, left, right))
    {
        return false;
    }

    let mut class_memberships: HashMap<usize, BTreeSet<String>> = HashMap::new();
    for node in nodes {
        let Some(class) = partition_class(node_index, partition, node) else {
            return false;
        };
        class_memberships
            .entry(class)
            .or_default()
            .insert(OWL_THING_IRI.to_string());
    }
    for (node, classes) in &facts.class_memberships {
        let Some(class) = partition_class(node_index, partition, node) else {
            return false;
        };
        class_memberships
            .entry(class)
            .or_default()
            .extend(classes.iter().cloned());
    }
    if class_memberships.values().any(|classes| {
        classes.contains(OWL_NOTHING_IRI)
            || classes.iter().any(|left| {
                classes.iter().any(|right| {
                    facts
                        .disjoint_class_pairs
                        .contains(&ordered_pair_str(left, right))
                })
            })
    }) {
        return false;
    }

    let mut role_edges = BTreeSet::new();
    for (subject, property, object) in &facts.role_edges {
        let Some(subject_class) = partition_class(node_index, partition, subject) else {
            return false;
        };
        let Some(object_class) = partition_class(node_index, partition, object) else {
            return false;
        };
        if subject_class == object_class && facts.irreflexive_properties.contains(property) {
            return false;
        }
        if property == OWL_BOTTOM_OBJECT_PROPERTY_IRI {
            return false;
        }
        role_edges.insert((subject_class, property.clone(), object_class));
    }

    for (subject, property, object) in &facts.negative_role_edges {
        let Some(subject_class) = partition_class(node_index, partition, subject) else {
            return false;
        };
        let Some(object_class) = partition_class(node_index, partition, object) else {
            return false;
        };
        if role_edges.contains(&(subject_class, property.clone(), object_class)) {
            return false;
        }
    }

    for cardinality in &facts.cardinalities {
        let Some(subject_class) = partition_class(node_index, partition, &cardinality.subject)
        else {
            return false;
        };
        let qualifying_targets: BTreeSet<usize> = role_edges
            .iter()
            .filter(|(subject, property, _)| {
                *subject == subject_class && property == &cardinality.property
            })
            .filter_map(|(_, _, object)| {
                partition_class_satisfies_qualifier(
                    &class_memberships,
                    *object,
                    &cardinality.qualifier,
                )
                .then_some(*object)
            })
            .collect();
        let count = qualifying_targets.len() as u32;
        if cardinality.minimum.is_some_and(|minimum| count < minimum) {
            return false;
        }
        if cardinality.maximum.is_some_and(|maximum| count > maximum) {
            return false;
        }
    }

    true
}

fn same_partition(
    node_index: &HashMap<String, usize>,
    partition: &[Option<usize>],
    left: &str,
    right: &str,
) -> bool {
    match (
        partition_class(node_index, partition, left),
        partition_class(node_index, partition, right),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn partition_class(
    node_index: &HashMap<String, usize>,
    partition: &[Option<usize>],
    node: &str,
) -> Option<usize> {
    partition.get(*node_index.get(node)?)?.as_ref().copied()
}

fn partition_class_satisfies_qualifier(
    class_memberships: &HashMap<usize, BTreeSet<String>>,
    individual_class: usize,
    qualifier: &Qualifier,
) -> bool {
    match qualifier {
        Qualifier::Any | Qualifier::Top => true,
        Qualifier::Bottom => false,
        Qualifier::Named(class) => class_memberships
            .get(&individual_class)
            .is_some_and(|classes| classes.contains(class)),
        Qualifier::All(qualifiers) => qualifiers.iter().all(|qualifier| {
            partition_class_satisfies_qualifier(class_memberships, individual_class, qualifier)
        }),
    }
}

fn qualifying_targets(facts: &QualifiedFacts, cardinality: &CardinalityAssertion) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for (subject, property, object) in &facts.role_edges {
        if subject == &cardinality.subject
            && property == &cardinality.property
            && individual_satisfies_qualifier(facts, object, &cardinality.qualifier)
        {
            targets.insert(object.clone());
        }
    }
    targets.into_iter().collect()
}

fn individual_satisfies_qualifier(
    facts: &QualifiedFacts,
    individual: &str,
    qualifier: &Qualifier,
) -> bool {
    match qualifier {
        Qualifier::Any | Qualifier::Top => true,
        Qualifier::Bottom => false,
        Qualifier::Named(class) => facts
            .class_memberships
            .get(individual)
            .is_some_and(|classes| classes.contains(class)),
        Qualifier::All(qualifiers) => qualifiers
            .iter()
            .all(|qualifier| individual_satisfies_qualifier(facts, individual, qualifier)),
    }
}

fn qualifier_is_unsatisfiable(facts: &QualifiedFacts, qualifier: &Qualifier) -> bool {
    match qualifier {
        Qualifier::Bottom => true,
        Qualifier::Named(class) => facts
            .disjoint_class_pairs
            .contains(&ordered_pair_str(OWL_THING_IRI, class)),
        Qualifier::All(qualifiers) => {
            if qualifiers
                .iter()
                .any(|qualifier| qualifier_is_unsatisfiable(facts, qualifier))
            {
                return true;
            }
            for (index, left) in qualifiers.iter().enumerate() {
                for right in qualifiers.iter().skip(index + 1) {
                    if qualifier_pair_is_disjoint(facts, left, right) {
                        return true;
                    }
                }
            }
            false
        }
        Qualifier::Any | Qualifier::Top => false,
    }
}

fn qualifier_pair_is_disjoint(facts: &QualifiedFacts, left: &Qualifier, right: &Qualifier) -> bool {
    match (left, right) {
        (Qualifier::Named(left), Qualifier::Named(right)) => facts
            .disjoint_class_pairs
            .contains(&ordered_pair_str(left, right)),
        (Qualifier::All(left), _) => left
            .iter()
            .any(|left| qualifier_pair_is_disjoint(facts, left, right)),
        (_, Qualifier::All(right)) => right
            .iter()
            .any(|right| qualifier_pair_is_disjoint(facts, left, right)),
        _ => false,
    }
}

fn reject_unsupported_cross_qualifier_bounds(facts: &QualifiedFacts) -> OwlResult<()> {
    for (index, left) in facts.cardinalities.iter().enumerate() {
        for right in facts.cardinalities.iter().skip(index + 1) {
            if left.subject != right.subject
                || left.property != right.property
                || left.qualifier == right.qualifier
            {
                continue;
            }
            let left_generates = left.minimum.is_some_and(|minimum| minimum > 0);
            let right_generates = right.minimum.is_some_and(|minimum| minimum > 0);
            let left_caps = left.maximum.is_some();
            let right_caps = right.maximum.is_some();
            if (left_generates && right_caps) || (right_generates && left_caps) {
                return unsupported(
                    "cross-qualifier min/max interaction needs unsupported subset reasoning",
                );
            }
        }
    }
    Ok(())
}

fn known_distinctness_exceeds_maximum(
    facts: &QualifiedFacts,
    targets: &[String],
    maximum: usize,
) -> OwlResult<bool> {
    if maximum == 0 {
        return Ok(!targets.is_empty());
    }
    if targets.len() <= maximum {
        return Ok(false);
    }
    if targets.len() > MAX_MODEL_NODES {
        return Err(OwlError::ResourceLimitExceeded {
            resource_type: "qualified_consistency_distinct_targets".to_string(),
            limit: MAX_MODEL_NODES,
            message: format!(
                "{} candidate fillers exceed bounded distinctness proof",
                targets.len()
            ),
        });
    }

    let mut order: Vec<usize> = (0..targets.len()).collect();
    order.sort_by_key(|index| {
        std::cmp::Reverse(
            targets
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != *index)
                .filter(|(_, target)| are_semantically_distinct(facts, &targets[*index], target))
                .count(),
        )
    });
    let mut colors = vec![None; targets.len()];
    let mut steps = 0_usize;
    Ok(!color_distinctness_graph(
        facts,
        targets,
        &order,
        &mut colors,
        0,
        maximum,
        &mut steps,
    )?)
}

fn color_distinctness_graph(
    facts: &QualifiedFacts,
    targets: &[String],
    order: &[usize],
    colors: &mut [Option<usize>],
    position: usize,
    color_count: usize,
    steps: &mut usize,
) -> OwlResult<bool> {
    *steps += 1;
    if *steps > MAX_PARTITION_STEPS {
        return Err(OwlError::ResourceLimitExceeded {
            resource_type: "qualified_consistency_distinctness_steps".to_string(),
            limit: MAX_PARTITION_STEPS,
            message: "bounded distinctness proof exceeded its step budget".to_string(),
        });
    }
    if position == order.len() {
        return Ok(true);
    }
    let target_index = order[position];
    for color in 0..color_count {
        let conflicts = colors.iter().enumerate().any(|(other_index, other_color)| {
            other_color.is_some_and(|other_color| {
                other_color == color
                    && are_semantically_distinct(
                        facts,
                        &targets[target_index],
                        &targets[other_index],
                    )
            })
        });
        if !conflicts {
            colors[target_index] = Some(color);
            if color_distinctness_graph(
                facts,
                targets,
                order,
                colors,
                position + 1,
                color_count,
                steps,
            )? {
                return Ok(true);
            }
            colors[target_index] = None;
        }
    }
    Ok(false)
}

fn are_semantically_distinct(facts: &QualifiedFacts, left: &str, right: &str) -> bool {
    if left == right {
        return false;
    }
    facts
        .different_pairs
        .contains(&ordered_pair_str(left, right))
        || facts
            .class_memberships
            .get(left)
            .into_iter()
            .flatten()
            .any(|left_class| {
                facts
                    .class_memberships
                    .get(right)
                    .into_iter()
                    .flatten()
                    .any(|right_class| {
                        facts
                            .disjoint_class_pairs
                            .contains(&ordered_pair_str(left_class, right_class))
                    })
            })
}

fn qualifier_key_optional(qualifier: Option<&ClassExpression>) -> OwlResult<Qualifier> {
    qualifier.map_or(Ok(Qualifier::Any), qualifier_key)
}

fn qualifier_key(expression: &ClassExpression) -> OwlResult<Qualifier> {
    match expression {
        ClassExpression::Class(class) if class.iri().as_str() == OWL_THING_IRI => {
            Ok(Qualifier::Top)
        }
        ClassExpression::Class(class) if class.iri().as_str() == OWL_NOTHING_IRI => {
            Ok(Qualifier::Bottom)
        }
        ClassExpression::Class(class) => Ok(Qualifier::Named(class.iri().as_str().to_string())),
        ClassExpression::ObjectIntersectionOf(operands) => {
            let mut keys = Vec::with_capacity(operands.len());
            for operand in operands {
                keys.push(qualifier_key(operand)?);
            }
            keys.sort();
            keys.dedup();
            if keys.iter().any(|key| matches!(key, Qualifier::Bottom)) {
                return Ok(Qualifier::Bottom);
            }
            keys.retain(|key| !matches!(key, Qualifier::Top));
            Ok(if keys.is_empty() {
                Qualifier::Top
            } else {
                Qualifier::All(keys)
            })
        }
        _ => unsupported("qualifier is outside named/top/bottom conjunction fragment"),
    }
}

fn named_object_property_iri(property: &ObjectPropertyExpression) -> OwlResult<&str> {
    match property {
        ObjectPropertyExpression::ObjectProperty(property)
            if property.iri().as_str() == OWL_TOP_OBJECT_PROPERTY_IRI =>
        {
            unsupported("topObjectProperty is outside bounded qualified consistency")
        }
        ObjectPropertyExpression::ObjectProperty(property) => Ok(property.iri().as_str()),
        ObjectPropertyExpression::ObjectInverseOf(_) => {
            unsupported("inverse object properties are unsupported in qualified consistency")
        }
    }
}

fn ordered_pair(left: &IRI, right: &IRI) -> (String, String) {
    ordered_pair_str(left.as_str(), right.as_str())
}

fn ordered_pair_str(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn unsupported<T>(message: &str) -> OwlResult<T> {
    Err(OwlError::UnsupportedFeature(message.to_string()))
}

#[cfg(test)]
mod fragment_critics {
    //! Guards for the bounded qualified checker's own fragment policy,
    //! moved from the integration suite when #113 made a declined
    //! fragment fall through to the sequential layers: these must keep
    //! declining (or hitting their budget) so the layers below know the
    //! checker has no verdict.

    use super::*;
    use crate::core::entities::{Class, NamedIndividual, ObjectProperty};
    use crate::core::ontology::Ontology;
    use crate::logic::axioms::core::IrreflexivePropertyAxiom;
    use crate::logic::axioms::{
        Axiom, ClassAssertionAxiom, NegativeObjectPropertyAssertionAxiom,
        ObjectPropertyExpression, PropertyAssertionAxiom,
    };
    use smallvec::smallvec;

    fn named(name: &str) -> ClassExpression {
        ClassExpression::Class(Class::new(format!("http://test.org/{name}")))
    }

    fn thing() -> ClassExpression {
        ClassExpression::Class(Class::new("http://www.w3.org/2002/07/owl#Thing"))
    }

    fn and(operands: Vec<ClassExpression>) -> ClassExpression {
        ClassExpression::ObjectIntersectionOf(operands.into_iter().map(Box::new).collect())
    }

    fn property(name: &str) -> ObjectProperty {
        ObjectProperty::new(format!("http://test.org/{name}"))
    }

    fn property_expr(name: &str) -> Box<ObjectPropertyExpression> {
        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
            property(name),
        )))
    }

    fn individual(name: &str) -> NamedIndividual {
        NamedIndividual::new(format!("http://test.org/{name}"))
    }

    fn assert_class(ontology: &mut Ontology, who: &NamedIndividual, expression: ClassExpression) {
        ontology.add_class_assertion(ClassAssertionAxiom::new(who.iri().clone(), expression)).unwrap();
    }

    fn assert_edge(ontology: &mut Ontology, from: &NamedIndividual, with: &ObjectProperty, to: &NamedIndividual) {
        ontology.add_property_assertion(PropertyAssertionAxiom::new(
            from.iri().clone(), with.iri().clone(), to.iri().clone(),
        )).unwrap();
    }

    fn assert_negative_edge(ontology: &mut Ontology, from: &NamedIndividual, with: &ObjectProperty, to: &NamedIndividual) {
        ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
            NegativeObjectPropertyAssertionAxiom::new(
                from.iri().clone(), with.iri().clone(), to.iri().clone(),
            ),
        ))).unwrap();
    }

#[test]
fn cross_qualifier_min_max_interaction_is_unsupported() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            2,
            property_expr("relation"),
            Some(Box::new(and(vec![named("Target"), named("Enabled")]))),
        ),
    );
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(named("Target"))),
        ),
    );

    // The bounded checker declines the cross-qualifier interaction.
    assert!(matches!(
        check_qualified_consistency(&ontology),
        Err(OwlError::UnsupportedFeature(_))
    ));
}

#[test]
fn critic_min_conjunctive_qualifier_under_narrower_max_is_not_reported_sat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            0,
            property_expr("relation"),
            Some(Box::new(named("C"))),
        ),
    );
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMinCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(and(vec![named("C"), named("D")]))),
        ),
    );

    assert!(matches!(
        check_qualified_consistency(&ontology),
        Err(OwlError::UnsupportedFeature(_))
    ));
}

#[test]
fn max_merge_that_creates_irreflexive_role_clash_is_not_reported_sat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let relation = property("relation");
    let side = property("side");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_edge(&mut ontology, &left, &side, &right);
    ontology
        .add_axiom(Axiom::IrreflexiveProperty(Box::new(
            IrreflexivePropertyAxiom::new(side.iri().clone()),
        )))
        .unwrap();
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(thing())),
        ),
    );

    assert!(matches!(
        check_qualified_consistency(&ontology),
        Err(OwlError::UnsupportedFeature(_))
    ));
}

#[test]
fn deeply_nested_qualifier_returns_resource_error_before_recursive_conversion() {
    let mut qualifier = named("Target");
    for _ in 0..256 {
        qualifier = ClassExpression::ObjectIntersectionOf(smallvec![Box::new(qualifier)]);
    }
    let mut ontology = Ontology::new();
    assert_class(
        &mut ontology,
        &individual("owner"),
        ClassExpression::ObjectMinCardinality(1, property_expr("r"), Some(Box::new(qualifier))),
    );
    assert!(matches!(
        check_qualified_consistency(&ontology),
        Err(OwlError::ResourceLimitExceeded { .. })
    ));
}

#[test]
fn max_merge_that_creates_negative_role_clash_is_not_reported_sat() {
    let mut ontology = Ontology::new();
    let owner = individual("owner");
    let left = individual("left");
    let right = individual("right");
    let object = individual("object");
    let relation = property("relation");
    let side = property("side");

    assert_edge(&mut ontology, &owner, &relation, &left);
    assert_edge(&mut ontology, &owner, &relation, &right);
    assert_edge(&mut ontology, &left, &side, &object);
    assert_negative_edge(&mut ontology, &right, &side, &object);
    assert_class(
        &mut ontology,
        &owner,
        ClassExpression::ObjectMaxCardinality(
            1,
            property_expr("relation"),
            Some(Box::new(thing())),
        ),
    );

    assert!(matches!(
        check_qualified_consistency(&ontology),
        Err(OwlError::UnsupportedFeature(_))
    ));
}
}
