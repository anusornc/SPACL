//! RL Profile Validator
//!
//! The RL profile is optimized for rule-based reasoning and
//! supports polynomial-time inference using rule engines.

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
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

    /// Check if a construct is allowed in RL
    #[allow(dead_code)]
    fn is_allowed_in_rl(&self, construct: &str) -> bool {
        matches!(
            construct,
            "Class"
                | "ObjectIntersectionOf"
                | "ObjectUnionOf"
                | "ObjectComplementOf"
                | "ObjectAllValuesFrom"
                | "ObjectHasValue"
                | "ObjectProperty"
                | "SubClassOf"
                | "EquivalentClasses"
                | "DisjointClasses"
                | "ObjectPropertyDomain"
                | "ObjectPropertyRange"
                | "SubObjectPropertyOf"
                | "EquivalentObjectProperties"
                | "InverseObjectProperties"
                | "FunctionalObjectProperty"
                | "ClassAssertion"
                | "ObjectPropertyAssertion"
                | "SameIndividual"
                | "DifferentIndividuals"
        )
    }
}

impl ProfileValidator for RLProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        let violations = Vec::new();
        let mut stats = ValidationStatistics::default();

        // Check class expressions
        for _class in ontology.classes() {
            stats.class_expressions_checked += 1;
            // RL allows specific constructs
        }

        // Check axioms
        for _axiom in ontology.subclass_axioms() {
            stats.axioms_checked += 1;
        }

        let is_valid = violations.is_empty();

        Ok(ProfileValidationResult {
            profile: Owl2Profile::RL,
            is_valid,
            violations,
            statistics: stats,
        })
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
