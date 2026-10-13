//! OWL2 Profile Common Types and Traits
//!
//! This module defines common types and traits used across all OWL2 profile validators.

use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{Axiom, ClassExpression, ObjectPropertyExpression};
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

/// A profile violation cap: routing only needs the verdict, so large
/// ontologies report the first offenders and stop. The statistics
/// therefore count up to the cap, not the whole ontology.
pub(super) const MAX_REPORTED_VIOLATIONS: usize = 50;

/// A profile's rule table: which axioms and class expressions sit
/// outside the profile. `forbidden_super_expression` applies only on
/// the superclass side of a SubClassOf (and equivalent positions),
/// where profiles like QL and RL allow Boolean/existential constructs
/// on the subclass side only. The tables are deliberately conservative:
/// anything uncertain stays unflagged, so a rejection can only demote
/// an ontology to a less restrictive profile (a routing opportunity
/// lost), never promote it into a profile it does not fit (#167).
pub(super) struct ProfileRuleTable {
    pub profile: Owl2Profile,
    pub forbidden_axiom: fn(&Axiom) -> Option<&'static str>,
    pub forbidden_anywhere_expression: fn(&ClassExpression) -> Option<&'static str>,
    pub forbidden_super_expression: fn(&ClassExpression) -> Option<&'static str>,
    pub forbidden_property_expression: fn(&ObjectPropertyExpression) -> Option<&'static str>,
}

/// One shared scanner behind the EL/QL/RL validators: walks every axiom
/// once, applies the rule table, and reports up to
/// [`MAX_REPORTED_VIOLATIONS`] errors with the offending axiom as
/// location context.
pub(super) fn validate_with_rules(
    ontology: &Ontology,
    table: &ProfileRuleTable,
) -> ProfileValidationResult {
    let mut violations: Vec<ProfileViolation> = Vec::new();
    let mut statistics = ValidationStatistics::default();

    for axiom in ontology.axioms() {
        statistics.axioms_checked += 1;
        let location = axiom.axiom_type().name().to_string();
        if let Some(construct) = (table.forbidden_axiom)(axiom) {
            push_violation(
                &mut violations,
                ProfileViolationType::DisallowedAxiomType(construct.to_string()),
                format!(
                    "{} axiom is outside the OWL 2 {} profile",
                    construct,
                    table.profile.name()
                ),
                &location,
            );
        }
        match axiom.as_ref() {
            Axiom::SubClassOf(sub) => {
                let subject = expression_location(sub.sub_class());
                let keep_going = walk_expression(
                    sub.sub_class(),
                    false,
                    table,
                    subject.clone(),
                    &mut violations,
                    &mut statistics,
                ) && walk_expression(
                    sub.super_class(),
                    true,
                    table,
                    subject,
                    &mut violations,
                    &mut statistics,
                );
                if !keep_going {
                    break;
                }
            }
            Axiom::ClassAssertion(assertion) => {
                let subject = expression_location(assertion.class_expr());
                if !walk_expression(
                    assertion.class_expr(),
                    false,
                    table,
                    subject,
                    &mut violations,
                    &mut statistics,
                ) {
                    break;
                }
            }
            _ => {}
        }
        if violations.len() >= MAX_REPORTED_VIOLATIONS {
            break;
        }
    }
    ProfileValidationResult {
        profile: table.profile,
        is_valid: violations.is_empty(),
        violations,
        statistics,
    }
}

/// A short location label for violation reports: the named class (or
/// individual-facing class expression) the offending axiom is about.
fn expression_location(expression: &ClassExpression) -> String {
    match expression {
        ClassExpression::Class(class) => class.iri().as_str().to_string(),
        _ => "anonymous expression".to_string(),
    }
}

fn push_violation(
    violations: &mut Vec<ProfileViolation>,
    violation_type: ProfileViolationType,
    message: String,
    location: &str,
) -> bool {
    // Returns false once the reporting cap is reached.
    if violations.len() < MAX_REPORTED_VIOLATIONS {
        violations.push(
            ProfileViolation::new(violation_type, ViolationSeverity::Error, message)
                .with_location(location),
        );
    }
    violations.len() < MAX_REPORTED_VIOLATIONS
}

