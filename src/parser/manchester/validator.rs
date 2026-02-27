//! Manchester Syntax Semantic Validation
//!
//! This module provides semantic validation for Manchester Syntax AST nodes,
//! ensuring that parsed constructs are semantically valid according to
//! OWL2 specifications and Manchester Syntax rules.

use super::syntax::{
    Annotation, AnnotationValue, ClassExpression, DataPropertyExpression, DataRange,
    IndividualExpression, ManchesterAST, ObjectPropertyExpression, PropertyAssertion,
    PropertyCharacteristic,
};
use crate::core::error::{OwlError, OwlResult};
use smallvec;
use smallvec::SmallVec;
use std::collections::HashMap;

/// Semantic validator for Manchester Syntax AST
#[derive(Debug, Clone)]
pub struct SyntaxValidator {
    /// Known prefixes for IRI resolution
    prefixes: HashMap<String, String>,

    /// Whether to perform strict validation
    strict_mode: bool,

    /// Validation context stack
    context_stack: Vec<ValidationContext>,
}

/// Validation context for error reporting
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Current entity being validated
    pub entity: Option<String>,

    /// Current validation phase
    pub phase: ValidationPhase,

    /// Parent expressions for nested validation
    pub parent_expressions: Vec<String>,
}

/// Validation phases
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationPhase {
    /// Prefix declaration validation
    PrefixDeclaration,

    /// Class declaration validation
    ClassDeclaration,

    /// Property declaration validation
    PropertyDeclaration,

    /// Individual declaration validation
    IndividualDeclaration,

    /// Expression validation
    ExpressionValidation,

    /// Axiom validation
    AxiomValidation,
}

/// Validation result with detailed information
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,

    /// Validation warnings (non-critical issues)
    pub warnings: Vec<ValidationWarning>,

    /// Validation errors (critical issues)
    pub errors: Vec<ValidationError>,
}

/// Validation warning
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Warning message
    pub message: String,

    /// Warning location (line, column if available)
    pub location: Option<(usize, usize)>,

    /// Warning code for categorization
    pub code: WarningCode,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message
    pub message: String,

    /// Error location (line, column if available)
    pub location: Option<(usize, usize)>,

    /// Error code for categorization
    pub code: ErrorCode,
}

/// Warning codes
#[derive(Debug, Clone, PartialEq)]
pub enum WarningCode {
    /// Unused prefix declaration
    UnusedPrefix,

    /// Redundant expression (e.g., A and A)
    RedundantExpression,

    /// Complex expression that could be simplified
    ComplexExpression,

    /// Deprecated construct usage
    DeprecatedConstruct,

    /// Invalid IRI format
    InvalidIRI,

    /// Invalid cardinality value
    InvalidCardinality,

    /// Unimplemented feature
    UnimplementedFeature,
}

/// Error codes
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    /// Undefined prefix
    UndefinedPrefix,

    /// Circular dependency
    CircularDependency,

    /// Invalid property characteristic
    InvalidPropertyCharacteristic,

    /// Type mismatch in expression
    TypeMismatch,

    /// Invalid cardinality restriction
    InvalidCardinality,

    /// Missing required component
    MissingRequiredComponent,

    /// Invalid IRI reference
    InvalidIRI,

    /// Duplicate declaration
    DuplicateDeclaration,
}

impl SyntaxValidator {
    /// Create a new syntax validator
    pub fn new() -> Self {
        Self {
            prefixes: HashMap::new(),
            strict_mode: true,
            context_stack: Vec::new(),
        }
    }

    /// Create a validator with strict mode setting
    pub fn with_strict_mode(strict: bool) -> Self {
        Self {
            prefixes: HashMap::new(),
            strict_mode: strict,
            context_stack: Vec::new(),
        }
    }

    /// Add a prefix mapping
    pub fn add_prefix(&mut self, prefix: String, iri: String) {
        self.prefixes.insert(prefix, iri);
    }

    /// Set strict mode
    pub fn set_strict_mode(&mut self, strict: bool) {
        self.strict_mode = strict;
    }

