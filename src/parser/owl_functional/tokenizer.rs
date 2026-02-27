//! Tokenizer for OWL Functional Syntax
//!
//! This module implements lexical analysis for OWL Functional Syntax,
//! converting input text into a stream of tokens for parsing.

use crate::parser::owl_functional::error::{FunctionalSyntaxError, FunctionalSyntaxResult};
use std::str::CharIndices;

/// Token types for OWL Functional Syntax
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    // Keywords
    Prefix,
    Ontology,
    Declaration,
    Class,
    ObjectProperty,
    DataProperty,
    NamedIndividual,
    AnonymousIndividual,
    AnnotationProperty,

    // Class expression constructors
    ObjectIntersectionOf,
    ObjectUnionOf,
    ObjectComplementOf,
    ObjectSomeValuesFrom,
    ObjectAllValuesFrom,
    ObjectHasValue,
    ObjectHasSelf,
    ObjectMinCardinality,
    ObjectMaxCardinality,
    ObjectExactCardinality,
    ObjectOneOf,
    DataSomeValuesFrom,
    DataAllValuesFrom,
    DataHasValue,
    DataMinCardinality,
    DataMaxCardinality,
    DataExactCardinality,
    DataIntersectionOf,
    DataUnionOf,
    DataComplementOf,
    DataOneOf,
    DatatypeRestriction,

    // Property expressions
    ObjectInverseOf,

    // Axiom types
    SubClassOf,
    EquivalentClasses,
    DisjointClasses,
    DisjointUnion,
    SubObjectPropertyOf,
    EquivalentObjectProperties,
    DisjointObjectProperties,
    ObjectPropertyDomain,
    ObjectPropertyRange,
    InverseObjectProperties,
    FunctionalObjectProperty,
    InverseFunctionalObjectProperty,
    ReflexiveObjectProperty,
    IrreflexiveObjectProperty,
    SymmetricObjectProperty,
    AsymmetricObjectProperty,
    TransitiveObjectProperty,
    SubDataPropertyOf,
    EquivalentDataProperties,
    DisjointDataProperties,
    DataPropertyDomain,
    DataPropertyRange,
    FunctionalDataProperty,
    ClassAssertion,
    ObjectPropertyAssertion,
    DataPropertyAssertion,
    NegativeObjectPropertyAssertion,
    NegativeDataPropertyAssertion,
    SameIndividual,
    DifferentIndividuals,
    HasKey,
    AnnotationAssertion,
    SubAnnotationPropertyOf,
    AnnotationPropertyDomain,
    AnnotationPropertyRange,
    Import,

    // Property characteristics
    Functional,
    InverseFunctional,
    Transitive,
    Symmetric,
    Asymmetric,
    Reflexive,
    Irreflexive,

    // Special symbols
    LeftParen,
    RightParen,
    Equals,
    Comma,
    Colon,
    Hash,
    LessThan,
    GreaterThan,
    Caret,
    At,

    // Literals and identifiers
    IRI,
    StringLiteral,
    NumberLiteral,
    Identifier,

    // Special tokens
    EOF,
    Newline,
}

/// A token in OWL Functional Syntax
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The type of the token
    pub token_type: TokenType,

    /// The lexeme (actual text) of the token
    pub lexeme: String,

    /// The line number where the token was found
    pub line: usize,

    /// The column number where the token was found
    pub column: usize,

    /// The position in the input stream
    pub position: usize,
}

impl Token {
    /// Create a new token
    pub fn new(
        token_type: TokenType,
        lexeme: String,
        line: usize,
        column: usize,
        position: usize,
    ) -> Self {
        Self {
            token_type,
            lexeme,
            line,
            column,
            position,
        }
    }

