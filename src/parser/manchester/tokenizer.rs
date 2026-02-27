//! Manchester Syntax Tokenizer
//!
//! This module implements lexical analysis for Manchester Syntax,
//! converting input text into a stream of tokens for parsing.

use crate::core::error::{OwlError, OwlResult};
use std::fmt;

/// Token types for Manchester Syntax
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    // Keywords
    Prefix,
    Class,
    ObjectProperty,
    DataProperty,
    Individual,
    SubClassOf,
    EquivalentTo,
    DisjointWith,
    Domain,
    Range,
    Characteristics,
    SubPropertyOf,
    InverseOf,
    Types,
    Facts,
    SameAs,
    DifferentFrom,
    Annotations,
    Rule,

    // Logical operators
    And,
    Or,
    Not,
    Some,
    Only,
    Value,
    SelfValue,
    Min,
    Max,
    Exactly,
    Inverse,

    // Property characteristics
    Functional,
    InverseFunctional,
    Transitive,
    Symmetric,
    Asymmetric,
    Reflexive,
    Irreflexive,

    // Data types
    Datatype,

    // Special symbols
    Colon,
    Semicolon,
    Comma,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Equals,
    Underscore,

    // Literals and identifiers
    Identifier,
    StringLiteral,
    NumberLiteral,
    IRI,

    // Special tokens
    EOF,
    Newline,
    Comment,
}

/// A token in Manchester Syntax
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
        Token {
            token_type,
            lexeme,
            line,
            column,
            position,
        }
    }

    /// Create an EOF token
    pub fn eof(position: usize) -> Self {
        Token::new(TokenType::EOF, String::new(), 0, 0, position)
    }

    /// Check if this token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::Prefix
                | TokenType::Class
                | TokenType::ObjectProperty
                | TokenType::DataProperty
                | TokenType::Individual
                | TokenType::SubClassOf
                | TokenType::EquivalentTo
                | TokenType::DisjointWith
                | TokenType::Domain
                | TokenType::Range
                | TokenType::Characteristics
                | TokenType::SubPropertyOf
                | TokenType::InverseOf
                | TokenType::Types
                | TokenType::Facts
                | TokenType::SameAs
                | TokenType::DifferentFrom
                | TokenType::Annotations
                | TokenType::Rule
        )
    }

    /// Check if this token is a logical operator
    pub fn is_logical_operator(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::And | TokenType::Or | TokenType::Not | TokenType::Some | TokenType::Only
        )
    }

    /// Check if this token is a property characteristic
    pub fn is_property_characteristic(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::Functional
                | TokenType::InverseFunctional
                | TokenType::Transitive
                | TokenType::Symmetric
                | TokenType::Asymmetric
                | TokenType::Reflexive
                | TokenType::Irreflexive
        )
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({}) at {}:{}",
            self.token_type, self.lexeme, self.line, self.column
        )
    }
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Prefix => write!(f, "PREFIX"),
            TokenType::Class => write!(f, "CLASS"),
            TokenType::ObjectProperty => write!(f, "OBJECT_PROPERTY"),
            TokenType::DataProperty => write!(f, "DATA_PROPERTY"),
            TokenType::Individual => write!(f, "INDIVIDUAL"),
            TokenType::SubClassOf => write!(f, "SUBCLASS_OF"),
            TokenType::EquivalentTo => write!(f, "EQUIVALENT_TO"),
            TokenType::DisjointWith => write!(f, "DISJOINT_WITH"),
            TokenType::Domain => write!(f, "DOMAIN"),
            TokenType::Range => write!(f, "RANGE"),
            TokenType::Characteristics => write!(f, "CHARACTERISTICS"),
            TokenType::SubPropertyOf => write!(f, "SUBPROPERTY_OF"),
            TokenType::InverseOf => write!(f, "INVERSE_OF"),
            TokenType::Types => write!(f, "TYPES"),
            TokenType::Facts => write!(f, "FACTS"),
            TokenType::SameAs => write!(f, "SAME_AS"),
            TokenType::DifferentFrom => write!(f, "DIFFERENT_FROM"),
            TokenType::Annotations => write!(f, "ANNOTATIONS"),
            TokenType::Rule => write!(f, "RULE"),
            TokenType::And => write!(f, "AND"),
            TokenType::Or => write!(f, "OR"),
            TokenType::Not => write!(f, "NOT"),
            TokenType::Some => write!(f, "SOME"),
            TokenType::Only => write!(f, "ONLY"),
            TokenType::Value => write!(f, "VALUE"),
            TokenType::SelfValue => write!(f, "SELF"),
            TokenType::Min => write!(f, "MIN"),
            TokenType::Max => write!(f, "MAX"),
            TokenType::Exactly => write!(f, "EXACTLY"),
            TokenType::Inverse => write!(f, "INVERSE"),
            TokenType::Functional => write!(f, "FUNCTIONAL"),
            TokenType::InverseFunctional => write!(f, "INVERSE_FUNCTIONAL"),
            TokenType::Transitive => write!(f, "TRANSITIVE"),
            TokenType::Symmetric => write!(f, "SYMMETRIC"),
            TokenType::Asymmetric => write!(f, "ASYMMETRIC"),
            TokenType::Reflexive => write!(f, "REFLEXIVE"),
            TokenType::Irreflexive => write!(f, "IRREFLEXIVE"),
            TokenType::Datatype => write!(f, "DATATYPE"),
            TokenType::Colon => write!(f, "COLON"),
            TokenType::Semicolon => write!(f, "SEMICOLON"),
            TokenType::Comma => write!(f, "COMMA"),
            TokenType::LeftParen => write!(f, "LEFT_PAREN"),
            TokenType::RightParen => write!(f, "RIGHT_PAREN"),
            TokenType::LeftBrace => write!(f, "LEFT_BRACE"),
            TokenType::RightBrace => write!(f, "RIGHT_BRACE"),
            TokenType::LeftBracket => write!(f, "LEFT_BRACKET"),
            TokenType::RightBracket => write!(f, "RIGHT_BRACKET"),
            TokenType::Equals => write!(f, "EQUALS"),
            TokenType::Underscore => write!(f, "UNDERSCORE"),
            TokenType::Identifier => write!(f, "IDENTIFIER"),
            TokenType::StringLiteral => write!(f, "STRING_LITERAL"),
            TokenType::NumberLiteral => write!(f, "NUMBER_LITERAL"),
            TokenType::IRI => write!(f, "IRI"),
            TokenType::EOF => write!(f, "EOF"),
            TokenType::Newline => write!(f, "NEWLINE"),
            TokenType::Comment => write!(f, "COMMENT"),
        }
    }
}

