//! Manchester Syntax Error Handling
//!
//! This module provides comprehensive error handling and recovery
//! for Manchester Syntax parsing, including detailed error messages
//! and location information.

use crate::core::error::OwlError;
use std::fmt;

/// Manchester Syntax parsing errors
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Unexpected token found
    UnexpectedToken {
        expected: Vec<String>,
        found: String,
        line: usize,
        column: usize,
    },

    /// Expected identifier but found something else
    ExpectedIdentifier {
        found: String,
        line: usize,
        column: usize,
    },

    /// Expected class expression but found something else
    ExpectedClassExpression {
        found: String,
        line: usize,
        column: usize,
    },

    /// Expected property expression but found something else
    ExpectedPropertyExpression {
        found: String,
        line: usize,
        column: usize,
    },

    /// Expected individual but found something else
    ExpectedIndividual {
        found: String,
        line: usize,
        column: usize,
    },

    /// Expected data range but found something else
    ExpectedDataRange {
        found: String,
        line: usize,
        column: usize,
    },

    /// Invalid property characteristic
    InvalidPropertyCharacteristic {
        characteristic: String,
        line: usize,
        column: usize,
    },

    /// Invalid annotation
    InvalidAnnotation {
        message: String,
        line: usize,
        column: usize,
    },

    /// Undefined prefix
    UndefinedPrefix {
        prefix: String,
        line: usize,
        column: usize,
    },

    /// Duplicate declaration
    DuplicateDeclaration {
        name: String,
        declaration_type: String,
        line: usize,
        column: usize,
    },

    /// Missing required field
    MissingRequiredField {
        field_name: String,
        declaration_type: String,
        line: usize,
        column: usize,
    },

    /// Invalid cardinality
    InvalidCardinality {
        value: String,
        line: usize,
        column: usize,
    },

    /// Invalid IRI
    InvalidIRI {
        iri: String,
        reason: String,
        line: usize,
        column: usize,
    },

    /// Syntax error
    SyntaxError {
        message: String,
        line: usize,
        column: usize,
    },

    /// Incomplete expression
    IncompleteExpression {
        expression_type: String,
        line: usize,
        column: usize,
    },

    /// Circular dependency detected
    CircularDependency {
        entity1: String,
        entity2: String,
        dependency_type: String,
        line: usize,
        column: usize,
    },
}

impl ParseError {
    /// Get the line number where the error occurred
    pub fn line(&self) -> usize {
        match self {
            ParseError::UnexpectedToken { line, .. } => *line,
            ParseError::ExpectedIdentifier { line, .. } => *line,
            ParseError::ExpectedClassExpression { line, .. } => *line,
            ParseError::ExpectedPropertyExpression { line, .. } => *line,
            ParseError::ExpectedIndividual { line, .. } => *line,
            ParseError::ExpectedDataRange { line, .. } => *line,
            ParseError::InvalidPropertyCharacteristic { line, .. } => *line,
            ParseError::InvalidAnnotation { line, .. } => *line,
            ParseError::UndefinedPrefix { line, .. } => *line,
            ParseError::DuplicateDeclaration { line, .. } => *line,
            ParseError::MissingRequiredField { line, .. } => *line,
            ParseError::InvalidCardinality { line, .. } => *line,
            ParseError::InvalidIRI { line, .. } => *line,
            ParseError::SyntaxError { line, .. } => *line,
            ParseError::IncompleteExpression { line, .. } => *line,
            ParseError::CircularDependency { line, .. } => *line,
        }
    }