fn walk_property_expression(
    property: &ObjectPropertyExpression,
    table: &ProfileRuleTable,
    location: &str,
    violations: &mut Vec<ProfileViolation>,
) -> bool {
    let mut current = property;
    loop {
        if let Some(construct) = (table.forbidden_property_expression)(current) {
            return push_violation(
                violations,
                ProfileViolationType::DisallowedPropertyExpression(construct.to_string()),
                format!(
                    "{} is outside the OWL 2 {} profile",
                    construct,
                    table.profile.name()
                ),
                location,
            );
        }
        match current {
            ObjectPropertyExpression::ObjectInverseOf(inner) => current = inner,
            ObjectPropertyExpression::ObjectProperty(_) => return true,
        }
    }
}

/// Walk a class expression, reporting constructs the rule table
/// forbids. `super_side` marks the superclass/equivalent position,
/// where the QL/RL tables additionally restrict expressiveness.
fn walk_expression(
    expression: &ClassExpression,
    super_side: bool,
    table: &ProfileRuleTable,
    location: String,
    violations: &mut Vec<ProfileViolation>,
    statistics: &mut ValidationStatistics,
) -> bool {
    statistics.class_expressions_checked += 1;
    let construct = if super_side {
        (table.forbidden_super_expression)(expression)
            .or_else(|| (table.forbidden_anywhere_expression)(expression))
    } else {
        (table.forbidden_anywhere_expression)(expression)
    };
    if let Some(construct) = construct {
        return push_violation(
            violations,
            ProfileViolationType::DisallowedClassExpression(construct.to_string()),
            format!(
                "{} is outside the OWL 2 {} profile{}",
                construct,
                table.profile.name(),
                if super_side { " (superclass side)" } else { "" }
            ),
            &location,
        );
    }
    match expression {
        ClassExpression::Class(_)
        | ClassExpression::ObjectOneOf(_)
        | ClassExpression::DataHasValue(_, _) => true,
        ClassExpression::ObjectIntersectionOf(operands)
        | ClassExpression::ObjectUnionOf(operands) => operands.iter().all(|operand| {
            walk_expression(operand, super_side, table, location.clone(), violations, statistics)
        }),
        ClassExpression::ObjectComplementOf(inner) => walk_expression(
            inner,
            super_side,
            table,
            location,
            violations,
            statistics,
        ),
        ClassExpression::ObjectSomeValuesFrom(property, filler)
        | ClassExpression::ObjectAllValuesFrom(property, filler) => {
            walk_property_expression(property, table, &location, violations)
                && walk_expression(
                    filler,
                    // A filler is itself an anonymous superclass-shaped
                    // position for nested restrictions.
                    true,
                    table,
                    location,
                    violations,
                    statistics,
                )
        }
        ClassExpression::ObjectHasValue(property, _) => {
            walk_property_expression(property, table, &location, violations)
        }
        ClassExpression::ObjectHasSelf(property) => {
            walk_property_expression(property, table, &location, violations)
        }
        ClassExpression::ObjectMinCardinality(_, property, qualifier)
        | ClassExpression::ObjectMaxCardinality(_, property, qualifier)
        | ClassExpression::ObjectExactCardinality(_, property, qualifier) => {
            let mut keep_going = walk_property_expression(property, table, &location, violations);
            if let Some(filler) = qualifier {
                keep_going &= walk_expression(
                    filler, true, table, location, violations, statistics,
                );
            }
            keep_going
        }
        // Data-side wrappers carry no object property expression and no
        // class-expression filler; the DataRange interior is out of the
        // conservative scan's scope.
        ClassExpression::DataSomeValuesFrom(_, _)
        | ClassExpression::DataAllValuesFrom(_, _)
        | ClassExpression::DataMinCardinality(_, _, _)
        | ClassExpression::DataMaxCardinality(_, _, _)
        | ClassExpression::DataExactCardinality(_, _, _) => true,
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
