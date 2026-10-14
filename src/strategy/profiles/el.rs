//! EL (EL++) Profile Validator
//!
//! Rejects the constructs the OWL 2 EL profile excludes, so the reasoner
//! router only takes the EL fast path for ontologies that genuinely sit
//! in EL (#167; previously every ontology validated, #166 Layer 1).

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression, ObjectPropertyExpression};
use std::sync::Arc;

/// EL Profile Validator
#[derive(Debug, Clone)]
pub struct ELProfileValidator {
    _ontology: Arc<Ontology>,
}

impl ELProfileValidator {
    /// Create a new EL profile validator
    pub fn new(ontology: Arc<Ontology>) -> Self {
        Self {
            _ontology: ontology,
        }
    }
}

/// Axioms OWL 2 EL does not support (profile specification, EL axiom
/// table): inverse declarations, property chains, object property
/// functionality, inverse functionality, symmetry and asymmetry,
/// irreflexivity, disjoint object properties, and negative property
/// assertions. Transitivity, reflexivity, keys, and data-property
/// functionality stay allowed.
fn forbidden_axiom(axiom: &Axiom) -> Option<&'static str> {
    match axiom {
        Axiom::InverseObjectProperties(_) => Some("InverseObjectProperties"),
        Axiom::SubPropertyChainOf(_) => Some("SubPropertyChainOf"),
        Axiom::FunctionalProperty(_) => Some("FunctionalProperty"),
        Axiom::InverseFunctionalProperty(_) => Some("InverseFunctionalProperty"),
        Axiom::SymmetricProperty(_) => Some("SymmetricProperty"),
        Axiom::AsymmetricProperty(_) => Some("AsymmetricProperty"),
        Axiom::IrreflexiveProperty(_) => Some("IrreflexiveProperty"),
        Axiom::DisjointObjectProperties(_) => Some("DisjointObjectProperties"),
        Axiom::NegativeObjectPropertyAssertion(_) => Some("NegativeObjectPropertyAssertion"),
        Axiom::NegativeDataPropertyAssertion(_) => Some("NegativeDataPropertyAssertion"),
        _ => None,
    }
}

/// Class expressions outside OWL 2 EL in any position: disjunction,
/// complement, universal restriction, and max/exact cardinality (object
/// and data sides). Existentials, intersections, oneOf, hasValue,
/// hasSelf, and min cardinality (any n, qualified) stay allowed.
fn forbidden_anywhere_expression(expression: &ClassExpression) -> Option<&'static str> {
    match expression {
        ClassExpression::ObjectUnionOf(_) => Some("ObjectUnionOf"),
        ClassExpression::ObjectComplementOf(_) => Some("ObjectComplementOf"),
        ClassExpression::ObjectAllValuesFrom(_, _) => Some("ObjectAllValuesFrom"),
        ClassExpression::ObjectMaxCardinality(_, _, _) => Some("ObjectMaxCardinality"),
        ClassExpression::ObjectExactCardinality(_, _, _) => Some("ObjectExactCardinality"),
        ClassExpression::DataAllValuesFrom(_, _) => Some("DataAllValuesFrom"),
        ClassExpression::DataMaxCardinality(_, _, _) => Some("DataMaxCardinality"),
        ClassExpression::DataExactCardinality(_, _, _) => Some("DataExactCardinality"),
        _ => None,
    }
}

/// EL has no superclass-side relaxations: what is forbidden is
/// forbidden everywhere, so the super table adds nothing.
fn forbidden_super_expression(_: &ClassExpression) -> Option<&'static str> {
    None
}

/// Inverse roles are not expressible in OWL 2 EL.
fn forbidden_property_expression(property: &ObjectPropertyExpression) -> Option<&'static str> {
    match property {
        ObjectPropertyExpression::ObjectInverseOf(_) => Some("ObjectInverseOf"),
        ObjectPropertyExpression::ObjectProperty(_) => None,
    }
}

static EL_RULE_TABLE: ProfileRuleTable = ProfileRuleTable {
    profile: Owl2Profile::EL,
    forbidden_axiom,
    forbidden_anywhere_expression,
    forbidden_super_expression,
    forbidden_property_expression,
};

impl ProfileValidator for ELProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        Ok(validate_with_rules(ontology, &EL_RULE_TABLE))
    }

    fn profile(&self) -> Owl2Profile {
        Owl2Profile::EL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_el_validator_creation() {
        let ontology = Arc::new(Ontology::new());
        let validator = ELProfileValidator::new(ontology);
        assert_eq!(validator.profile(), Owl2Profile::EL);
    }
}