    /// Check if this token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::Prefix
                | TokenType::Ontology
                | TokenType::Declaration
                | TokenType::Class
                | TokenType::ObjectProperty
                | TokenType::DataProperty
                | TokenType::NamedIndividual
                | TokenType::AnonymousIndividual
                | TokenType::AnnotationProperty
                | TokenType::SubClassOf
                | TokenType::EquivalentClasses
                | TokenType::DisjointClasses
                | TokenType::DisjointUnion
                | TokenType::SubObjectPropertyOf
                | TokenType::EquivalentObjectProperties
                | TokenType::DisjointObjectProperties
                | TokenType::ObjectPropertyDomain
                | TokenType::ObjectPropertyRange
                | TokenType::InverseObjectProperties
                | TokenType::FunctionalObjectProperty
                | TokenType::InverseFunctionalObjectProperty
                | TokenType::ReflexiveObjectProperty
                | TokenType::IrreflexiveObjectProperty
                | TokenType::SymmetricObjectProperty
                | TokenType::AsymmetricObjectProperty
                | TokenType::TransitiveObjectProperty
                | TokenType::SubDataPropertyOf
                | TokenType::EquivalentDataProperties
                | TokenType::DisjointDataProperties
                | TokenType::DataPropertyDomain
                | TokenType::DataPropertyRange
                | TokenType::FunctionalDataProperty
                | TokenType::ClassAssertion
                | TokenType::ObjectPropertyAssertion
                | TokenType::DataPropertyAssertion
                | TokenType::NegativeObjectPropertyAssertion
                | TokenType::NegativeDataPropertyAssertion
                | TokenType::SameIndividual
                | TokenType::DifferentIndividuals
                | TokenType::HasKey
                | TokenType::AnnotationAssertion
                | TokenType::SubAnnotationPropertyOf
                | TokenType::AnnotationPropertyDomain
                | TokenType::AnnotationPropertyRange
                | TokenType::Import
        )
    }

    /// Check if this token is a punctuation
    pub fn is_punctuation(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::LeftParen
                | TokenType::RightParen
                | TokenType::Equals
                | TokenType::Comma
                | TokenType::Colon
                | TokenType::Hash
                | TokenType::LessThan
                | TokenType::GreaterThan
                | TokenType::Caret
                | TokenType::At
        )
    }
}

