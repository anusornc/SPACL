//! Manchester Syntax Parser
//!
//! This module implements the main parsing logic for Manchester Syntax,
//! converting tokens into Abstract Syntax Trees (AST).

use super::error::{ErrorContext, ErrorReporter, ParseError, ParseResult};
use super::syntax::{
    Annotation, ClassExpression, DataPropertyExpression, DataRange, IndividualExpression,
    ManchesterAST, ObjectPropertyExpression, PropertyAssertion, PropertyCharacteristic,
};
use super::tokenizer::{ManchesterTokenizer, Token, TokenType};
use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::parser::{OntologyParser, ParserConfig};
use smallvec;
use smallvec::SmallVec;
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;

/// Manchester Syntax Parser
pub struct ManchesterParser {
    /// Parser configuration
    config: ParserConfig,
    /// The tokenizer for lexical analysis
    tokenizer: ManchesterTokenizer,

    /// Error context and reporter
    error_reporter: ErrorReporter,

    /// Current lookahead token
    current_token: Option<Token>,

    /// Parser state
    state: ParserState,
}

/// Parser state information
#[derive(Debug, Clone, Default)]
struct ParserState {
    /// Whether we're in a recovery mode
    recovery_mode: bool,

    /// Expected tokens for error reporting
    expected_tokens: Vec<String>,
}

impl ManchesterParser {
    /// Create a new parser with default configuration
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new parser for the given input
    pub fn with_input(input: String) -> Self {
        Self::with_input_and_config(input, ParserConfig::default())
    }

    fn with_input_and_config(input: String, config: ParserConfig) -> Self {
        let context = ErrorContext::new(input.clone());
        let error_reporter = ErrorReporter::new(context);
        let tokenizer = ManchesterTokenizer::new(input);

        ManchesterParser {
            config,
            tokenizer,
            error_reporter,
            current_token: None,
            state: ParserState::default(),
        }
    }

    /// Create a new parser with custom configuration
    pub fn with_config(config: ParserConfig) -> Self {
        Self::with_input_and_config(String::new(), config)
    }

    fn ast_to_ontology(ast: Vec<ManchesterAST>) -> Ontology {
        let mut ontology = Ontology::new();

        // Placeholder conversion. Keeps compatibility with current behavior.
        for node in ast {
            match node {
                ManchesterAST::ClassDeclaration { name, .. } => {
                    if let Ok(iri) = crate::core::iri::IRI::new(format!("http://example.org/{}", name)) {
                        let class = crate::core::entities::Class::new(Arc::new(iri));
                        if ontology.add_class(class).is_err() {
                            // Ignore duplicates in placeholder path.
                        }
                    }
                }
                _ => {}
            }
        }

        ontology
    }

    fn parse_owned_input(input: String, config: ParserConfig) -> OwlResult<Ontology> {
        let mut parser = ManchesterParser::with_input_and_config(input, config);
        let ast = parser
            .parse()
            .map_err(|e| crate::core::error::OwlError::ParseError(e.to_string()))?;
        Ok(Self::ast_to_ontology(ast))
    }

    /// Parse the entire input into an AST
    pub fn parse(&mut self) -> ParseResult<Vec<ManchesterAST>> {
        let mut ast_nodes = Vec::new();
        self.advance_token();

        while self
            .current_token
            .as_ref()
            .is_some_and(|t| t.token_type != TokenType::EOF)
        {
            match self.parse_declaration() {
                Ok(node) => ast_nodes.push(node),
                Err(error) => {
                    self.error_reporter.report_error(error);
                    if !self.error_reporter.should_continue() {
                        break;
                    }
                    self.recover_from_error();
                }
            }
        }

        if self.error_reporter.errors().has_errors() {
            Err(ParseError::syntax_error(
                "Parsing completed with errors",
                self.current_token.as_ref().map_or(0, |t| t.line),
                self.current_token.as_ref().map_or(0, |t| t.column),
            ))
        } else {
            Ok(ast_nodes)
        }
    }

