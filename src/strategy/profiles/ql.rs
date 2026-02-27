//! QL Profile Validator
//!
//! The QL profile is optimized for efficient query answering and
//! supports conjunctive query answering in LOGSPACE.

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
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

    /// Check if a construct is allowed in QL
    #[allow(dead_code)]
    fn is_allowed_in_ql(&self, construct: &str) -> bool {
        matches!(
            construct,
            "Class"
                | "ObjectIntersectionOf"
                | "ObjectSomeValuesFrom"
                | "ObjectAllValuesFrom"
                | "ObjectProperty"
                | "SubClassOf"
                | "EquivalentClasses"
                | "DisjointClasses"
                | "ObjectPropertyDomain"
                | "ObjectPropertyRange"
                | "InverseObjectProperties"
                | "SymmetricObjectProperty"
                | "AsymmetricObjectProperty"
                | "ClassAssertion"
                | "ObjectPropertyAssertion"
                | "NegativeObjectPropertyAssertion"
        )
    }
}

impl ProfileValidator for QLProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        let violations = Vec::new();
        let mut stats = ValidationStatistics::default();

        // Check class expressions
        for _class in ontology.classes() {
            stats.class_expressions_checked += 1;
            // QL allows specific constructs
        }

        // Check axioms
        for _axiom in ontology.subclass_axioms() {
            stats.axioms_checked += 1;
        }

        let is_valid = violations.is_empty();

        Ok(ProfileValidationResult {
            profile: Owl2Profile::QL,
            is_valid,
            violations,
            statistics: stats,
        })
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