/// Tokenizer for OWL Functional Syntax
pub struct Tokenizer<'a> {
    /// The input text
    #[allow(dead_code)]
    input: &'a str,
    /// Iterator over character indices
    chars: CharIndices<'a>,
    /// Current line number
    line: usize,
    /// Current column number
    column: usize,
    /// Current position in the input
    position: usize,
    /// Whether we're at the end of input
    at_end: bool,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices(),
            line: 1,
            column: 1,
            position: 0,
            at_end: false,
        }
    }

    /// Get the next token from the input
    pub fn next_token(&mut self) -> FunctionalSyntaxResult<Token> {
        self.skip_whitespace();

        if self.at_end {
            return Ok(Token::new(
                TokenType::EOF,
                String::new(),
                self.line,
                self.column,
                self.position,
            ));
        }

        let (pos, ch) = match self.chars.next() {
            Some(c) => c,
            None => {
                self.at_end = true;
                return Ok(Token::new(
                    TokenType::EOF,
                    String::new(),
                    self.line,
                    self.column,
                    self.position,
                ));
            }
        };

        self.position = pos;
        let start_column = self.column;

        match ch {
            // Punctuation
            '(' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::LeftParen,
                    "(".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            ')' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::RightParen,
                    ")".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '=' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::Equals,
                    "=".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            ',' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::Comma,
                    ",".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            ':' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::Colon,
                    ":".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '#' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::Hash,
                    "#".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '<' => self.parse_iri(pos, start_column),
            '>' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::GreaterThan,
                    ">".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '^' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::Caret,
                    "^".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '@' => {
                self.column += 1;
                Ok(Token::new(
                    TokenType::At,
                    "@".to_string(),
                    self.line,
                    start_column,
                    pos,
                ))
            }
            '"' => self.parse_string_literal(pos, start_column),
            '0'..='9' | '-' => self.parse_number_literal(pos, start_column),
            'a'..='z' | 'A'..='Z' | '_' => self.parse_identifier_or_keyword(pos, start_column, ch),
            '\n' => {
                self.line += 1;
                self.column = 1;
                Ok(Token::new(
                    TokenType::Newline,
                    "\n".to_string(),
                    self.line - 1,
                    start_column,
                    pos,
                ))
            }
            _ => Err(FunctionalSyntaxError::Tokenization(format!(
                "Unexpected character '{}' at line {}, column {}",
                ch, self.line, self.column
            ))),
        }
    }

    /// Parse an IRI enclosed in angle brackets
    fn parse_iri(
        &mut self,
        start_pos: usize,
        start_column: usize,
    ) -> FunctionalSyntaxResult<Token> {
        let mut iri = String::new();
        iri.push('<');
        self.column += 1;

        for (pos, ch) in self.chars.by_ref() {
            self.position = pos;
            self.column += 1;
            iri.push(ch);

            if ch == '>' {
                return Ok(Token::new(
                    TokenType::IRI,
                    iri,
                    self.line,
                    start_column,
                    start_pos,
                ));
            }
        }

        Err(FunctionalSyntaxError::Tokenization(
            "Unterminated IRI".to_string(),
        ))
    }

    /// Parse a string literal
    fn parse_string_literal(
        &mut self,
        start_pos: usize,
        start_column: usize,
    ) -> FunctionalSyntaxResult<Token> {
        let mut literal = String::new();
        literal.push('"');
        self.column += 1;

        while let Some((pos, ch)) = self.chars.next() {
            self.position = pos;
            self.column += 1;
            literal.push(ch);

            if ch == '"' {
                return Ok(Token::new(
                    TokenType::StringLiteral,
                    literal,
                    self.line,
                    start_column,
                    start_pos,
                ));
            } else if ch == '\\' {
                // Handle escape sequences
                if let Some((_, esc_ch)) = self.chars.next() {
                    self.column += 1;
                    literal.push(esc_ch);
                }
            }
        }

        Err(FunctionalSyntaxError::Tokenization(
            "Unterminated string literal".to_string(),
        ))
    }

    /// Parse a number literal
    fn parse_number_literal(
        &mut self,
        start_pos: usize,
        start_column: usize,
    ) -> FunctionalSyntaxResult<Token> {
        let mut number = String::new();
        number.push('-'); // If it started with '-'
        self.column += 1;

        while let Some((pos, ch)) = self.chars.clone().next() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-'
            {
                if let Some((_, ch)) = self.chars.next() {
                    self.position = pos;
                    self.column += 1;
                    number.push(ch);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(Token::new(
            TokenType::NumberLiteral,
            number,
            self.line,
            start_column,
            start_pos,
        ))
    }

    /// Parse an identifier or keyword
    fn parse_identifier_or_keyword(
        &mut self,
        start_pos: usize,
        start_column: usize,
        first_ch: char,
    ) -> FunctionalSyntaxResult<Token> {
        let mut identifier = String::new();
        identifier.push(first_ch);

        while let Some((pos, ch)) = self.chars.clone().next() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                if let Some((_, ch)) = self.chars.next() {
                    self.position = pos;
                    self.column += 1;
                    identifier.push(ch);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Check if it's a keyword
        let token_type = match identifier.as_str() {
            "Prefix" => TokenType::Prefix,
            "Ontology" => TokenType::Ontology,
            "Declaration" => TokenType::Declaration,
            "Class" => TokenType::Class,
            "ObjectProperty" => TokenType::ObjectProperty,
            "DataProperty" => TokenType::DataProperty,
            "NamedIndividual" => TokenType::NamedIndividual,
            "AnonymousIndividual" => TokenType::AnonymousIndividual,
            "AnnotationProperty" => TokenType::AnnotationProperty,
            "ObjectIntersectionOf" => TokenType::ObjectIntersectionOf,
            "ObjectUnionOf" => TokenType::ObjectUnionOf,
            "ObjectComplementOf" => TokenType::ObjectComplementOf,
            "ObjectSomeValuesFrom" => TokenType::ObjectSomeValuesFrom,
            "ObjectAllValuesFrom" => TokenType::ObjectAllValuesFrom,
            "SubClassOf" => TokenType::SubClassOf,
            "EquivalentClasses" => TokenType::EquivalentClasses,
            "DisjointClasses" => TokenType::DisjointClasses,
            "DisjointUnion" => TokenType::DisjointUnion,
            "SubObjectPropertyOf" => TokenType::SubObjectPropertyOf,
            "EquivalentObjectProperties" => TokenType::EquivalentObjectProperties,
            "DisjointObjectProperties" => TokenType::DisjointObjectProperties,
            "ObjectPropertyDomain" => TokenType::ObjectPropertyDomain,
            "ObjectPropertyRange" => TokenType::ObjectPropertyRange,
            "InverseObjectProperties" => TokenType::InverseObjectProperties,
            "FunctionalObjectProperty" => TokenType::FunctionalObjectProperty,
            "InverseFunctionalObjectProperty" => TokenType::InverseFunctionalObjectProperty,
            "ReflexiveObjectProperty" => TokenType::ReflexiveObjectProperty,
            "IrreflexiveObjectProperty" => TokenType::IrreflexiveObjectProperty,
            "SymmetricObjectProperty" => TokenType::SymmetricObjectProperty,
            "AsymmetricObjectProperty" => TokenType::AsymmetricObjectProperty,
            "TransitiveObjectProperty" => TokenType::TransitiveObjectProperty,
            "SubDataPropertyOf" => TokenType::SubDataPropertyOf,
            "EquivalentDataProperties" => TokenType::EquivalentDataProperties,
            "DisjointDataProperties" => TokenType::DisjointDataProperties,
            "DataPropertyDomain" => TokenType::DataPropertyDomain,
            "DataPropertyRange" => TokenType::DataPropertyRange,
            "FunctionalDataProperty" => TokenType::FunctionalDataProperty,
            "ClassAssertion" => TokenType::ClassAssertion,
            "ObjectPropertyAssertion" => TokenType::ObjectPropertyAssertion,
            "DataPropertyAssertion" => TokenType::DataPropertyAssertion,
            "NegativeObjectPropertyAssertion" => TokenType::NegativeObjectPropertyAssertion,
            "NegativeDataPropertyAssertion" => TokenType::NegativeDataPropertyAssertion,
            "SameIndividual" => TokenType::SameIndividual,
            "DifferentIndividuals" => TokenType::DifferentIndividuals,
            "HasKey" => TokenType::HasKey,
            "AnnotationAssertion" => TokenType::AnnotationAssertion,
            "SubAnnotationPropertyOf" => TokenType::SubAnnotationPropertyOf,
            "AnnotationPropertyDomain" => TokenType::AnnotationPropertyDomain,
            "AnnotationPropertyRange" => TokenType::AnnotationPropertyRange,
            "Import" => TokenType::Import,
            _ => TokenType::Identifier,
        };

        Ok(Token::new(
            token_type,
            identifier,
            self.line,
            start_column,
            start_pos,
        ))
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while let Some((_, ch)) = self.chars.clone().next() {
            if ch == '\n' {
                self.chars.next();
                self.line += 1;
                self.column = 1;
            } else if ch.is_whitespace() {
                self.chars.next();
                self.column += 1;
            } else {
                break;
            }
        }
    }

    /// Tokenize the entire input and return all tokens
    pub fn tokenize(mut self) -> FunctionalSyntaxResult<Vec<Token>> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            if token.token_type == TokenType::EOF {
                break;
            }
            tokens.push(token);
        }

        Ok(tokens)
    }

    /// Peek at the next token without consuming it
    pub fn peek_token(&mut self) -> FunctionalSyntaxResult<Token> {
        let current_pos = self.position;
        let current_line = self.line;
        let current_column = self.column;
        let current_chars = self.chars.clone();

        let token = self.next_token()?;

        // Restore state
        self.position = current_pos;
        self.line = current_line;
        self.column = current_column;
        self.chars = current_chars;

        Ok(token)
    }
}
