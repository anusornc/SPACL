//! Manchester Syntax Parser for OWL2 Ontologies
//!
//! This module implements a comprehensive parser for the Manchester Syntax,
//! a human-readable text-based syntax for OWL2 ontologies. The parser is
//! modularized into separate components for better maintainability.
//!
//! ## Module Structure
//!
//! - **tokenizer**: Lexical analysis and tokenization
//! - **grammar**: Grammar rules and production handling
//! - **parser**: Main parsing logic and AST construction
//! - **syntax**: Syntax tree definitions and utilities
//! - **error**: Error handling and recovery
//! - **validator**: Semantic validation

pub mod error;
pub mod grammar;
pub mod parser;
pub mod syntax;
pub mod tokenizer;
pub mod validator;

// Re-export main types for backward compatibility
pub use error::{ParseError, ParseResult};
pub use parser::ManchesterParser;
pub use syntax::{ClassExpression, ManchesterAST, ObjectPropertyExpression};
pub use tokenizer::{ManchesterTokenizer, Token, TokenType};
pub use validator::SyntaxValidator;
