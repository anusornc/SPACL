//! QL Profile Validator
//!
//! Rejects the constructs the OWL 2 QL profile excludes, so the reasoner
//! router only takes the QL path for ontologies that genuinely sit in
//! QL (#167; previously every ontology validated, #166 Layer 1).

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression, ObjectPropertyExpression};
use std::sync::Arc;

/// QL Profile Validator
#[derive(Debug, Clone)]
pub struct QLProfileValidator {
    _ontology: Arc<Ontology>,
}

impl QLProfileValidator {
    /// Create a new QL profile validator
    pub fn new(ontology: Arc<Ontology>) -> Self {
        Self {
            _ontology: ontology,
        }
    }
}

/// Axioms OWL 2 QL does not support: property chains, inverse-
/// functional roles, and transitive roles (the QL fragment carries role
/// inclusions, inverses, and disjointness, but no transitivity).
/// Conservative: anything uncertain stays unflagged, so a rejection
/// demotes an ontology to Full rather than wrongly claiming QL.
fn forbidden_axiom(axiom: &Axiom) -> Option<&'static str> {
    match axiom {
        Axiom::SubPropertyChainOf(_) => Some("SubPropertyChainOf"),
        Axiom::InverseFunctionalProperty(_) => Some("InverseFunctionalProperty"),
        Axiom::TransitiveProperty(_) => Some("TransitiveProperty"),
        _ => None,
    }
}

/// Class expressions outside OWL 2 QL in any position: universal
/// restrictions and exact cardinality (object and data sides).
fn forbidden_anywhere_expression(expression: &ClassExpression) -> Option<&'static str> {
    match expression {
        ClassExpression::ObjectAllValuesFrom(_, _) => Some("ObjectAllValuesFrom"),
        ClassExpression::ObjectExactCardinality(_, _, _) => Some("ObjectExactCardinality"),
        ClassExpression::DataAllValuesFrom(_, _) => Some("DataAllValuesFrom"),
        ClassExpression::DataExactCardinality(_, _, _) => Some("DataExactCardinality"),
        _ => None,
    }
}

/// QL is superclass-side restricted: existentials, Boolean class
/// expressions, oneOf, and hasSelf are legal only in subclass position.
fn forbidden_super_expression(expression: &ClassExpression) -> Option<&'static str> {
    match expression {
        ClassExpression::ObjectSomeValuesFrom(_, _) => Some("ObjectSomeValuesFrom"),
        ClassExpression::ObjectUnionOf(_) => Some("ObjectUnionOf"),
        ClassExpression::ObjectComplementOf(_) => Some("ObjectComplementOf"),
        ClassExpression::ObjectOneOf(_) => Some("ObjectOneOf"),
        ClassExpression::ObjectHasSelf(_) => Some("ObjectHasSelf"),
        _ => None,
    }
}

/// Inverse roles are expressible in QL.
fn forbidden_property_expression(_: &ObjectPropertyExpression) -> Option<&'static str> {
    None
}

static QL_RULE_TABLE: ProfileRuleTable = ProfileRuleTable {
    profile: Owl2Profile::QL,
    forbidden_axiom,
    forbidden_anywhere_expression,
    forbidden_super_expression,
    forbidden_property_expression,
};

impl ProfileValidator for QLProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        Ok(validate_with_rules(ontology, &QL_RULE_TABLE))
    }

    fn profile(&self) -> Owl2Profile {
        Owl2Profile::QL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ql_validator_creation() {
        let ontology = Arc::new(Ontology::new());
        let validator = QLProfileValidator::new(ontology);
        assert_eq!(validator.profile(), Owl2Profile::QL);
    }
}