    /// Get the column number where the error occurred
    pub fn column(&self) -> usize {
        match self {
            ParseError::UnexpectedToken { column, .. } => *column,
            ParseError::ExpectedIdentifier { column, .. } => *column,
            ParseError::ExpectedClassExpression { column, .. } => *column,
            ParseError::ExpectedPropertyExpression { column, .. } => *column,
            ParseError::ExpectedIndividual { column, .. } => *column,
            ParseError::ExpectedDataRange { column, .. } => *column,
            ParseError::InvalidPropertyCharacteristic { column, .. } => *column,
            ParseError::InvalidAnnotation { column, .. } => *column,
            ParseError::UndefinedPrefix { column, .. } => *column,
            ParseError::DuplicateDeclaration { column, .. } => *column,
            ParseError::MissingRequiredField { column, .. } => *column,
            ParseError::InvalidCardinality { column, .. } => *column,
            ParseError::InvalidIRI { column, .. } => *column,
            ParseError::SyntaxError { column, .. } => *column,
            ParseError::IncompleteExpression { column, .. } => *column,
            ParseError::CircularDependency { column, .. } => *column,
        }
    }

    /// Get a human-readable error message
    pub fn message(&self) -> String {
        match self {
            ParseError::UnexpectedToken {
                expected, found, ..
            } => {
                if expected.is_empty() {
                    format!("Unexpected token: {}", found)
                } else if expected.len() == 1 {
                    format!("Expected {}, but found {}", expected[0], found)
                } else {
                    format!(
                        "Expected one of [{}], but found {}",
                        expected.join(", "),
                        found
                    )
                }
            }
            ParseError::ExpectedIdentifier { found, .. } => {
                format!("Expected identifier, but found {}", found)
            }
            ParseError::ExpectedClassExpression { found, .. } => {
                format!("Expected class expression, but found {}", found)
            }
            ParseError::ExpectedPropertyExpression { found, .. } => {
                format!("Expected property expression, but found {}", found)
            }
            ParseError::ExpectedIndividual { found, .. } => {
                format!("Expected individual, but found {}", found)
            }
            ParseError::ExpectedDataRange { found, .. } => {
                format!("Expected data range, but found {}", found)
            }
            ParseError::InvalidPropertyCharacteristic { characteristic, .. } => {
                format!("Invalid property characteristic: {}", characteristic)
            }
            ParseError::InvalidAnnotation { message, .. } => {
                format!("Invalid annotation: {}", message)
            }
            ParseError::UndefinedPrefix { prefix, .. } => {
                format!("Undefined prefix: {}", prefix)
            }
            ParseError::DuplicateDeclaration {
                name,
                declaration_type,
                ..
            } => {
                format!("Duplicate {} declaration: {}", declaration_type, name)
            }
            ParseError::MissingRequiredField {
                field_name,
                declaration_type,
                ..
            } => {
                format!(
                    "Missing required field '{}' for {} declaration",
                    field_name, declaration_type
                )
            }
            ParseError::InvalidCardinality { value, .. } => {
                format!("Invalid cardinality value: {}", value)
            }
            ParseError::InvalidIRI { iri, reason, .. } => {
                format!("Invalid IRI '{}': {}", iri, reason)
            }
            ParseError::SyntaxError { message, .. } => {
                format!("Syntax error: {}", message)
            }
            ParseError::IncompleteExpression {
                expression_type, ..
            } => {
                format!("Incomplete {} expression", expression_type)
            }
            ParseError::CircularDependency {
                entity1,
                entity2,
                dependency_type,
                ..
            } => {
                format!(
                    "Circular dependency detected: {} depends on {} via {}",
                    entity1, entity2, dependency_type
                )
            }
        }
    }

