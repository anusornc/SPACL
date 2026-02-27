//! Syntax tree definitions for OWL Functional Syntax
//!
//! This module defines Abstract Syntax Tree (AST) structures for representing
//! OWL Functional Syntax documents during parsing.

use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::iri::IRI;

/// Top-level AST node for OWL Functional Syntax documents
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionalSyntaxAST {
    /// Ontology document with prefixes and content
    OntologyDocument {
        /// The ontology IRI
        ontology_iri: Option<String>,
        /// Prefix declarations
        prefixes: Vec<PrefixDeclaration>,
        /// Ontology content (axioms and declarations)
        content: Vec<OntologyContent>,
    },
}

/// Prefix declaration
#[derive(Debug, Clone, PartialEq)]
pub struct PrefixDeclaration {
    /// The prefix name
    pub prefix: String,
    /// The full namespace IRI
    pub namespace: String,
}

/// Content items within an ontology
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OntologyContent {
    /// Entity declaration
    Declaration(EntityDeclaration),
    /// Axiom
    Axiom(Axiom),
    /// Import declaration
    Import(ImportDeclaration),
}

/// Entity declaration
#[derive(Debug, Clone, PartialEq)]
pub enum EntityDeclaration {
    /// Class declaration
    Class(Class),
    /// Object property declaration
    ObjectProperty(ObjectProperty),
    /// Data property declaration
    DataProperty(DataProperty),
    /// Named individual declaration
    NamedIndividual(NamedIndividual),
    /// Anonymous individual declaration
    AnonymousIndividual(AnonymousIndividual),
    /// Annotation property declaration
    AnnotationProperty(AnnotationProperty),
}

/// Import declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDeclaration {
    /// The IRI of the ontology to import
    pub import_iri: IRI,
}

/// Parsed annotation
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAnnotation {
    /// The annotation property
    pub property: AnnotationProperty,
    /// The annotation value
    pub value: AnnotationValue,
}

/// Utility functions for AST construction and manipulation
impl FunctionalSyntaxAST {
    /// Create a new empty ontology document
    pub fn new() -> Self {
        Self::OntologyDocument {
            ontology_iri: None,
            prefixes: Vec::new(),
            content: Vec::new(),
        }
    }

    /// Add a prefix declaration to the document
    pub fn add_prefix(&mut self, prefix: String, namespace: String) {
        let Self::OntologyDocument { prefixes, .. } = self;
        prefixes.push(PrefixDeclaration { prefix, namespace });
    }

    /// Add content to the document
    pub fn add_content(&mut self, content: OntologyContent) {
        let Self::OntologyDocument {
            content: contents, ..
        } = self;
        contents.push(content);
    }

    /// Get all prefix declarations
    pub fn prefixes(&self) -> &[PrefixDeclaration] {
        match self {
            Self::OntologyDocument { prefixes, .. } => prefixes,
        }
    }

    /// Get all ontology content
    pub fn content(&self) -> &[OntologyContent] {
        match self {
            Self::OntologyDocument { content, .. } => content,
        }
    }
}

impl Default for FunctionalSyntaxAST {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefixDeclaration {
    /// Create a new prefix declaration
    pub fn new(prefix: String, namespace: String) -> Self {
        Self { prefix, namespace }
    }
}

impl ImportDeclaration {
    /// Create a new import declaration
    pub fn new(import_iri: IRI) -> Self {
        Self { import_iri }
    }
}

impl ParsedAnnotation {
    /// Create a new parsed annotation
    pub fn new(property: AnnotationProperty, value: AnnotationValue) -> Self {
        Self { property, value }
    }
}