/// Manchester Syntax tokenizer
pub struct ManchesterTokenizer {
    /// The input text to tokenize
    input: String,

    /// Current position in the input
    current_pos: usize,

    /// Current line number
    current_line: usize,

    /// Current column number
    current_column: usize,

    /// Buffer for lookahead
    lookahead: Vec<Token>,
}

impl ManchesterTokenizer {
    /// Create a new tokenizer for the given input
    pub fn new(input: String) -> Self {
        ManchesterTokenizer {
            input,
            current_pos: 0,
            current_line: 1,
            current_column: 1,
            lookahead: Vec::new(),
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(&mut self) -> OwlResult<Vec<Token>> {
        let mut tokens = Vec::new();

        while let Some(token) = self.next_token()? {
            if token.token_type != TokenType::Comment {
                tokens.push(token);
            }
        }

        Ok(tokens)
    }

    /// Get the next token
    pub fn next_token(&mut self) -> OwlResult<Option<Token>> {
        if !self.lookahead.is_empty() {
            return Ok(Some(self.lookahead.remove(0)));
        }

        self.skip_whitespace();

        if self.current_pos >= self.input.len() {
            return Ok(Some(Token::eof(self.current_pos)));
        }

        let c = match self.peek_char() {
            Some(c) => c,
            None => return Ok(Some(Token::eof(self.current_pos))),
        };

        // Handle comments
        if c == '#' {
            self.consume_char();
            while let Some(next_c) = self.peek_char() {
                if next_c == '\n' {
                    break;
                }
                self.consume_char();
            }
            return self.next_token(); // Recursive call to get the next real token
        }

        // Handle string literals
        if c == '"' {
            return Ok(Some(self.tokenize_string_literal()?));
        }

        // Handle IRIs in angle brackets
        if c == '<' {
            return Ok(Some(self.tokenize_iri()?));
        }

        // Handle numbers
        if c.is_ascii_digit() {
            return Ok(Some(self.tokenize_number()?));
        }

        // Handle identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            return Ok(Some(self.tokenize_identifier_or_keyword()?));
        }

        // Handle symbols
        let token = match c {
            ':' => {
                self.consume_char();
                Token::new(
                    TokenType::Colon,
                    ":".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            ';' => {
                self.consume_char();
                Token::new(
                    TokenType::Semicolon,
                    ";".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            ',' => {
                self.consume_char();
                Token::new(
                    TokenType::Comma,
                    ",".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '(' => {
                self.consume_char();
                Token::new(
                    TokenType::LeftParen,
                    "(".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            ')' => {
                self.consume_char();
                Token::new(
                    TokenType::RightParen,
                    ")".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '{' => {
                self.consume_char();
                Token::new(
                    TokenType::LeftBrace,
                    "{".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '}' => {
                self.consume_char();
                Token::new(
                    TokenType::RightBrace,
                    "}".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '[' => {
                self.consume_char();
                Token::new(
                    TokenType::LeftBracket,
                    "[".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            ']' => {
                self.consume_char();
                Token::new(
                    TokenType::RightBracket,
                    "]".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '=' => {
                self.consume_char();
                Token::new(
                    TokenType::Equals,
                    "=".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '_' => {
                self.consume_char();
                Token::new(
                    TokenType::Underscore,
                    "_".to_string(),
                    self.current_line,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            '\n' => {
                self.consume_char();
                self.current_line += 1;
                self.current_column = 1;
                Token::new(
                    TokenType::Newline,
                    "\n".to_string(),
                    self.current_line - 1,
                    self.current_column,
                    self.current_pos - 1,
                )
            }
            _ => {
                return Err(OwlError::ParseError(format!(
                    "Unexpected character '{}' at line {}, column {}",
                    c, self.current_line, self.current_column
                )))
            }
        };

        Ok(Some(token))
    }

    /// Peek at the current character without consuming it
    fn peek_char(&self) -> Option<char> {
        self.input[self.current_pos..].chars().next()
    }

    /// Consume the current character
    fn consume_char(&mut self) {
        if let Some(c) = self.peek_char() {
            self.current_pos += c.len_utf8();
            self.current_column += 1;
        }
    }

    /// Skip whitespace
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() && c != '\n' {
                self.consume_char();
            } else {
                break;
            }
        }
    }

    /// Tokenize a string literal
    fn tokenize_string_literal(&mut self) -> OwlResult<Token> {
        let start_pos = self.current_pos;
        let start_line = self.current_line;
        let start_column = self.current_column;

        self.consume_char(); // Consume opening quote
        let mut literal = String::new();

        while let Some(c) = self.peek_char() {
            match c {
                '"' => {
                    self.consume_char(); // Consume closing quote
                    let lexeme = format!("\"{}\"", literal);
                    return Ok(Token::new(
                        TokenType::StringLiteral,
                        lexeme,
                        start_line,
                        start_column,
                        start_pos,
                    ));
                }
                '\\' => {
                    self.consume_char(); // Consume backslash
                    if let Some(escaped_char) = self.peek_char() {
                        literal.push(escaped_char);
                        self.consume_char();
                    }
                }
                '\n' => {
                    return Err(OwlError::ParseError(format!(
                        "Unterminated string literal at line {}, column {}",
                        start_line, start_column
                    )));
                }
                _ => {
                    literal.push(c);
                    self.consume_char();
                }
            }
        }

        Err(OwlError::ParseError(format!(
            "Unterminated string literal at line {}, column {}",
            start_line, start_column
        )))
    }

    /// Tokenize an IRI
    fn tokenize_iri(&mut self) -> OwlResult<Token> {
        let start_pos = self.current_pos;
        let start_line = self.current_line;
        let start_column = self.current_column;

        self.consume_char(); // Consume opening angle bracket
        let mut iri = String::new();

        while let Some(c) = self.peek_char() {
            match c {
                '>' => {
                    self.consume_char(); // Consume closing angle bracket
                    let lexeme = format!("<{}>", iri);
                    return Ok(Token::new(
                        TokenType::IRI,
                        lexeme,
                        start_line,
                        start_column,
                        start_pos,
                    ));
                }
                '\n' => {
                    return Err(OwlError::ParseError(format!(
                        "Unterminated IRI at line {}, column {}",
                        start_line, start_column
                    )));
                }
                _ => {
                    iri.push(c);
                    self.consume_char();
                }
            }
        }

        Err(OwlError::ParseError(format!(
            "Unterminated IRI at line {}, column {}",
            start_line, start_column
        )))
    }

    /// Tokenize a number
    fn tokenize_number(&mut self) -> OwlResult<Token> {
        let start_pos = self.current_pos;
        let start_line = self.current_line;
        let start_column = self.current_column;
        let mut number = String::new();

        // Parse integer part
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                number.push(c);
                self.consume_char();
            } else {
                break;
            }
        }

        // Parse fractional part
        if let Some('.') = self.peek_char() {
            number.push('.');
            self.consume_char();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    number.push(c);
                    self.consume_char();
                } else {
                    break;
                }
            }
        }

        Ok(Token::new(
            TokenType::NumberLiteral,
            number,
            start_line,
            start_column,
            start_pos,
        ))
    }

    /// Tokenize an identifier or keyword
    fn tokenize_identifier_or_keyword(&mut self) -> OwlResult<Token> {
        let start_pos = self.current_pos;
        let start_line = self.current_line;
        let start_column = self.current_column;
        let mut identifier = String::new();

        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                identifier.push(c);
                self.consume_char();
            } else {
                break;
            }
        }

        // Check if it's a keyword
        let token_type = match identifier.as_str() {
            "Prefix" => TokenType::Prefix,
            "Class" => TokenType::Class,
            "ObjectProperty" => TokenType::ObjectProperty,
            "DataProperty" => TokenType::DataProperty,
            "Individual" => TokenType::Individual,
            "SubClassOf" => TokenType::SubClassOf,
            "EquivalentTo" => TokenType::EquivalentTo,
            "DisjointWith" => TokenType::DisjointWith,
            "Domain" => TokenType::Domain,
            "Range" => TokenType::Range,
            "Characteristics" => TokenType::Characteristics,
            "SubPropertyOf" => TokenType::SubPropertyOf,
            "InverseOf" => TokenType::InverseOf,
            "Types" => TokenType::Types,
            "Facts" => TokenType::Facts,
            "SameAs" => TokenType::SameAs,
            "DifferentFrom" => TokenType::DifferentFrom,
            "Annotations" => TokenType::Annotations,
            "Rule" => TokenType::Rule,
            "and" => TokenType::And,
            "or" => TokenType::Or,
            "not" => TokenType::Not,
            "some" => TokenType::Some,
            "only" => TokenType::Only,
            "value" => TokenType::Value,
            "Self" => TokenType::SelfValue,
            "min" => TokenType::Min,
            "max" => TokenType::Max,
            "exactly" => TokenType::Exactly,
            "inverse" => TokenType::Inverse,
            "Functional" => TokenType::Functional,
            "InverseFunctional" => TokenType::InverseFunctional,
            "Transitive" => TokenType::Transitive,
            "Symmetric" => TokenType::Symmetric,
            "Asymmetric" => TokenType::Asymmetric,
            "Reflexive" => TokenType::Reflexive,
            "Irreflexive" => TokenType::Irreflexive,
            "Datatype" => TokenType::Datatype,
            _ => TokenType::Identifier,
        };

        Ok(Token::new(
            token_type,
            identifier,
            start_line,
            start_column,
            start_pos,
        ))
    }

    /// Look ahead at the next token without consuming it
    pub fn peek_token(&mut self) -> OwlResult<Option<&Token>> {
        if self.lookahead.is_empty() {
            if let Some(token) = self.next_token()? {
                self.lookahead.push(token);
            }
        }
        Ok(self.lookahead.first())
    }

    /// Get the current position
    pub fn position(&self) -> usize {
        self.current_pos
    }

    /// Get the current line
    pub fn line(&self) -> usize {
        self.current_line
    }

    /// Get the current column
    pub fn column(&self) -> usize {
        self.current_column
    }
}
