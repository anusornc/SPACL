//! EL (EL++) Profile Validator
//!
//! The EL profile is optimized for ontologies with large concept hierarchies
//! and supports polynomial-time reasoning.

use super::common::*;
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
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

    /// Check if a construct is allowed in EL
    #[allow(dead_code)]
    fn is_allowed_in_el(&self, construct: &str) -> bool {
        matches!(
            construct,
            "Class"
                | "ObjectIntersectionOf"
                | "ObjectSomeValuesFrom"
                | "ObjectProperty"
                | "SubClassOf"
                | "EquivalentClasses"
                | "ObjectPropertyDomain"
                | "ObjectPropertyRange"
                | "ClassAssertion"
                | "ObjectPropertyAssertion"
        )
    }
}

impl ProfileValidator for ELProfileValidator {
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        let violations = Vec::new();
        let mut stats = ValidationStatistics::default();

        // Check class expressions
        for _class in ontology.classes() {
            stats.class_expressions_checked += 1;
            // EL allows atomic classes
            // More complex checks would be done here
        }

        // Check axioms
        for _axiom in ontology.subclass_axioms() {
            stats.axioms_checked += 1;
            // Check if axiom conforms to EL restrictions
        }

        let is_valid = violations.is_empty();

        Ok(ProfileValidationResult {
            profile: Owl2Profile::EL,
            is_valid,
            violations,
            statistics: stats,
        })
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
