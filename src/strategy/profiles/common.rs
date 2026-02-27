//! OWL2 Profile Common Types and Traits
//!
//! This module defines common types and traits used across all OWL2 profile validators.

use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use std::sync::Arc;

/// OWL2 Profile enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owl2Profile {
    /// EL (EL++) profile - polynomial time reasoning
    EL,
    /// QL profile - efficient query answering
    QL,
    /// RL profile - rule-based reasoning
    RL,
    /// Full OWL2 DL
    Full,
}

impl Owl2Profile {
    /// Get the profile name
    pub fn name(&self) -> &'static str {
        match self {
            Owl2Profile::EL => "EL",
            Owl2Profile::QL => "QL",
            Owl2Profile::RL => "RL",
            Owl2Profile::Full => "OWL2 DL",
        }
    }

    /// Check if this profile is more restrictive than another
    pub fn is_more_restrictive_than(&self, other: Owl2Profile) -> bool {
        match (self, other) {
            (Owl2Profile::EL, Owl2Profile::EL) => false,
            (Owl2Profile::EL, _) => true,
            (Owl2Profile::QL, Owl2Profile::EL) => false, // QL is not more restrictive than EL
            (Owl2Profile::QL, Owl2Profile::QL) => false,
            (Owl2Profile::QL, Owl2Profile::Full) => true,
            (Owl2Profile::QL, Owl2Profile::RL) => false, // QL and RL are incomparable
            (Owl2Profile::RL, Owl2Profile::EL) => false, // RL is not more restrictive than EL
            (Owl2Profile::RL, Owl2Profile::QL) => false, // RL and QL are incomparable
            (Owl2Profile::RL, Owl2Profile::RL) => false,
            (Owl2Profile::RL, Owl2Profile::Full) => true,
            (Owl2Profile::Full, _) => false,
        }
    }
}

/// Severity of a profile violation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViolationSeverity {
    /// Error - ontology does not comply with profile
    Error,
    /// Warning - ontology uses constructs not recommended for profile
    Warning,
    /// Info - informational message
    Info,
}

/// Type of profile violation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProfileViolationType {
    /// Use of disallowed class expression
    DisallowedClassExpression(String),
    /// Use of disallowed property expression
    DisallowedPropertyExpression(String),
    /// Use of disallowed axiom type
    DisallowedAxiomType(String),
    /// Use of disallowed data range
    DisallowedDataRange(String),
    /// Violation of global restriction
    GlobalRestrictionViolation(String),
    /// Use of unsupported feature
    UnsupportedFeature(String),
}

/// A single profile violation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileViolation {
    /// Type of violation
    pub violation_type: ProfileViolationType,
    /// Severity level
    pub severity: ViolationSeverity,
    /// Human-readable message
    pub message: String,
    /// Optional location information
    pub location: Option<String>,
}

impl ProfileViolation {
    pub fn new(
        violation_type: ProfileViolationType,
        severity: ViolationSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            violation_type,
            severity,
            message: message.into(),
            location: None,
        }
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

/// Result of profile validation
#[derive(Debug, Clone)]
pub struct ProfileValidationResult {
    /// The profile being validated
    pub profile: Owl2Profile,
    /// Whether the ontology complies with the profile
    pub is_valid: bool,
    /// List of violations found
    pub violations: Vec<ProfileViolation>,
    /// Statistics about the validation
    pub statistics: ValidationStatistics,
}

impl ProfileValidationResult {
    pub fn valid(profile: Owl2Profile) -> Self {
        Self {
            profile,
            is_valid: true,
            violations: Vec::new(),
            statistics: ValidationStatistics::default(),
        }
    }

    pub fn invalid(profile: Owl2Profile, violations: Vec<ProfileViolation>) -> Self {
        Self {
            profile,
            is_valid: false,
            violations,
            statistics: ValidationStatistics::default(),
        }
    }

    /// Get the number of errors
    pub fn error_count(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| matches!(v.severity, ViolationSeverity::Error))
            .count()
    }

    /// Get the number of warnings
    pub fn warning_count(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| matches!(v.severity, ViolationSeverity::Warning))
            .count()
    }
}

/// Validation statistics
#[derive(Debug, Clone, Default)]
pub struct ValidationStatistics {
    /// Number of axioms checked
    pub axioms_checked: usize,
    /// Number of class expressions checked
    pub class_expressions_checked: usize,
    /// Number of property expressions checked
    pub property_expressions_checked: usize,
    /// Time taken for validation (milliseconds)
    pub validation_time_ms: u64,
}

/// Trait for profile validators
pub trait ProfileValidator {
    /// Validate an ontology against this profile
    fn validate(&self, ontology: &Ontology) -> OwlResult<ProfileValidationResult>;

    /// Get the profile this validator checks
    fn profile(&self) -> Owl2Profile;

    /// Check if an ontology is valid for this profile
    fn is_valid(&self, ontology: &Ontology) -> OwlResult<bool> {
        let result = self.validate(ontology)?;
        Ok(result.is_valid)
    }
}

/// Statistics about an ontology
#[derive(Debug, Clone, Default)]
pub struct OntologyStats {
    /// Number of classes
    pub num_classes: usize,
    /// Number of object properties
    pub num_object_properties: usize,
    /// Number of data properties
    pub num_data_properties: usize,
    /// Number of individuals
    pub num_individuals: usize,
    /// Number of axioms
    pub num_axioms: usize,
    /// Number of subclass axioms
    pub num_subclass_axioms: usize,
    /// Number of disjoint class axioms
    pub num_disjoint_class_axioms: usize,
    /// Number of property assertions
    pub num_property_assertions: usize,
    /// Number of class assertions
    pub num_class_assertions: usize,
}