    /// Parse a single declaration
    fn parse_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.state.expected_tokens.clear();

        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::Prefix => self.parse_prefix_declaration(),
                TokenType::Class => self.parse_class_declaration(),
                TokenType::ObjectProperty => self.parse_object_property_declaration(),
                TokenType::DataProperty => self.parse_data_property_declaration(),
                TokenType::Individual => self.parse_individual_declaration(),
                _ => {
                    self.state.expected_tokens.extend(vec![
                        "Prefix".to_string(),
                        "Class".to_string(),
                        "ObjectProperty".to_string(),
                        "DataProperty".to_string(),
                        "Individual".to_string(),
                    ]);
                    Err(ParseError::unexpected_token(
                        self.state.expected_tokens.clone(),
                        token.lexeme.clone(),
                        token.line,
                        token.column,
                    ))
                }
            }
        } else {
            Err(ParseError::syntax_error("Unexpected end of input", 0, 0))
        }
    }

    /// Parse a prefix declaration
    fn parse_prefix_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.expect_token(TokenType::Prefix)?;
        self.expect_token(TokenType::Colon)?;

        let prefix = self.expect_identifier()?;
        self.expect_token(TokenType::IRI)?;
        let iri = self
            .current_token
            .as_ref()
            .ok_or_else(|| ParseError::syntax_error("Expected IRI token but none found", 0, 0))?
            .lexeme
            .clone();
        self.advance_token();

        // Add prefix to context
        self.error_reporter
            .context_mut()
            .add_prefix(prefix.clone(), iri.clone());

        Ok(ManchesterAST::PrefixDeclaration { prefix, iri })
    }

    /// Parse a class declaration
    fn parse_class_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.expect_token(TokenType::Class)?;
        self.expect_token(TokenType::Colon)?;

        let name = self.expect_identifier()?;
        let mut sub_class_of: SmallVec<[Box<ClassExpression>; 4]> =
            SmallVec::new();
        let mut equivalent_to: SmallVec<[Box<ClassExpression>; 4]> =
            SmallVec::new();
        let mut disjoint_with: SmallVec<[Box<ClassExpression>; 4]> =
            SmallVec::new();
        let mut annotations: SmallVec<[Annotation; 2]> = SmallVec::new();

        while self.is_declaration_continuer() {
            if let Some(ref token) = self.current_token {
                match token.token_type {
                    TokenType::SubClassOf => {
                        self.advance_token();
                        sub_class_of.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::EquivalentTo => {
                        self.advance_token();
                        equivalent_to.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::DisjointWith => {
                        self.advance_token();
                        disjoint_with.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::Annotations => {
                        self.advance_token();
                        annotations.push(self.parse_annotation()?);
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        Ok(ManchesterAST::ClassDeclaration {
            name,
            sub_class_of,
            equivalent_to,
            disjoint_with,
            annotations,
        })
    }

    /// Parse an object property declaration
    fn parse_object_property_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.expect_token(TokenType::ObjectProperty)?;
        self.expect_token(TokenType::Colon)?;

        let name = self.expect_identifier()?;
        let mut domain: SmallVec<[Box<ClassExpression>; 4]> =
            SmallVec::new();
        let mut range: SmallVec<[Box<ClassExpression>; 4]> = SmallVec::new();
        let mut characteristics: SmallVec<[PropertyCharacteristic; 4]> = SmallVec::new();
        let mut sub_property_of: SmallVec<[ObjectPropertyExpression; 4]> =
            SmallVec::new();
        let mut equivalent_to: SmallVec<[ObjectPropertyExpression; 4]> =
            SmallVec::new();
        let mut disjoint_with: SmallVec<[ObjectPropertyExpression; 4]> =
            SmallVec::new();
        let mut inverse_of: SmallVec<[ObjectPropertyExpression; 4]> =
            SmallVec::new();
        let mut annotations: SmallVec<[Annotation; 2]> = SmallVec::new();

        while self.is_declaration_continuer() {
            if let Some(ref token) = self.current_token {
                match token.token_type {
                    TokenType::Domain => {
                        self.advance_token();
                        domain.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::Range => {
                        self.advance_token();
                        range.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::Characteristics => {
                        self.advance_token();
                        characteristics.push(self.parse_property_characteristic()?);
                    }
                    TokenType::SubPropertyOf => {
                        self.advance_token();
                        sub_property_of.push(self.parse_object_property_expression()?);
                    }
                    TokenType::EquivalentTo => {
                        self.advance_token();
                        equivalent_to.push(self.parse_object_property_expression()?);
                    }
                    TokenType::DisjointWith => {
                        self.advance_token();
                        disjoint_with.push(self.parse_object_property_expression()?);
                    }
                    TokenType::InverseOf => {
                        self.advance_token();
                        inverse_of.push(self.parse_object_property_expression()?);
                    }
                    TokenType::Annotations => {
                        self.advance_token();
                        annotations.push(self.parse_annotation()?);
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        Ok(ManchesterAST::ObjectPropertyDeclaration {
            name,
            domain: Box::new(domain),
            range: Box::new(range),
            characteristics: Box::new(characteristics),
            sub_property_of: Box::new(sub_property_of),
            equivalent_to: Box::new(equivalent_to),
            disjoint_with: Box::new(disjoint_with),
            inverse_of: Box::new(inverse_of),
            annotations: Box::new(annotations),
        })
    }

    /// Parse a data property declaration
    fn parse_data_property_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.expect_token(TokenType::DataProperty)?;
        self.expect_token(TokenType::Colon)?;

        let name = self.expect_identifier()?;
        let mut domain: SmallVec<[Box<ClassExpression>; 4]> =
            SmallVec::new();
        let mut range: SmallVec<[DataRange; 4]> = SmallVec::new();
        let mut characteristics: SmallVec<[PropertyCharacteristic; 4]> = SmallVec::new();
        let mut sub_property_of: SmallVec<[DataPropertyExpression; 4]> =
            SmallVec::new();
        let mut equivalent_to: SmallVec<[DataPropertyExpression; 4]> =
            SmallVec::new();
        let mut disjoint_with: SmallVec<[DataPropertyExpression; 4]> =
            SmallVec::new();
        let mut annotations: SmallVec<[Annotation; 2]> = SmallVec::new();

        while self.is_declaration_continuer() {
            if let Some(ref token) = self.current_token {
                match token.token_type {
                    TokenType::Domain => {
                        self.advance_token();
                        domain.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::Range => {
                        self.advance_token();
                        range.push(self.parse_data_range()?);
                    }
                    TokenType::Characteristics => {
                        self.advance_token();
                        characteristics.push(self.parse_property_characteristic()?);
                    }
                    TokenType::SubPropertyOf => {
                        self.advance_token();
                        sub_property_of.push(self.parse_data_property_expression()?);
                    }
                    TokenType::EquivalentTo => {
                        self.advance_token();
                        equivalent_to.push(self.parse_data_property_expression()?);
                    }
                    TokenType::DisjointWith => {
                        self.advance_token();
                        disjoint_with.push(self.parse_data_property_expression()?);
                    }
                    TokenType::Annotations => {
                        self.advance_token();
                        annotations.push(self.parse_annotation()?);
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        Ok(ManchesterAST::DataPropertyDeclaration {
            name,
            domain: Box::new(domain),
            range: Box::new(range),
            characteristics: Box::new(characteristics),
            sub_property_of: Box::new(sub_property_of),
            equivalent_to: Box::new(equivalent_to),
            disjoint_with: Box::new(disjoint_with),
            annotations: Box::new(annotations),
        })
    }

    /// Parse an individual declaration
    fn parse_individual_declaration(&mut self) -> ParseResult<ManchesterAST> {
        self.expect_token(TokenType::Individual)?;
        self.expect_token(TokenType::Colon)?;

        let name = self.expect_identifier()?;
        let mut types: SmallVec<[Box<ClassExpression>; 4]> = SmallVec::new();
        let mut facts: SmallVec<[PropertyAssertion; 8]> = SmallVec::new();
        let mut same_as: SmallVec<[IndividualExpression; 4]> = SmallVec::new();
        let mut different_from: SmallVec<[IndividualExpression; 4]> = SmallVec::new();
        let mut annotations: SmallVec<[Annotation; 2]> = SmallVec::new();

        while self.is_declaration_continuer() {
            if let Some(ref token) = self.current_token {
                match token.token_type {
                    TokenType::Types => {
                        self.advance_token();
                        types.push(Box::new(self.parse_class_expression()?));
                    }
                    TokenType::Facts => {
                        self.advance_token();
                        facts.push(self.parse_property_assertion()?);
                    }
                    TokenType::SameAs => {
                        self.advance_token();
                        same_as.push(IndividualExpression::NamedIndividual(
                            self.parse_individual_expression()?,
                        ));
                    }
                    TokenType::DifferentFrom => {
                        self.advance_token();
                        different_from.push(IndividualExpression::NamedIndividual(
                            self.parse_individual_expression()?,
                        ));
                    }
                    TokenType::Annotations => {
                        self.advance_token();
                        annotations.push(self.parse_annotation()?);
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }

        Ok(ManchesterAST::IndividualDeclaration {
            name,
            types: Box::new(types),
            facts: Box::new(facts),
            same_as: Box::new(same_as),
            different_from: Box::new(different_from),
            annotations: Box::new(annotations),
        })
    }

    /// Parse a class expression
    fn parse_class_expression(&mut self) -> ParseResult<ClassExpression> {
        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::LeftParen => self.parse_complex_class_expression(),
                TokenType::Identifier => Ok(ClassExpression::NamedClass(token.lexeme.clone())),
                _ => Err(ParseError::expected_class_expression(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                )),
            }
        } else {
            Err(ParseError::incomplete_expression("class", 0, 0))
        }
    }

    /// Parse a complex class expression (with parentheses)
    fn parse_complex_class_expression(&mut self) -> ParseResult<ClassExpression> {
        self.expect_token(TokenType::LeftParen)?;

        let token_type = self.current_token.as_ref().map(|t| t.token_type.clone());
        let token_lexeme = self.current_token.as_ref().map(|t| t.lexeme.clone());
        let token_line = self.current_token.as_ref().map(|t| t.line);
        let token_column = self.current_token.as_ref().map(|t| t.column);

        if let (Some(token_type), Some(token_lexeme)) = (token_type, token_lexeme) {
            let expr = match token_type {
                TokenType::Identifier => {
                    self.advance_token();
                    let identifier = token_lexeme;

                    match identifier.as_str() {
                        "and" => {
                            let mut operands: SmallVec<
                                [Box<ClassExpression>; 4],
                            > = SmallVec::new();
                            while self
                                .current_token
                                .as_ref()
                                .is_some_and(|t| t.token_type != TokenType::RightParen)
                            {
                                operands.push(Box::new(self.parse_class_expression()?));
                            }
                            ClassExpression::ObjectIntersection(operands)
                        }
                        "or" => {
                            let mut operands: SmallVec<
                                [Box<ClassExpression>; 4],
                            > = SmallVec::new();
                            while self
                                .current_token
                                .as_ref()
                                .is_some_and(|t| t.token_type != TokenType::RightParen)
                            {
                                operands.push(Box::new(self.parse_class_expression()?));
                            }
                            ClassExpression::ObjectUnion(operands)
                        }
                        "not" => {
                            let operand = self.parse_class_expression()?;
                            ClassExpression::ObjectComplement(Box::new(operand))
                        }
                        _ => {
                            return Err(ParseError::syntax_error(
                                format!("Unexpected operator: {}", identifier),
                                token_line.unwrap_or(0),
                                token_column.unwrap_or(0),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(ParseError::expected_class_expression(
                        token_lexeme,
                        token_line.unwrap_or(0),
                        token_column.unwrap_or(0),
                    ));
                }
            };

            self.expect_token(TokenType::RightParen)?;
            Ok(expr)
        } else {
            Err(ParseError::incomplete_expression("complex class", 0, 0))
        }
    }

    /// Parse an object property expression
    fn parse_object_property_expression(&mut self) -> ParseResult<ObjectPropertyExpression> {
        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::Identifier => {
                    let name = token.lexeme.clone();
                    self.advance_token();
                    Ok(ObjectPropertyExpression::NamedProperty(name))
                }
                TokenType::Inverse => {
                    self.advance_token();
                    self.expect_token(TokenType::LeftParen)?;
                    let prop = self.parse_object_property_expression()?;
                    self.expect_token(TokenType::RightParen)?;
                    Ok(ObjectPropertyExpression::InverseProperty(Box::new(prop)))
                }
                _ => Err(ParseError::expected_property_expression(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                )),
            }
        } else {
            Err(ParseError::incomplete_expression("object property", 0, 0))
        }
    }

    /// Parse a data property expression
    fn parse_data_property_expression(&mut self) -> ParseResult<DataPropertyExpression> {
        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::Identifier => {
                    let name = token.lexeme.clone();
                    self.advance_token();
                    Ok(DataPropertyExpression::NamedProperty(name))
                }
                _ => Err(ParseError::expected_property_expression(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                )),
            }
        } else {
            Err(ParseError::incomplete_expression("data property", 0, 0))
        }
    }

    /// Parse a data range
    fn parse_data_range(&mut self) -> ParseResult<DataRange> {
        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::Identifier => Ok(DataRange::Datatype(token.lexeme.clone())),
                TokenType::StringLiteral => {
                    let literal = token.lexeme.clone();
                    self.advance_token();
                    Ok(DataRange::DataOneOf(vec![literal]))
                }
                _ => Err(ParseError::expected_data_range(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                )),
            }
        } else {
            Err(ParseError::incomplete_expression("data range", 0, 0))
        }
    }

    /// Parse an individual expression
    fn parse_individual_expression(&mut self) -> ParseResult<String> {
        if let Some(ref token) = self.current_token {
            match token.token_type {
                TokenType::Identifier => {
                    let name = token.lexeme.clone();
                    self.advance_token();
                    Ok(name)
                }
                _ => Err(ParseError::expected_individual(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                )),
            }
        } else {
            Err(ParseError::incomplete_expression("individual", 0, 0))
        }
    }

    /// Parse a property assertion
    fn parse_property_assertion(&mut self) -> ParseResult<PropertyAssertion> {
        // Simplified implementation - would need more complex logic for full syntax
        if let Some(ref token) = self.current_token {
            if token.token_type == TokenType::Identifier {
                let property = ObjectPropertyExpression::NamedProperty(token.lexeme.clone());
                self.advance_token();
                let object = self.parse_individual_expression()?;
                Ok(PropertyAssertion::ObjectPropertyAssertion {
                    subject: "unknown".to_string(), // Would need context
                    property,
                    object,
                })
            } else {
                Err(ParseError::syntax_error(
                    "Expected property assertion",
                    token.line,
                    token.column,
                ))
            }
        } else {
            Err(ParseError::incomplete_expression(
                "property assertion",
                0,
                0,
            ))
        }
    }

    /// Parse a property characteristic
    fn parse_property_characteristic(&mut self) -> ParseResult<PropertyCharacteristic> {
        if let Some(ref token) = self.current_token {
            let characteristic = match token.token_type {
                TokenType::Functional => PropertyCharacteristic::Functional,
                TokenType::InverseFunctional => PropertyCharacteristic::InverseFunctional,
                TokenType::Transitive => PropertyCharacteristic::Transitive,
                TokenType::Symmetric => PropertyCharacteristic::Symmetric,
                TokenType::Asymmetric => PropertyCharacteristic::Asymmetric,
                TokenType::Reflexive => PropertyCharacteristic::Reflexive,
                TokenType::Irreflexive => PropertyCharacteristic::Irreflexive,
                _ => {
                    return Err(ParseError::invalid_property_characteristic(
                        token.lexeme.clone(),
                        token.line,
                        token.column,
                    ));
                }
            };
            self.advance_token();
            Ok(characteristic)
        } else {
            Err(ParseError::incomplete_expression(
                "property characteristic",
                0,
                0,
            ))
        }
    }

    /// Parse an annotation
    fn parse_annotation(&mut self) -> ParseResult<Annotation> {
        if let Some(ref token) = self.current_token {
            let property = token.lexeme.clone();
            self.advance_token();

            if let Some(ref value_token) = self.current_token {
                let value = match value_token.token_type {
                    TokenType::StringLiteral => {
                        super::syntax::AnnotationValue::Literal(value_token.lexeme.clone())
                    }
                    TokenType::IRI => {
                        super::syntax::AnnotationValue::IRI(value_token.lexeme.clone())
                    }
                    _ => super::syntax::AnnotationValue::Literal(value_token.lexeme.clone()),
                };
                self.advance_token();
                Ok(Annotation { property, value })
            } else {
                Err(ParseError::incomplete_expression("annotation", 0, 0))
            }
        } else {
            Err(ParseError::incomplete_expression("annotation", 0, 0))
        }
    }

    /// Check if current token continues a declaration
    fn is_declaration_continuer(&self) -> bool {
        if let Some(ref token) = self.current_token {
            matches!(
                token.token_type,
                TokenType::SubClassOf
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
            )
        } else {
            false
        }
    }

    /// Advance to the next token
    fn advance_token(&mut self) {
        self.current_token = match self.tokenizer.next_token() {
            Ok(Some(token)) => Some(token),
            Ok(None) => None,
            Err(e) => {
                self.error_reporter.report_error(ParseError::syntax_error(
                    e.to_string(),
                    self.tokenizer.line(),
                    self.tokenizer.column(),
                ));
                None
            }
        };
    }

    /// Expect a specific token type
    fn expect_token(&mut self, expected: TokenType) -> ParseResult<()> {
        if let Some(ref token) = self.current_token {
            if token.token_type == expected {
                self.advance_token();
                Ok(())
            } else {
                self.state.expected_tokens.push(format!("{:?}", expected));
                Err(ParseError::unexpected_token(
                    vec![format!("{:?}", expected)],
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                ))
            }
        } else {
            Err(ParseError::syntax_error(
                format!("Expected {:?}, but found end of input", expected),
                0,
                0,
            ))
        }
    }

    /// Expect an identifier
    fn expect_identifier(&mut self) -> ParseResult<String> {
        if let Some(ref token) = self.current_token {
            if token.token_type == TokenType::Identifier {
                let identifier = token.lexeme.clone();
                self.advance_token();
                Ok(identifier)
            } else {
                Err(ParseError::expected_identifier(
                    token.lexeme.clone(),
                    token.line,
                    token.column,
                ))
            }
        } else {
            Err(ParseError::syntax_error(
                "Expected identifier, but found end of input",
                0,
                0,
            ))
        }
    }

    /// Recover from parsing error
    fn recover_from_error(&mut self) {
        // Simple recovery: advance to next declaration or synchronization point
        while let Some(ref token) = self.current_token {
            if matches!(
                token.token_type,
                TokenType::Prefix
                    | TokenType::Class
                    | TokenType::ObjectProperty
                    | TokenType::DataProperty
                    | TokenType::Individual
                    | TokenType::EOF
            ) {
                break;
            }
            self.advance_token();
        }
        self.state.recovery_mode = false;
    }

    /// Get the error reporter
    pub fn error_reporter(&self) -> &ErrorReporter {
        &self.error_reporter
    }

    /// Get mutable error reporter
    pub fn error_reporter_mut(&mut self) -> &mut ErrorReporter {
        &mut self.error_reporter
    }
}

impl OntologyParser for ManchesterParser {
    fn parse_str(&self, content: &str) -> OwlResult<Ontology> {
        Self::parse_owned_input(content.to_string(), self.config.clone())
    }

    fn parse_file(&self, path: &Path) -> OwlResult<Ontology> {
        if self.config.max_file_size > 0 {
            let metadata = fs::metadata(path)?;
            if metadata.len() > self.config.max_file_size as u64 {
                return Err(crate::core::error::OwlError::ParseError(format!(
                    "File size exceeds maximum allowed size: {} bytes",
                    self.config.max_file_size
                )));
            }
        }

        let file = fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        let capacity = fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let mut content = String::with_capacity(capacity.saturating_add(1));
        reader.read_to_string(&mut content)?;

        Self::parse_owned_input(content, self.config.clone())
    }

    fn format_name(&self) -> &'static str {
        "Manchester Syntax"
    }
}

impl Default for ManchesterParser {
    fn default() -> Self {
        Self::new()
    }
}