    /// Create an unexpected token error
    pub fn unexpected_token<S: Into<String>>(
        expected: Vec<S>,
        found: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::UnexpectedToken {
            expected: expected.into_iter().map(|s| s.into()).collect(),
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an expected identifier error
    pub fn expected_identifier<S: Into<String>>(found: S, line: usize, column: usize) -> Self {
        ParseError::ExpectedIdentifier {
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an expected class expression error
    pub fn expected_class_expression<S: Into<String>>(
        found: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::ExpectedClassExpression {
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an expected property expression error
    pub fn expected_property_expression<S: Into<String>>(
        found: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::ExpectedPropertyExpression {
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an expected data range error
    pub fn expected_data_range<S: Into<String>>(found: S, line: usize, column: usize) -> Self {
        ParseError::ExpectedDataRange {
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an expected individual error
    pub fn expected_individual<S: Into<String>>(found: S, line: usize, column: usize) -> Self {
        ParseError::ExpectedIndividual {
            found: found.into(),
            line,
            column,
        }
    }

    /// Create an invalid property characteristic error
    pub fn invalid_property_characteristic<S: Into<String>>(
        characteristic: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::InvalidPropertyCharacteristic {
            characteristic: characteristic.into(),
            line,
            column,
        }
    }

    /// Create an undefined prefix error
    pub fn undefined_prefix<S: Into<String>>(prefix: S, line: usize, column: usize) -> Self {
        ParseError::UndefinedPrefix {
            prefix: prefix.into(),
            line,
            column,
        }
    }

    /// Create a duplicate declaration error
    pub fn duplicate_declaration<S: Into<String>>(
        name: S,
        declaration_type: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::DuplicateDeclaration {
            name: name.into(),
            declaration_type: declaration_type.into(),
            line,
            column,
        }
    }

    /// Create a syntax error
    pub fn syntax_error<S: Into<String>>(message: S, line: usize, column: usize) -> Self {
        ParseError::SyntaxError {
            message: message.into(),
            line,
            column,
        }
    }

    /// Create an incomplete expression error
    pub fn incomplete_expression<S: Into<String>>(
        expression_type: S,
        line: usize,
        column: usize,
    ) -> Self {
        ParseError::IncompleteExpression {
            expression_type: expression_type.into(),
            line,
            column,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at line {}, column {}: {}",
            self.line(),
            self.column(),
            self.message()
        )
    }
}

impl std::error::Error for ParseError {}

/// Parse result type
pub type ParseResult<T> = Result<T, ParseError>;

/// Error collection for multiple parsing errors
#[derive(Debug, Clone, Default)]
pub struct ErrorCollection {
    errors: Vec<ParseError>,
    warnings: Vec<String>,
}

impl ErrorCollection {
    /// Create a new error collection
    pub fn new() -> Self {
        ErrorCollection {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add an error to the collection
    pub fn add_error(&mut self, error: ParseError) {
        self.errors.push(error);
    }

    /// Add a warning to the collection
    pub fn add_warning<S: Into<String>>(&mut self, warning: S) {
        self.warnings.push(warning.into());
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Get the number of errors
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Get the number of warnings
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }

    /// Get all errors
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Get all warnings
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Convert to OwlError if there are errors
    pub fn to_owl_error(&self) -> Option<OwlError> {
        if self.has_errors() {
            let error_messages: Vec<String> = self.errors.iter().map(|e| e.to_string()).collect();
            Some(OwlError::ParseError(error_messages.join("\n")))
        } else {
            None
        }
    }

    /// Clear all errors and warnings
    pub fn clear(&mut self) {
        self.errors.clear();
        self.warnings.clear();
    }
}

/// Error recovery strategy
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStrategy {
    /// Panic mode - skip to next synchronization point
    PanicMode,

    /// Error production - continue with error node
    ErrorProduction,

    /// Best effort - try to recover and continue
    BestEffort,
}

/// Error context for improved error reporting
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The current file being parsed
    pub file_path: Option<String>,

    /// The input text being parsed
    pub input_text: String,

    /// Current parsing context
    pub context_stack: Vec<String>,

    /// Active prefixes
    pub prefixes: std::collections::HashMap<String, String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(input_text: String) -> Self {
        ErrorContext {
            file_path: None,
            input_text,
            context_stack: Vec::new(),
            prefixes: std::collections::HashMap::new(),
        }
    }

    /// Set the file path
    pub fn with_file_path<S: Into<String>>(mut self, file_path: S) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    /// Push a context onto the stack
    pub fn push_context<S: Into<String>>(&mut self, context: S) {
        self.context_stack.push(context.into());
    }

    /// Pop a context from the stack
    pub fn pop_context(&mut self) -> Option<String> {
        self.context_stack.pop()
    }

    /// Get the current context
    pub fn current_context(&self) -> Option<&str> {
        self.context_stack.last().map(|s| s.as_str())
    }

    /// Add a prefix mapping
    pub fn add_prefix<S: Into<String>>(&mut self, prefix: S, iri: S) {
        self.prefixes.insert(prefix.into(), iri.into());
    }

    /// Resolve a prefixed name to an IRI
    pub fn resolve_prefix(&self, prefixed_name: &str) -> Option<String> {
        if let Some((prefix, local_name)) = prefixed_name.split_once(':') {
            self.prefixes
                .get(prefix)
                .map(|iri| format!("{}{}", iri, local_name))
        } else {
            None
        }
    }

    /// Get the line of text at a given position
    pub fn get_line_text(&self, line: usize) -> Option<&str> {
        let lines: Vec<&str> = self.input_text.lines().collect();
        lines.get(line.checked_sub(1)?).copied()
    }

    /// Create a formatted error message with context
    pub fn format_error(&self, error: &ParseError) -> String {
        let mut result = String::new();

        // Add file and location information
        if let Some(ref file_path) = self.file_path {
            result.push_str(&format!("{}:", file_path));
        }
        result.push_str(&format!("{}:{}: ", error.line(), error.column()));

        // Add error message
        result.push_str(&error.message());

        // Add context if available
        if let Some(context) = self.current_context() {
            result.push_str(&format!(" (in {})", context));
        }

        // Add the line of text where the error occurred
        if let Some(line_text) = self.get_line_text(error.line()) {
            result.push('\n');
            result.push_str(line_text);
            result.push('\n');

            // Add pointer to the exact column
            for _ in 0..error.column().saturating_sub(1) {
                result.push(' ');
            }
            result.push('^');
        }

        result
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// Error reporter for collecting and reporting parse errors
pub struct ErrorReporter {
    context: ErrorContext,
    errors: ErrorCollection,
    recovery_strategy: RecoveryStrategy,
}

impl ErrorReporter {
    /// Create a new error reporter
    pub fn new(context: ErrorContext) -> Self {
        ErrorReporter {
            context,
            errors: ErrorCollection::new(),
            recovery_strategy: RecoveryStrategy::BestEffort,
        }
    }

    /// Set the recovery strategy
    pub fn with_recovery_strategy(mut self, strategy: RecoveryStrategy) -> Self {
        self.recovery_strategy = strategy;
        self
    }

    /// Report an error
    pub fn report_error(&mut self, error: ParseError) {
        self.errors.add_error(error);
    }

    /// Report a warning
    pub fn report_warning<S: Into<String>>(&mut self, warning: S) {
        self.errors.add_warning(warning);
    }

    /// Get the error context
    pub fn context(&self) -> &ErrorContext {
        &self.context
    }

    /// Get mutable error context
    pub fn context_mut(&mut self) -> &mut ErrorContext {
        &mut self.context
    }

    /// Get the error collection
    pub fn errors(&self) -> &ErrorCollection {
        &self.errors
    }

    /// Get mutable error collection
    pub fn errors_mut(&mut self) -> &mut ErrorCollection {
        &mut self.errors
    }

    /// Format all errors for display
    pub fn format_errors(&self) -> String {
        let mut result = String::new();

        if self.errors.has_errors() {
            result.push_str("Errors:\n");
            for error in &self.errors.errors {
                result.push_str(&self.context.format_error(error));
                result.push('\n');
            }
        }

        if self.errors.has_warnings() {
            result.push_str("Warnings:\n");
            for warning in &self.errors.warnings {
                result.push_str(&format!("Warning: {}\n", warning));
            }
        }

        result
    }

    /// Check if parsing should continue based on recovery strategy
    pub fn should_continue(&self) -> bool {
        match self.recovery_strategy {
            RecoveryStrategy::PanicMode => !self.errors.has_errors(),
            RecoveryStrategy::ErrorProduction => true,
            RecoveryStrategy::BestEffort => self.errors.error_count() < 10, // Limit errors for practicality
        }
    }

    /// Clear all errors and warnings
    pub fn clear(&mut self) {
        self.errors.clear();
    }
}