    /// Validate a complete AST
    pub fn validate_ast(&mut self, ast: &ManchesterAST) -> ValidationResult {
        let mut result = ValidationResult {
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        match ast {
            ManchesterAST::PrefixDeclaration { prefix, iri } => {
                self.validate_prefix_declaration(prefix, iri, &mut result);
            }
            ManchesterAST::ClassDeclaration {
                name,
                sub_class_of,
                equivalent_to,
                disjoint_with,
                ..
            } => {
                self.validate_class_declaration(
                    name,
                    sub_class_of,
                    equivalent_to,
                    disjoint_with,
                    &mut result,
                );
            }
            ManchesterAST::ObjectPropertyDeclaration {
                name,
                domain,
                range,
                characteristics,
                ..
            } => {
                self.validate_object_property_declaration(
                    name,
                    domain,
                    range,
                    characteristics,
                    &mut result,
                );
            }
            ManchesterAST::DataPropertyDeclaration {
                name,
                domain,
                range,
                characteristics,
                ..
            } => {
                self.validate_data_property_declaration(
                    name,
                    domain,
                    range,
                    characteristics,
                    &mut result,
                );
            }
            ManchesterAST::IndividualDeclaration {
                name, types, facts, ..
            } => {
                self.validate_individual_declaration(name, types, facts, &mut result);
            }
            ManchesterAST::DisjointClasses {
                classes,
                annotations: _,
            } => {
                self.validate_disjoint_classes(classes, &mut result);
            }
            ManchesterAST::EquivalentClasses {
                classes,
                annotations: _,
            } => {
                self.validate_equivalent_classes(classes, &mut result);
            }
            ManchesterAST::DifferentIndividuals {
                individuals,
                annotations: _,
            } => {
                self.validate_different_individuals(individuals, &mut result);
            }
            ManchesterAST::SameIndividual {
                individuals,
                annotations: _,
            } => {
                self.validate_same_individuals(individuals, &mut result);
            }
            ManchesterAST::AnnotationDeclaration { name, annotations } => {
                self.validate_annotation_declaration(name, annotations, &mut result);
            }
            ManchesterAST::RuleDeclaration {
                name,
                body,
                head,
                annotations,
            } => {
                self.validate_rule_declaration(name, body, head, annotations, &mut result);
            }
        }

        result.is_valid = result.errors.is_empty();
        result
    }

    /// Validate prefix declaration
    fn validate_prefix_declaration(&self, prefix: &str, iri: &str, result: &mut ValidationResult) {
        // Check prefix format
        if prefix.is_empty() {
            result.errors.push(ValidationError {
                message: "Prefix cannot be empty".to_string(),
                location: None,
                code: ErrorCode::InvalidIRI,
            });
            return;
        }

        // Check for invalid characters in prefix
        if prefix
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
        {
            result.errors.push(ValidationError {
                message: format!("Invalid characters in prefix: {}", prefix),
                location: None,
                code: ErrorCode::InvalidIRI,
            });
        }

        // Check IRI format (basic validation)
        if iri.is_empty() {
            result.errors.push(ValidationError {
                message: "IRI cannot be empty".to_string(),
                location: None,
                code: ErrorCode::InvalidIRI,
            });
            return;
        }

        if !iri.starts_with("http://") && !iri.starts_with("https://") && !iri.starts_with("urn:") {
            result.warnings.push(ValidationWarning {
                message: format!("IRI doesn't start with common scheme: {}", iri),
                location: None,
                code: WarningCode::InvalidIRI,
            });
        }

        // Check if IRI ends with separator
        if !iri.ends_with('#') && !iri.ends_with('/') && !iri.ends_with(':') {
            result.warnings.push(ValidationWarning {
                message: format!("IRI should end with separator (#, /, or :): {}", iri),
                location: None,
                code: WarningCode::InvalidIRI,
            });
        }
    }

    /// Validate class declaration
    fn validate_class_declaration(
        &self,
        name: &str,
        sub_class_of: &SmallVec<[Box<ClassExpression>; 4]>,
        equivalent_to: &SmallVec<[Box<ClassExpression>; 4]>,
        disjoint_with: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        // Validate class name
        self.validate_entity_name(name, "Class", result);

        // Validate subclass expressions
        for expr in sub_class_of {
            self.validate_class_expression(expr, result);
        }

        // Validate equivalent expressions
        for expr in equivalent_to {
            self.validate_class_expression(expr, result);
        }

        // Validate disjoint expressions
        for expr in disjoint_with {
            self.validate_class_expression(expr, result);
        }

        // Check for circular dependencies in strict mode
        if self.strict_mode {
            self.check_circular_dependencies(name, sub_class_of, result);
        }

        // Check for redundant expressions
        if self.strict_mode {
            self.check_redundant_expressions(sub_class_of, result);
            self.check_redundant_expressions(equivalent_to, result);
        }
    }

    /// Validate object property declaration
    fn validate_object_property_declaration(
        &self,
        name: &str,
        domain: &SmallVec<[Box<ClassExpression>; 4]>,
        range: &SmallVec<[Box<ClassExpression>; 4]>,
        characteristics: &SmallVec<[PropertyCharacteristic; 4]>,
        result: &mut ValidationResult,
    ) {
        // Validate property name
        self.validate_entity_name(name, "ObjectProperty", result);

        // Validate domain
        for domain_expr in domain {
            self.validate_class_expression(domain_expr, result);
        }

        // Validate range
        for range_expr in range {
            self.validate_class_expression(range_expr, result);
        }

        // Validate characteristics
        for characteristic in characteristics {
            self.validate_property_characteristic(characteristic, result);
        }

        // Check for incompatible characteristics
        if self.strict_mode {
            self.check_incompatible_characteristics(characteristics, result);
        }
    }

    /// Validate data property declaration
    fn validate_data_property_declaration(
        &self,
        name: &str,
        domain: &SmallVec<[Box<ClassExpression>; 4]>,
        range: &SmallVec<[DataRange; 4]>,
        characteristics: &SmallVec<[PropertyCharacteristic; 4]>,
        result: &mut ValidationResult,
    ) {
        // Validate property name
        self.validate_entity_name(name, "DataProperty", result);

        // Validate domain
        for domain_expr in domain {
            self.validate_class_expression(domain_expr, result);
        }

        // Validate range
        for range_expr in range {
            self.validate_data_range(range_expr, result);
        }

        // Validate characteristics
        for characteristic in characteristics {
            self.validate_property_characteristic(characteristic, result);
        }

        // Check that data properties only have valid characteristics
        if self.strict_mode {
            self.check_data_property_characteristics(characteristics, result);
        }
    }

    /// Validate individual declaration
    fn validate_individual_declaration(
        &self,
        name: &str,
        types: &SmallVec<[Box<ClassExpression>; 4]>,
        facts: &SmallVec<[PropertyAssertion; 8]>,
        result: &mut ValidationResult,
    ) {
        // Validate individual name
        self.validate_entity_name(name, "Individual", result);

        // Validate type expressions
        for expr in types {
            self.validate_class_expression(expr, result);
        }

        // Validate property assertions
        for fact in facts {
            self.validate_property_assertion(fact, result);
        }
    }

    /// Validate disjoint classes axiom
    fn validate_disjoint_classes(
        &self,
        classes: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        if classes.len() < 2 {
            result.errors.push(ValidationError {
                message: "DisjointClasses requires at least 2 classes".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        for expr in classes {
            self.validate_class_expression(expr, result);
        }
    }

    /// Validate equivalent classes axiom
    fn validate_equivalent_classes(
        &self,
        classes: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        if classes.len() < 2 {
            result.errors.push(ValidationError {
                message: "EquivalentClasses requires at least 2 classes".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        for expr in classes {
            self.validate_class_expression(expr, result);
        }
    }

    /// Validate different individuals axiom
    fn validate_different_individuals(
        &self,
        individuals: &SmallVec<[IndividualExpression; 6]>,
        result: &mut ValidationResult,
    ) {
        if individuals.len() < 2 {
            result.errors.push(ValidationError {
                message: "DifferentIndividuals requires at least 2 individuals".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        // Check for duplicate individuals
        let mut seen = std::collections::HashSet::new();
        for individual in individuals {
            if seen.contains(individual) {
                result.errors.push(ValidationError {
                    message: format!(
                        "Duplicate individual in DifferentIndividuals: {:?}",
                        individual
                    ),
                    location: None,
                    code: ErrorCode::DuplicateDeclaration,
                });
            }
            seen.insert(individual.clone());
        }
    }

    /// Validate same individual axiom
    fn validate_same_individuals(
        &self,
        individuals: &SmallVec<[IndividualExpression; 6]>,
        result: &mut ValidationResult,
    ) {
        if individuals.len() < 2 {
            result.errors.push(ValidationError {
                message: "SameIndividual requires at least 2 individuals".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        // Check for duplicate individuals
        let mut seen = std::collections::HashSet::new();
        for individual in individuals {
            if seen.contains(individual) {
                result.errors.push(ValidationError {
                    message: format!("Duplicate individual in SameIndividual: {:?}", individual),
                    location: None,
                    code: ErrorCode::DuplicateDeclaration,
                });
            }
            seen.insert(individual.clone());
        }
    }

    /// Validate annotation declaration
    fn validate_annotation_declaration(
        &self,
        name: &str,
        annotations: &SmallVec<[Annotation; 2]>,
        result: &mut ValidationResult,
    ) {
        // Validate annotation property name
        if name.is_empty() {
            result.errors.push(ValidationError {
                message: "Annotation property name cannot be empty".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        // Validate that annotation property is a valid IRI
        self.validate_iri_reference(name, result);

        // Check if it's a built-in annotation property
        if self.is_builtin_annotation_property(name) {
            // Built-in annotation properties have specific constraints
            self.validate_builtin_annotation_property(name, annotations, result);
        }

        // Validate each annotation
        for annotation in annotations {
            self.validate_annotation(annotation, result);
        }

        // Check for duplicate annotations
        if self.strict_mode {
            self.check_duplicate_annotations(annotations, result);
        }
    }

    /// Validate rule declaration (SWRL rule)
    fn validate_rule_declaration(
        &self,
        name: &Option<String>,
        body: &SmallVec<[Box<ClassExpression>; 4]>,
        head: &SmallVec<[Box<ClassExpression>; 4]>,
        annotations: &SmallVec<[Annotation; 2]>,
        result: &mut ValidationResult,
    ) {
        // Validate rule name if present
        if let Some(rule_name) = name {
            if rule_name.is_empty() {
                result.errors.push(ValidationError {
                    message: "Rule name cannot be empty".to_string(),
                    location: None,
                    code: ErrorCode::InvalidIRI,
                });
            }
        }

        // Rule must have at least one atom in body or head
        if body.is_empty() && head.is_empty() {
            result.errors.push(ValidationError {
                message: "Rule must have at least one atom in body or head".to_string(),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        // Validate body atoms (antecedent)
        if body.is_empty() {
            result.warnings.push(ValidationWarning {
                message: "Rule has empty body - this creates a fact".to_string(),
                location: None,
                code: WarningCode::ComplexExpression,
            });
        } else {
            for atom in body {
                self.validate_rule_atom(atom, true, result); // true = body atom
            }
        }

        // Validate head atoms (consequent)
        if head.is_empty() {
            result.warnings.push(ValidationWarning {
                message: "Rule has empty head - rule may never trigger".to_string(),
                location: None,
                code: WarningCode::ComplexExpression,
            });
        } else {
            for atom in head {
                self.validate_rule_atom(atom, false, result); // false = head atom
            }
        }

        // Validate annotations on the rule
        for annotation in annotations {
            self.validate_annotation(annotation, result);
        }

        // Check for unsafe variable usage in strict mode
        if self.strict_mode {
            self.check_rule_variable_safety(body, head, result);
        }

        // Check for rule consistency (no contradictions in head)
        if self.strict_mode {
            self.check_rule_head_consistency(head, result);
        }
    }

    /// Validate a single annotation
    fn validate_annotation(&self, annotation: &Annotation, result: &mut ValidationResult) {
        // Validate annotation property
        self.validate_iri_reference(&annotation.property, result);

        // Validate annotation value
        match &annotation.value {
            AnnotationValue::IRI(iri) => {
                self.validate_iri_reference(iri, result);
            }
            AnnotationValue::Literal(literal) => {
                if literal.is_empty() {
                    result.warnings.push(ValidationWarning {
                        message: "Empty literal value in annotation".to_string(),
                        location: None,
                        code: WarningCode::InvalidIRI,
                    });
                }

                // Check for well-formed literal syntax
                if literal.starts_with('"') && literal.ends_with('"') {
                    // Quoted literal - check for language tags or datatype
                    let content = &literal[1..literal.len() - 1];
                    if content.is_empty() {
                        result.warnings.push(ValidationWarning {
                            message: "Empty quoted literal".to_string(),
                            location: None,
                            code: WarningCode::InvalidIRI,
                        });
                    }
                }
            }
            AnnotationValue::AnonymousIndividual => {
                // Anonymous individuals are always valid in annotations
                // but we might want to warn about their usage in certain contexts
                if self.strict_mode {
                    result.warnings.push(ValidationWarning {
                        message: "Anonymous individual used in annotation value".to_string(),
                        location: None,
                        code: WarningCode::ComplexExpression,
                    });
                }
            }
        }
    }

    /// Validate a rule atom (class expression used in SWRL rule)
    fn validate_rule_atom(
        &self,
        atom: &ClassExpression,
        is_body: bool,
        result: &mut ValidationResult,
    ) {
        match atom {
            ClassExpression::NamedClass(class_name) => {
                // Validate class reference
                self.validate_iri_reference(class_name, result);

                // Check for special OWL classes that might not be appropriate in rules
                if class_name == "owl:Thing" || class_name == "owl:Nothing" {
                    result.warnings.push(ValidationWarning {
                        message: format!(
                            "Use of {} in rule atom may not be meaningful",
                            class_name
                        ),
                        location: None,
                        code: WarningCode::ComplexExpression,
                    });
                }
            }
            ClassExpression::ObjectSomeValuesFrom(prop, expr) => {
                self.validate_object_property_expression(prop, result);
                self.validate_rule_atom(expr, is_body, result);
            }
            ClassExpression::ObjectAllValuesFrom(prop, expr) => {
                self.validate_object_property_expression(prop, result);
                self.validate_rule_atom(expr, is_body, result);
            }
            ClassExpression::ObjectHasValue(prop, individual) => {
                self.validate_object_property_expression(prop, result);
                self.validate_iri_reference(individual, result);
            }
            ClassExpression::DataHasValue(prop, literal) => {
                self.validate_data_property_expression(prop, result);
                if literal.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataHasValue literal in rule cannot be empty".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                }
            }
            ClassExpression::ObjectIntersection(operands) => {
                if operands.is_empty() {
                    result.errors.push(ValidationError {
                        message: "ObjectIntersection in rule requires at least 1 operand"
                            .to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for operand in operands {
                    self.validate_rule_atom(operand, is_body, result);
                }
            }
            ClassExpression::ObjectUnion(operands) => {
                if operands.is_empty() {
                    result.errors.push(ValidationError {
                        message: "ObjectUnion in rule requires at least 1 operand".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for operand in operands {
                    self.validate_rule_atom(operand, is_body, result);
                }
            }
            ClassExpression::ObjectComplement(expr) => {
                // Negation is only allowed in rule body in SWRL
                if !is_body {
                    result.errors.push(ValidationError {
                        message:
                            "Negation (ObjectComplement) is only allowed in rule body, not head"
                                .to_string(),
                        location: None,
                        code: ErrorCode::TypeMismatch,
                    });
                }
                self.validate_rule_atom(expr, is_body, result);
            }
            // Restrict cardinality expressions in rules as they may not be supported
            ClassExpression::ObjectMinCardinality(_, _)
            | ClassExpression::ObjectMaxCardinality(_, _)
            | ClassExpression::ObjectExactCardinality(_, _)
            | ClassExpression::DataMinCardinality(_, _)
            | ClassExpression::DataMaxCardinality(_, _)
            | ClassExpression::DataExactCardinality(_, _) => {
                result.warnings.push(ValidationWarning {
                    message: "Cardinality restrictions in SWRL rules may not be supported by all reasoners".to_string(),
                    location: None,
                    code: WarningCode::UnimplementedFeature,
                });
                // Still validate the components
                match atom {
                    ClassExpression::ObjectMinCardinality(prop, card)
                    | ClassExpression::ObjectMaxCardinality(prop, card)
                    | ClassExpression::ObjectExactCardinality(prop, card) => {
                        self.validate_object_property_expression(prop, result);
                        self.validate_cardinality(*card, result);
                    }
                    ClassExpression::DataMinCardinality(prop, card)
                    | ClassExpression::DataMaxCardinality(prop, card)
                    | ClassExpression::DataExactCardinality(prop, card) => {
                        self.validate_data_property_expression(prop, result);
                        self.validate_cardinality(*card, result);
                    }
                    _ => {}
                }
            }
            // Data restrictions
            ClassExpression::DataSomeValuesFrom(prop, range) => {
                self.validate_data_property_expression(prop, result);
                self.validate_data_range(range, result);
            }
            ClassExpression::DataAllValuesFrom(prop, range) => {
                self.validate_data_property_expression(prop, result);
                self.validate_data_range(range, result);
            }
            // Handle other cases
            ClassExpression::ObjectOneOf(_) => {
                result.warnings.push(ValidationWarning {
                    message: "ObjectOneOf in SWRL rules may not be supported by all reasoners"
                        .to_string(),
                    location: None,
                    code: WarningCode::UnimplementedFeature,
                });
            }
            ClassExpression::ObjectHasSelf(_) => {
                // ObjectHasSelf is generally supported in rules
                if let ClassExpression::ObjectHasSelf(prop) = atom {
                    self.validate_object_property_expression(prop, result);
                }
            }
        }
    }

    /// Check if an annotation property is a built-in OWL annotation property
    fn is_builtin_annotation_property(&self, property: &str) -> bool {
        matches!(
            property,
            "rdfs:label"
                | "rdfs:comment"
                | "rdfs:seeAlso"
                | "rdfs:isDefinedBy"
                | "owl:versionInfo"
                | "owl:priorVersion"
                | "owl:backwardCompatibleWith"
                | "owl:incompatibleWith"
                | "owl:deprecated"
                | "dc:creator"
                | "dc:date"
                | "dc:description"
                | "dc:title"
                | "dc:source"
                | "dc:subject"
                | "dcterms:creator"
                | "dcterms:date"
                | "dcterms:description"
                | "dcterms:title"
                | "dcterms:source"
                | "dcterms:subject"
        )
    }

    /// Validate built-in annotation property constraints
    fn validate_builtin_annotation_property(
        &self,
        property: &str,
        annotations: &[Annotation],
        result: &mut ValidationResult,
    ) {
        match property {
            "owl:deprecated" => {
                // owl:deprecated should have boolean literal value "true" or "false"
                if let Some(annotation) = annotations.first() {
                    match &annotation.value {
                        AnnotationValue::Literal(literal) => {
                            let normalized = literal.to_lowercase().trim_matches('"').to_string();
                            if normalized != "true" && normalized != "false" {
                                result.warnings.push(ValidationWarning {
                                    message: format!(
                                        "owl:deprecated should have boolean value, got: {}",
                                        literal
                                    ),
                                    location: None,
                                    code: WarningCode::InvalidIRI,
                                });
                            }
                        }
                        _ => {
                            result.warnings.push(ValidationWarning {
                                message: "owl:deprecated should have a literal value".to_string(),
                                location: None,
                                code: WarningCode::InvalidIRI,
                            });
                        }
                    }
                }
            }
            "rdfs:label" => {
                // rdfs:label should have a literal value
                if let Some(annotation) = annotations.first() {
                    if !matches!(&annotation.value, AnnotationValue::Literal(_)) {
                        result.warnings.push(ValidationWarning {
                            message: "rdfs:label should have a literal value".to_string(),
                            location: None,
                            code: WarningCode::InvalidIRI,
                        });
                    }
                }
            }
            "rdfs:comment" => {
                // rdfs:comment should have a literal value
                if let Some(annotation) = annotations.first() {
                    if !matches!(&annotation.value, AnnotationValue::Literal(_)) {
                        result.warnings.push(ValidationWarning {
                            message: "rdfs:comment should have a literal value".to_string(),
                            location: None,
                            code: WarningCode::InvalidIRI,
                        });
                    }
                }
            }
            _ => {
                // Other built-in annotation properties are generally flexible
            }
        }
    }

    /// Check for duplicate annotations
    fn check_duplicate_annotations(
        &self,
        annotations: &[Annotation],
        result: &mut ValidationResult,
    ) {
        let mut seen = std::collections::HashSet::new();
        for annotation in annotations {
            let key = (&annotation.property, annotation.value.clone());
            if seen.contains(&key) {
                result.warnings.push(ValidationWarning {
                    message: format!(
                        "Duplicate annotation detected: {} -> {:?}",
                        annotation.property, annotation.value
                    ),
                    location: None,
                    code: WarningCode::RedundantExpression,
                });
            }
            seen.insert(key);
        }
    }

    /// Check for unsafe variable usage in rules
    fn check_rule_variable_safety(
        &self,
        _body: &SmallVec<[Box<ClassExpression>; 4]>,
        head: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        // This is a simplified check - a full implementation would track variable usage
        // across the rule and ensure variables in the head are bound in the body

        // For now, just warn about potentially unsafe patterns
        for atom in head {
            match &**atom {
                ClassExpression::ObjectHasValue(_, _individual) => {
                    // These atoms introduce new constants that might not be safe
                    result.warnings.push(ValidationWarning {
                        message: "Rule head introduces new constants - ensure variables are properly bound".to_string(),
                        location: None,
                        code: WarningCode::ComplexExpression,
                    });
                }
                ClassExpression::DataHasValue(_, _) => {
                    // These atoms introduce new constants that might not be safe
                    result.warnings.push(ValidationWarning {
                        message: "Rule head introduces new constants - ensure variables are properly bound".to_string(),
                        location: None,
                        code: WarningCode::ComplexExpression,
                    });
                }
                _ => {}
            }
        }
    }

    /// Check for contradictions in rule head
    fn check_rule_head_consistency(
        &self,
        head: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        // Simple check for obvious contradictions
        let mut classes = Vec::new();
        let mut negated_classes = Vec::new();

        for atom in head {
            match &**atom {
                ClassExpression::NamedClass(class_name) => {
                    classes.push(class_name.clone());
                }
                ClassExpression::ObjectComplement(expr) => {
                    if let ClassExpression::NamedClass(class_name) = &**expr {
                        negated_classes.push(class_name.clone());
                    }
                }
                _ => {}
            }
        }

        // Check for class and its negation in the head
        for class in &classes {
            if negated_classes.contains(class) {
                result.errors.push(ValidationError {
                    message: format!(
                        "Rule head contains contradiction: {} and not {}",
                        class, class
                    ),
                    location: None,
                    code: ErrorCode::TypeMismatch,
                });
            }
        }

        // Check for owl:Nothing in head (rule would never be satisfiable)
        if classes.contains(&"owl:Nothing".to_string()) {
            result.errors.push(ValidationError {
                message: "Rule head contains owl:Nothing - rule would never be satisfiable"
                    .to_string(),
                location: None,
                code: ErrorCode::TypeMismatch,
            });
        }
    }

    /// Validate class expression
    fn validate_class_expression(&self, expr: &ClassExpression, result: &mut ValidationResult) {
        match expr {
            ClassExpression::NamedClass(class_name) => {
                self.validate_iri_reference(class_name, result);
            }
            ClassExpression::ObjectIntersection(operands) => {
                if operands.is_empty() {
                    result.errors.push(ValidationError {
                        message: "ObjectIntersection requires at least 1 operand".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for operand in operands {
                    self.validate_class_expression(operand, result);
                }
            }
            ClassExpression::ObjectUnion(operands) => {
                if operands.is_empty() {
                    result.errors.push(ValidationError {
                        message: "ObjectUnion requires at least 1 operand".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for operand in operands {
                    self.validate_class_expression(operand, result);
                }
            }
            ClassExpression::ObjectComplement(expr) => {
                self.validate_class_expression(expr, result);
            }
            ClassExpression::ObjectOneOf(individuals) => {
                if individuals.is_empty() {
                    result.errors.push(ValidationError {
                        message: "ObjectOneOf requires at least 1 individual".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for individual in individuals {
                    self.validate_iri_reference(individual, result);
                }
            }
            ClassExpression::ObjectSomeValuesFrom(prop, expr) => {
                self.validate_object_property_expression(prop, result);
                self.validate_class_expression(expr, result);
            }
            ClassExpression::ObjectAllValuesFrom(prop, expr) => {
                self.validate_object_property_expression(prop, result);
                self.validate_class_expression(expr, result);
            }
            ClassExpression::ObjectHasValue(prop, individual) => {
                self.validate_object_property_expression(prop, result);
                self.validate_iri_reference(individual, result);
            }
            ClassExpression::ObjectHasSelf(prop) => {
                self.validate_object_property_expression(prop, result);
            }
            ClassExpression::ObjectMinCardinality(prop, cardinality) => {
                self.validate_object_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
            ClassExpression::ObjectMaxCardinality(prop, cardinality) => {
                self.validate_object_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
            ClassExpression::ObjectExactCardinality(prop, cardinality) => {
                self.validate_object_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
            // Data property restrictions
            ClassExpression::DataSomeValuesFrom(prop, range) => {
                self.validate_data_property_expression(prop, result);
                self.validate_data_range(range, result);
            }
            ClassExpression::DataAllValuesFrom(prop, range) => {
                self.validate_data_property_expression(prop, result);
                self.validate_data_range(range, result);
            }
            ClassExpression::DataHasValue(prop, literal) => {
                self.validate_data_property_expression(prop, result);
                // Validate literal format
                if literal.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataHasValue literal cannot be empty".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                }
            }
            ClassExpression::DataMinCardinality(prop, cardinality) => {
                self.validate_data_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
            ClassExpression::DataMaxCardinality(prop, cardinality) => {
                self.validate_data_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
            ClassExpression::DataExactCardinality(prop, cardinality) => {
                self.validate_data_property_expression(prop, result);
                self.validate_cardinality(*cardinality, result);
            }
        }
    }

    /// Validate object property expression
    fn validate_object_property_expression(
        &self,
        expr: &ObjectPropertyExpression,
        result: &mut ValidationResult,
    ) {
        match expr {
            ObjectPropertyExpression::NamedProperty(name) => {
                self.validate_iri_reference(name, result);
            }
            ObjectPropertyExpression::InverseProperty(prop) => {
                self.validate_object_property_expression(prop, result);
            }
        }
    }

    /// Validate data property expression
    fn validate_data_property_expression(
        &self,
        expr: &DataPropertyExpression,
        result: &mut ValidationResult,
    ) {
        match expr {
            DataPropertyExpression::NamedProperty(name) => {
                self.validate_iri_reference(name, result);
            }
        }
    }

    /// Validate data range
    fn validate_data_range(&self, range: &DataRange, result: &mut ValidationResult) {
        match range {
            DataRange::Datatype(iri) => {
                self.validate_iri_reference(iri, result);
            }
            DataRange::DataIntersection(ranges) => {
                if ranges.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataIntersection requires at least 1 operand".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for r in ranges {
                    self.validate_data_range(r, result);
                }
            }
            DataRange::DataUnion(ranges) => {
                if ranges.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataUnion requires at least 1 operand".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for r in ranges {
                    self.validate_data_range(r, result);
                }
            }
            DataRange::DataComplement(range) => {
                self.validate_data_range(range, result);
            }
            DataRange::DataOneOf(literals) => {
                if literals.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataOneOf requires at least 1 literal".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                    return;
                }
                for literal in literals {
                    if literal.is_empty() {
                        result.errors.push(ValidationError {
                            message: "DataOneOf literal cannot be empty".to_string(),
                            location: None,
                            code: ErrorCode::MissingRequiredComponent,
                        });
                    }
                }
            }
            DataRange::DatatypeRestriction {
                datatype,
                restrictions,
            } => {
                self.validate_iri_reference(datatype, result);
                if restrictions.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DatatypeRestriction requires at least 1 restriction".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                }
            }
        }
    }

    /// Validate property assertion
    fn validate_property_assertion(
        &self,
        assertion: &PropertyAssertion,
        result: &mut ValidationResult,
    ) {
        match assertion {
            PropertyAssertion::ObjectPropertyAssertion {
                subject,
                property,
                object,
            } => {
                self.validate_iri_reference(subject, result);
                self.validate_object_property_expression(property, result);
                self.validate_iri_reference(object, result);
            }
            PropertyAssertion::DataPropertyAssertion {
                subject,
                property,
                object,
            } => {
                self.validate_iri_reference(subject, result);
                self.validate_data_property_expression(property, result);
                if object.is_empty() {
                    result.errors.push(ValidationError {
                        message: "DataPropertyAssertion literal cannot be empty".to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                }
            }
            PropertyAssertion::NegativeObjectPropertyAssertion {
                subject,
                property,
                object,
            } => {
                self.validate_iri_reference(subject, result);
                self.validate_object_property_expression(property, result);
                self.validate_iri_reference(object, result);
            }
            PropertyAssertion::NegativeDataPropertyAssertion {
                subject,
                property,
                object,
            } => {
                self.validate_iri_reference(subject, result);
                self.validate_data_property_expression(property, result);
                if object.is_empty() {
                    result.errors.push(ValidationError {
                        message: "NegativeDataPropertyAssertion literal cannot be empty"
                            .to_string(),
                        location: None,
                        code: ErrorCode::MissingRequiredComponent,
                    });
                }
            }
        }
    }

    /// Validate property characteristic
    fn validate_property_characteristic(
        &self,
        _characteristic: &PropertyCharacteristic,
        _result: &mut ValidationResult,
    ) {
        // All property characteristics are valid enum variants, so no validation needed here
        // This could be extended with specific rules for characteristic combinations
    }

    /// Validate entity name
    fn validate_entity_name(&self, name: &str, entity_type: &str, result: &mut ValidationResult) {
        if name.is_empty() {
            result.errors.push(ValidationError {
                message: format!("{} name cannot be empty", entity_type),
                location: None,
                code: ErrorCode::MissingRequiredComponent,
            });
            return;
        }

        // Check for invalid characters
        if name.chars().any(|c| c.is_control()) {
            result.errors.push(ValidationError {
                message: format!("{} name contains control characters: {}", entity_type, name),
                location: None,
                code: ErrorCode::InvalidIRI,
            });
        }

        // Validate as IRI reference
        self.validate_iri_reference(name, result);
    }

    /// Validate IRI reference
    fn validate_iri_reference(&self, iri_ref: &str, result: &mut ValidationResult) {
        if iri_ref.is_empty() {
            result.errors.push(ValidationError {
                message: "IRI reference cannot be empty".to_string(),
                location: None,
                code: ErrorCode::InvalidIRI,
            });
            return;
        }

        // Check for prefixed names
        if let Some((prefix, local_name)) = iri_ref.split_once(':') {
            if !self.prefixes.contains_key(prefix) {
                result.errors.push(ValidationError {
                    message: format!("Undefined prefix: {}", prefix),
                    location: None,
                    code: ErrorCode::UndefinedPrefix,
                });
            }

            // Validate local name
            if local_name.is_empty() {
                result.errors.push(ValidationError {
                    message: format!("Local name cannot be empty in prefixed name: {}", iri_ref),
                    location: None,
                    code: ErrorCode::InvalidIRI,
                });
            }
        } else {
            // Full IRI validation (basic check)
            if !(iri_ref.starts_with("http://")
                || iri_ref.starts_with("https://")
                || iri_ref.starts_with("urn:"))
            {
                result.warnings.push(ValidationWarning {
                    message: format!("IRI reference doesn't use standard scheme: {}", iri_ref),
                    location: None,
                    code: WarningCode::InvalidIRI,
                });
            }
        }
    }

    /// Validate cardinality value
    fn validate_cardinality(&self, cardinality: u32, result: &mut ValidationResult) {
        // Cardinality is always valid as u32, but could add range constraints
        if cardinality == 0 {
            result.warnings.push(ValidationWarning {
                message: format!("Cardinality of 0 may not be meaningful: {}", cardinality),
                location: None,
                code: WarningCode::InvalidCardinality,
            });
        }
    }

    /// Check for circular dependencies
    fn check_circular_dependencies(
        &self,
        class_name: &str,
        sub_class_of: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        // Simple circular dependency detection
        for expr in sub_class_of {
            if let ClassExpression::NamedClass(name) = &**expr {
                if name == class_name {
                    result.errors.push(ValidationError {
                        message: format!(
                            "Circular dependency detected: {} SubClassOf {}",
                            class_name, name
                        ),
                        location: None,
                        code: ErrorCode::CircularDependency,
                    });
                }
            }
        }
    }

    /// Check for redundant expressions
    fn check_redundant_expressions(
        &self,
        expressions: &SmallVec<[Box<ClassExpression>; 4]>,
        result: &mut ValidationResult,
    ) {
        if expressions.len() < 2 {
            return;
        }

        // Simple duplicate detection
        for i in 0..expressions.len() {
            for j in i + 1..expressions.len() {
                if *expressions[i] == *expressions[j] {
                    result.warnings.push(ValidationWarning {
                        message: format!("Duplicate expression detected: {:?}", expressions[i]),
                        location: None,
                        code: WarningCode::RedundantExpression,
                    });
                }
            }
        }
    }

    /// Check for incompatible property characteristics
    fn check_incompatible_characteristics(
        &self,
        characteristics: &[PropertyCharacteristic],
        result: &mut ValidationResult,
    ) {
        // Check for incompatible combinations
        let has_transitive = characteristics.contains(&PropertyCharacteristic::Transitive);
        let has_asymmetric = characteristics.contains(&PropertyCharacteristic::Asymmetric);
        let has_reflexive = characteristics.contains(&PropertyCharacteristic::Reflexive);
        let has_irreflexive = characteristics.contains(&PropertyCharacteristic::Irreflexive);

        if has_transitive && has_asymmetric {
            result.errors.push(ValidationError {
                message: "Property cannot be both Transitive and Asymmetric".to_string(),
                location: None,
                code: ErrorCode::InvalidPropertyCharacteristic,
            });
        }

        if has_reflexive && has_irreflexive {
            result.errors.push(ValidationError {
                message: "Property cannot be both Reflexive and Irreflexive".to_string(),
                location: None,
                code: ErrorCode::InvalidPropertyCharacteristic,
            });
        }
    }

    /// Check data property characteristics
    fn check_data_property_characteristics(
        &self,
        characteristics: &[PropertyCharacteristic],
        result: &mut ValidationResult,
    ) {
        for characteristic in characteristics {
            match characteristic {
                PropertyCharacteristic::Transitive
                | PropertyCharacteristic::Symmetric
                | PropertyCharacteristic::Asymmetric
                | PropertyCharacteristic::Reflexive
                | PropertyCharacteristic::Irreflexive => {
                    result.errors.push(ValidationError {
                        message: format!(
                            "Data properties cannot have {:?} characteristic",
                            characteristic
                        ),
                        location: None,
                        code: ErrorCode::InvalidPropertyCharacteristic,
                    });
                }
                PropertyCharacteristic::Functional
                | PropertyCharacteristic::InverseFunctional
                | PropertyCharacteristic::Annotation
                | PropertyCharacteristic::Ontology
                | PropertyCharacteristic::Data
                | PropertyCharacteristic::Object => {
                    // These are valid for data properties
                }
            }
        }
    }

    /// Get current validation context
    pub fn current_context(&self) -> Option<&ValidationContext> {
        self.context_stack.last()
    }

    /// Get all defined prefixes
    pub fn prefixes(&self) -> &HashMap<String, String> {
        &self.prefixes
    }

    /// Check if validation is in strict mode
    pub fn is_strict(&self) -> bool {
        self.strict_mode
    }

    /// Validate and convert to OwlResult
    pub fn validate_to_result(&mut self, ast: &ManchesterAST) -> OwlResult<()> {
        let result = self.validate_ast(ast);
        if result.is_valid {
            Ok(())
        } else {
            let error_messages: Vec<String> =
                result.errors.iter().map(|e| e.message.clone()).collect();
            Err(OwlError::ParseError(error_messages.join("\n")))
        }
    }
}

impl Default for SyntaxValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationResult {
    /// Create a new validation result
    pub fn new() -> Self {
        Self {
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    /// Add an error
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
        self.is_valid = false;
    }

    /// Check if validation passed
    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Get warnings
    pub fn warnings(&self) -> &[ValidationWarning] {
        &self.warnings
    }

    /// Get errors
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Get formatted error messages
    pub fn error_messages(&self) -> Vec<String> {
        self.errors.iter().map(|e| e.message.clone()).collect()
    }

    /// Get formatted warning messages
    pub fn warning_messages(&self) -> Vec<String> {
        self.warnings.iter().map(|w| w.message.clone()).collect()
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationContext {
    /// Create a new validation context
    pub fn new(entity: Option<String>, phase: ValidationPhase) -> Self {
        Self {
            entity,
            phase,
            parent_expressions: Vec::new(),
        }
    }

    /// Create a context for entity validation
    pub fn for_entity(entity: String, phase: ValidationPhase) -> Self {
        Self::new(Some(entity), phase)
    }

    /// Create a context for phase validation
    pub fn for_phase(phase: ValidationPhase) -> Self {
        Self::new(None, phase)
    }
}

#[cfg(test)]
mod tests {
    use super::super::syntax::{Annotation, AnnotationValue};
    use super::*;
    use smallvec::smallvec;

    /// Helper function to create a validator with common prefixes
    fn create_validator_with_prefixes() -> SyntaxValidator {
        let mut validator = SyntaxValidator::new();
        validator.add_prefix(
            "rdfs".to_string(),
            "http://www.w3.org/2000/01/rdf-schema#".to_string(),
        );
        validator.add_prefix(
            "owl".to_string(),
            "http://www.w3.org/2002/07/owl#".to_string(),
        );
        validator.add_prefix("ex".to_string(), "http://example.org/".to_string());
        validator.add_prefix(
            "dc".to_string(),
            "http://purl.org/dc/elements/1.1/".to_string(),
        );
        validator.add_prefix(
            "dcterms".to_string(),
            "http://purl.org/dc/terms/".to_string(),
        );
        validator
    }

    #[test]
    fn test_annotation_declaration_validation() {
        let mut validator = create_validator_with_prefixes();

        // Test valid annotation declaration
        let valid_annotation = ManchesterAST::AnnotationDeclaration {
            name: "rdfs:comment".to_string(),
            annotations: smallvec![Annotation {
                property: "rdfs:comment".to_string(),
                value: AnnotationValue::Literal("A test comment".to_string()),
            }],
        };

        let result = validator.validate_ast(&valid_annotation);
        if !result.is_valid {
            println!("Validation errors:");
            for error in &result.errors {
                println!("  Error: {}", error.message);
            }
        }
        if !result.warnings.is_empty() {
            println!("Validation warnings:");
            for warning in &result.warnings {
                println!("  Warning: {}", warning.message);
            }
        }
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_annotation_declaration_empty_name() {
        let mut validator = SyntaxValidator::new();

        let invalid_annotation = ManchesterAST::AnnotationDeclaration {
            name: "".to_string(),
            annotations: smallvec![],
        };

        let result = validator.validate_ast(&invalid_annotation);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("cannot be empty")));
    }

    #[test]
    fn test_builtin_annotation_property_validation() {
        let mut validator = create_validator_with_prefixes();

        // Test owl:deprecated with valid boolean value
        let valid_deprecated = ManchesterAST::AnnotationDeclaration {
            name: "owl:deprecated".to_string(),
            annotations: smallvec![Annotation {
                property: "owl:deprecated".to_string(),
                value: AnnotationValue::Literal("\"true\"".to_string()),
            }],
        };

        let result = validator.validate_ast(&valid_deprecated);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_rule_declaration_basic() {
        let mut validator = create_validator_with_prefixes();

        let valid_rule = ManchesterAST::RuleDeclaration {
            name: Some("ex:TestRule".to_string()),
            body: Box::new(smallvec![Box::new(ClassExpression::NamedClass(
                "ex:Person".to_string()
            )),]),
            head: Box::new(smallvec![Box::new(ClassExpression::NamedClass(
                "ex:Human".to_string()
            )),]),
            annotations: Box::new(smallvec![]),
        };

        let result = validator.validate_ast(&valid_rule);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_rule_declaration_empty() {
        let mut validator = create_validator_with_prefixes();

        let empty_rule = ManchesterAST::RuleDeclaration {
            name: None,
            body: Box::new(smallvec![]),
            head: Box::new(smallvec![]),
            annotations: Box::new(smallvec![]),
        };

        let result = validator.validate_ast(&empty_rule);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("at least one atom")));
    }

    #[test]
    fn test_rule_negation_in_head_error() {
        let mut validator = create_validator_with_prefixes();

        let rule_with_negation_in_head = ManchesterAST::RuleDeclaration {
            name: Some("ex:InvalidRule".to_string()),
            body: Box::new(smallvec![Box::new(ClassExpression::NamedClass(
                "ex:Person".to_string()
            )),]),
            head: Box::new(smallvec![Box::new(ClassExpression::ObjectComplement(
                Box::new(ClassExpression::NamedClass("ex:Student".to_string()))
            )),]),
            annotations: Box::new(smallvec![]),
        };

        let result = validator.validate_ast(&rule_with_negation_in_head);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("only allowed in rule body")));
    }

    #[test]
    fn test_rule_negation_in_body_valid() {
        let mut validator = create_validator_with_prefixes();

        let rule_with_negation_in_body = ManchesterAST::RuleDeclaration {
            name: Some("ex:ValidRule".to_string()),
            body: Box::new(smallvec![
                Box::new(ClassExpression::NamedClass("ex:Person".to_string())),
                Box::new(ClassExpression::ObjectComplement(Box::new(
                    ClassExpression::NamedClass("ex:Student".to_string())
                ))),
            ]),
            head: Box::new(smallvec![Box::new(ClassExpression::NamedClass(
                "ex:Adult".to_string()
            )),]),
            annotations: Box::new(smallvec![]),
        };

        let result = validator.validate_ast(&rule_with_negation_in_body);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_rule_head_contradiction() {
        let mut validator = create_validator_with_prefixes();
        validator.set_strict_mode(true);

        let contradictory_rule = ManchesterAST::RuleDeclaration {
            name: Some("ex:ContradictoryRule".to_string()),
            body: Box::new(smallvec![Box::new(ClassExpression::NamedClass(
                "ex:Person".to_string()
            )),]),
            head: Box::new(smallvec![
                Box::new(ClassExpression::NamedClass("ex:Student".to_string())),
                Box::new(ClassExpression::ObjectComplement(Box::new(
                    ClassExpression::NamedClass("ex:Student".to_string())
                ))),
            ]),
            annotations: Box::new(smallvec![]),
        };

        let result = validator.validate_ast(&contradictory_rule);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("contradiction")));
    }

    #[test]
    fn test_validation_result_utilities() {
        let mut result = ValidationResult::new();

        assert!(result.is_valid());
        assert!(result.warnings().is_empty());
        assert!(result.errors().is_empty());

        result.add_warning(ValidationWarning {
            message: "Test warning".to_string(),
            location: Some((1, 10)),
            code: WarningCode::ComplexExpression,
        });

        result.add_error(ValidationError {
            message: "Test error".to_string(),
            location: Some((2, 20)),
            code: ErrorCode::InvalidIRI,
        });

        assert!(!result.is_valid());
        assert_eq!(result.warnings().len(), 1);
        assert_eq!(result.errors().len(), 1);
        assert_eq!(result.warning_messages()[0], "Test warning");
        assert_eq!(result.error_messages()[0], "Test error");
    }

    #[test]
    fn test_builtin_annotation_property_recognition() {
        let validator = SyntaxValidator::new();

        // Test various built-in annotation properties
        assert!(validator.is_builtin_annotation_property("rdfs:label"));
        assert!(validator.is_builtin_annotation_property("rdfs:comment"));
        assert!(validator.is_builtin_annotation_property("owl:deprecated"));
        assert!(validator.is_builtin_annotation_property("dc:creator"));
        assert!(validator.is_builtin_annotation_property("dcterms:title"));

        // Test non-built-in properties
        assert!(!validator.is_builtin_annotation_property("ex:customProp"));
        assert!(!validator.is_builtin_annotation_property("unknown:property"));
    }
}