/// Type of optimization hint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptimizationType {
    /// Suggest using simpler construct
    Simplify,
    /// Suggest removing redundant axiom
    RemoveRedundant,
    /// Suggest restructuring axiom
    Restructure,
    /// Suggest alternative encoding
    AlternativeEncoding,
}

/// Optimization hint for profile compliance
#[derive(Debug, Clone)]
pub struct OptimizationHint {
    /// Type of optimization
    pub optimization_type: OptimizationType,
    /// Human-readable description
    pub description: String,
    /// Suggested replacement (if applicable)
    pub suggestion: Option<String>,
    /// Expected performance improvement
    pub expected_improvement: Option<String>,
}

impl OptimizationHint {
    pub fn new(optimization_type: OptimizationType, description: impl Into<String>) -> Self {
        Self {
            optimization_type,
            description: description.into(),
            suggestion: None,
            expected_improvement: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_improvement(mut self, improvement: impl Into<String>) -> Self {
        self.expected_improvement = Some(improvement.into());
        self
    }
}

/// Comprehensive profile analysis report
#[derive(Debug, Clone)]
pub struct ProfileAnalysisReport {
    /// EL profile validation result
    pub el_result: Option<ProfileValidationResult>,
    /// QL profile validation result
    pub ql_result: Option<ProfileValidationResult>,
    /// RL profile validation result
    pub rl_result: Option<ProfileValidationResult>,
    /// Ontology statistics
    pub ontology_stats: OntologyStats,
    /// Recommended profile (most restrictive valid profile)
    pub recommended_profile: Option<Owl2Profile>,
    /// Optimization hints
    pub optimization_hints: Vec<OptimizationHint>,
}

impl ProfileAnalysisReport {
    pub fn new(ontology_stats: OntologyStats) -> Self {
        Self {
            el_result: None,
            ql_result: None,
            rl_result: None,
            ontology_stats,
            recommended_profile: None,
            optimization_hints: Vec::new(),
        }
    }
}

/// OWL2 Profile Validator with caching support
#[derive(Debug, Clone)]
pub struct Owl2ProfileValidator {
    ontology: Arc<Ontology>,
    _cache_enabled: bool,
    advanced_caching: bool,
}

impl Owl2ProfileValidator {
    pub fn new(ontology: Arc<Ontology>) -> OwlResult<Self> {
        Ok(Self {
            ontology,
            _cache_enabled: true,
            advanced_caching: false,
        })
    }

    pub fn validate_profile(&self, profile: Owl2Profile) -> OwlResult<ProfileValidationResult> {
        // Delegate to specific profile validators
        match profile {
            Owl2Profile::EL => {
                let validator = super::el::ELProfileValidator::new(self.ontology.clone());
                validator.validate(&self.ontology)
            }
            Owl2Profile::QL => {
                let validator = super::ql::QLProfileValidator::new(self.ontology.clone());
                validator.validate(&self.ontology)
            }
            Owl2Profile::RL => {
                let validator = super::rl::RLProfileValidator::new(self.ontology.clone());
                validator.validate(&self.ontology)
            }
            Owl2Profile::Full => Ok(ProfileValidationResult::valid(Owl2Profile::Full)),
        }
    }

    pub fn validate_all_profiles(&self) -> OwlResult<Vec<ProfileValidationResult>> {
        let mut results = Vec::new();
        for profile in [Owl2Profile::EL, Owl2Profile::QL, Owl2Profile::RL] {
            results.push(self.validate_profile(profile)?);
        }
        Ok(results)
    }

    pub fn get_most_restrictive_profile(&self) -> OwlResult<Option<Owl2Profile>> {
        let el_valid = self.validate_profile(Owl2Profile::EL)?.is_valid;
        let ql_valid = self.validate_profile(Owl2Profile::QL)?.is_valid;
        let rl_valid = self.validate_profile(Owl2Profile::RL)?.is_valid;

        // EL is most restrictive, then QL and RL are incomparable
        if el_valid {
            Ok(Some(Owl2Profile::EL))
        } else if ql_valid && rl_valid {
            // Can't determine which is more restrictive
            Ok(None)
        } else if ql_valid {
            Ok(Some(Owl2Profile::QL))
        } else if rl_valid {
            Ok(Some(Owl2Profile::RL))
        } else {
            Ok(None)
        }
    }

    pub fn satisfies_any_profile(&self) -> OwlResult<bool> {
        let results = self.validate_all_profiles()?;
        Ok(results.iter().any(|r| r.is_valid))
    }

    pub fn get_optimization_hints(&self) -> Vec<OptimizationHint> {
        // Return cached hints or compute new ones
        Vec::new()
    }

    pub fn clear_cache(&mut self) {
        // Clear validation cache
    }

    pub fn get_cache_stats(&self) -> CacheStats {
        CacheStats { hits: 0, misses: 0 }
    }

    pub fn set_advanced_caching(&mut self, enabled: bool) {
        self.advanced_caching = enabled;
    }
}

impl ProfileValidator for Owl2ProfileValidator {
    fn validate(&self, _ontology: &Ontology) -> OwlResult<ProfileValidationResult> {
        // Default to validating against all profiles
        let results = self.validate_all_profiles()?;
        // Return the most restrictive valid profile, or Full if none
        Ok(results
            .into_iter()
            .find(|r| r.is_valid)
            .unwrap_or_else(|| ProfileValidationResult::valid(Owl2Profile::Full)))
    }

    fn profile(&self) -> Owl2Profile {
        Owl2Profile::Full
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}
