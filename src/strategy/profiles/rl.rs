//! RL Profile Validator
//!
//! Rejects the constructs the OWL 2 RL profile excludes, so the reasoner
//! router only takes the RL path for ontologies that genuinely sit in
//! RL (#167; previously every ontology validated, #166 Layer 1).
//!
//! Note what RL *supports* (profile specification, RL axiom table):
//! property chains (prp-spo2 is an RL rule — this repo's own closure
//! fragment composes them under the RL banner), inverse-functional
//! roles, transitivity, symmetry, and role disjointness. The excluded
//! surface is on the superclass side: existentials, universals, and
//! hasSelf cannot conclude there, because Datalog rules cannot head on
//! a fresh existential witness.

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression, ObjectPropertyExpression};
use std::sync::Arc;

/// RL Profile Validator
#[derive(Debug, Clone)]
pub struct RLProfileValidator {
    _ontology: Arc<Ontology>,
}

impl RLProfileValidator {
    /// Create a new RL profile validator
    pub fn new(ontology: Arc<Ontology>) -> Self {
        Self {
            _ontology: ontology,
        }
    }
}

/// No axiom-level exclusion is certain enough for RL: the obvious
/// candidates (chains, inverse-functionality, transitivity, symmetry)
/// are all supported with side conditions. Deliberately empty.
fn forbidden_axiom(_: &Axiom) -> Option<&'static str> {
    None
}

/// No position-free class-expression exclusion is certain for RL (its
/// restrictions are position-dependent: Boolean expressions are legal
/// in subclass position). Deliberately empty.
fn forbidden_anywhere_expression(_: &ClassExpression) -> Option<&'static str> {
    None
}

/// RL's superclass side is the restricted one: existential and
/// universal restrictions and hasSelf cannot appear there (a rule head
/// cannot introduce a witness). Everything else is delegated to the
/// conservative policy: uncertain rules stay unflagged.
fn forbidden_super_expression(expression: &ClassExpression) -> Option<&'static str> {
    match expression {
        ClassExpression::ObjectSomeValuesFrom(_, _) => Some("ObjectSomeValuesFrom"),
        ClassExpression::ObjectAllValuesFrom(_, _) => Some("ObjectAllValuesFrom"),
        ClassExpression::ObjectHasSelf(_) => Some("ObjectHasSelf"),
        _ => None,
    }
}

/// Inverse roles are expressible in RL.
fn forbidden_property_expression(_: &ObjectPropertyExpression) -> Option<&'static str> {
    None
}

static RL_RULE_TABLE: ProfileRuleTable = ProfileRuleTable {
    profile: Owl2Profile::RL,
    forbidden_axiom,
    forbidden_anywhere_expression,
    forbidden_super_expression,
    forbidden_property_expression,
};

impl ProfileValidator for RLProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        Ok(validate_with_rules(ontology, &RL_RULE_TABLE))
    }

    fn profile(&self) -> Owl2Profile {
        Owl2Profile::RL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rl_validator_creation() {
        let ontology = Arc::new(Ontology::new());
        let validator = RLProfileValidator::new(ontology);
        assert_eq!(validator.profile(), Owl2Profile::RL);
    }
}
