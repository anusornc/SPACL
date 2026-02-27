//! Error handling for OWL Functional Syntax parser
//!
//! This module defines error types and error handling utilities specific
//! to the OWL Functional Syntax parser.

use crate::core::error::OwlError;
use std::fmt;

/// Specialized error type for OWL Functional Syntax parsing
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionalSyntaxError {
    /// Tokenization error
    Tokenization(String),
    /// Syntax error with position information
    Syntax {
        message: String,
        line: usize,
        column: usize,
    },
    /// Grammar rule violation
    Grammar(String),
    /// Invalid IRI reference
    InvalidIRI(String),
    /// Invalid literal format
    InvalidLiteral(String),
    /// Invalid class expression
    InvalidClassExpression(String),
    /// Invalid property expression
    InvalidPropertyExpression(String),
    /// Missing required component
    MissingComponent(String),
    /// Unexpected token
    UnexpectedToken { expected: String, found: String },
    /// Unknown axiom type
    UnknownAxiom(String),
    /// Validation error
    Validation(String),
}

impl fmt::Display for FunctionalSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionalSyntaxError::Tokenization(msg) => write!(f, "Tokenization error: {}", msg),
            FunctionalSyntaxError::Syntax {
                message,
                line,
                column,
            } => {
                write!(
                    f,
                    "Syntax error at line {}, column {}: {}",
                    line, column, message
                )
            }
            FunctionalSyntaxError::Grammar(msg) => write!(f, "Grammar error: {}", msg),
            FunctionalSyntaxError::InvalidIRI(msg) => write!(f, "Invalid IRI: {}", msg),
            FunctionalSyntaxError::InvalidLiteral(msg) => write!(f, "Invalid literal: {}", msg),
            FunctionalSyntaxError::InvalidClassExpression(msg) => {
                write!(f, "Invalid class expression: {}", msg)
            }
            FunctionalSyntaxError::InvalidPropertyExpression(msg) => {
                write!(f, "Invalid property expression: {}", msg)
            }
            FunctionalSyntaxError::MissingComponent(msg) => write!(f, "Missing component: {}", msg),
            FunctionalSyntaxError::UnexpectedToken { expected, found } => {
                write!(f, "Expected '{}', found '{}'", expected, found)
            }
            FunctionalSyntaxError::UnknownAxiom(msg) => write!(f, "Unknown axiom type: {}", msg),
            FunctionalSyntaxError::Validation(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for FunctionalSyntaxError {}

impl From<FunctionalSyntaxError> for OwlError {
    fn from(err: FunctionalSyntaxError) -> Self {
        OwlError::ParseError(err.to_string())
    }
}

/// Specialized result type for OWL Functional Syntax operations
pub type FunctionalSyntaxResult<T> = Result<T, FunctionalSyntaxError>;

/// Create a syntax error with position information
pub fn syntax_error(message: String, line: usize, column: usize) -> FunctionalSyntaxError {
    FunctionalSyntaxError::Syntax {
        message,
        line,
        column,
    }
}

/// Create a grammar error
pub fn grammar_error(message: String) -> FunctionalSyntaxError {
    FunctionalSyntaxError::Grammar(message)
}

/// Create an invalid IRI error
pub fn invalid_iri_error(message: String) -> FunctionalSyntaxError {
    FunctionalSyntaxError::InvalidIRI(message)
}

/// Create an invalid literal error
pub fn invalid_literal_error(message: String) -> FunctionalSyntaxError {
    FunctionalSyntaxError::InvalidLiteral(message)
}

/// Create a validation error
pub fn validation_error(message: String) -> FunctionalSyntaxError {
    FunctionalSyntaxError::Validation(message)
}
