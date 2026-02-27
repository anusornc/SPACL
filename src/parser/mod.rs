//! Parser module for OWL2 ontology formats
//!
//! Provides parsers for various RDF/OWL serialization formats including:
//! - Turtle (TTL)
//! - RDF/XML
//! - OWL/XML
//! - N-Triples
//! - JSON-LD

pub mod arena;
pub mod common;
pub mod import_resolver;
pub mod json_ld;
pub mod manchester;
pub mod owl_functional;
pub mod owl_xml;
pub mod rdf_xml;
pub mod rdf_xml_common;
pub mod rdf_xml_legacy;
pub mod rdf_xml_streaming;
pub mod restriction_parser;
// pub mod streaming;  // WIP: has borrow checker issues
pub mod turtle;

pub use arena::*;
pub use common::*;
pub use import_resolver::*;
pub use json_ld::JsonLdParser;
pub use manchester::{ManchesterAST, ManchesterParser};
pub use owl_functional::OwlFunctionalSyntaxParser;
pub use owl_xml::*;
pub use rdf_xml::*;
pub use turtle::*;

use crate::core::entities::Class;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml_common::{RDF_TYPE, RDFS_SUBCLASSOF};
use rio_api::model::{Literal as RioLiteral, Subject as RioSubject, Term as RioTerm};
use rio_turtle::TurtleError;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Parser trait for different serialization formats
pub trait OntologyParser {
    /// Parse an ontology from a string
    fn parse_str(&self, content: &str) -> OwlResult<Ontology>;

    /// Parse an ontology from a file
    fn parse_file(&self, path: &std::path::Path) -> OwlResult<Ontology>;

    /// Get the supported format name
    fn format_name(&self) -> &'static str;
}

/// Factory for creating parsers based on file extension or content type
pub struct ParserFactory;

impl ParserFactory {
    fn env_truthy(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn config_from_env() -> ParserConfig {
        let mut config = ParserConfig::default();

        if Self::env_truthy("OWL2_REASONER_LARGE_PARSE") {
            config.max_file_size = 0;
            config.use_arena_allocation = true;
            config.arena_capacity = 16 * 1024 * 1024;
            config.max_arena_size = 256 * 1024 * 1024;
        }

        if let Ok(value) = std::env::var("OWL2_REASONER_MAX_FILE_SIZE") {
            if let Ok(bytes) = value.trim().parse::<usize>() {
                config.max_file_size = bytes;
            }
        }

        config
    }

    /// Create a parser based on file extension
    pub fn for_file_extension(ext: &str) -> Option<Box<dyn OntologyParser>> {
        let config = Self::config_from_env();
        match ext.to_lowercase().as_str() {
            "ttl" | "turtle" => Some(Box::new(TurtleParser::with_config(config))),
            "rdf" | "rdfs" => Some(Box::new(RdfXmlParser::with_config(config))),
            "owl" | "ofn" => {
                Some(Box::new(OwlFunctionalSyntaxParser::with_config(config)))
            }
            "owx" | "xml" => Some(Box::new(OwlXmlParser::with_config(config))),
            "nt" => Some(Box::new(NtriplesParser::new())),
            "jsonld" | "json-ld" | "json" => Some(Box::new(JsonLdParser::with_config(config))),
            "man" | "mn" | "manchester" => Some(Box::new(ManchesterParser::with_config(config))),
            _ => None,
        }
    }

    /// Create a parser based on content type
    pub fn for_content_type(content_type: &str) -> Option<Box<dyn OntologyParser>> {
        let config = Self::config_from_env();
        match content_type {
            "text/turtle" | "application/x-turtle" => {
                Some(Box::new(TurtleParser::with_config(config)))
            }
            "application/rdf+xml" => Some(Box::new(RdfXmlParser::with_config(config))),
            "application/owl+xml" => Some(Box::new(OwlXmlParser::with_config(config))),
            "application/n-triples" | "text/plain" => Some(Box::new(NtriplesParser::new())),
            "application/ld+json" | "application/json" => {
                Some(Box::new(JsonLdParser::with_config(config)))
            }
            "text/manchester" | "application/manchester" => {
                Some(Box::new(ManchesterParser::with_config(config)))
            }
            _ => None,
        }
    }

    /// Auto-detect format and create appropriate parser
    pub fn auto_detect(content: &str) -> Option<Box<dyn OntologyParser>> {
        let config = Self::config_from_env();
        let content_trimmed = content.trim();

        // Check for JSON-LD (highest priority due to distinct format)
        if (content_trimmed.starts_with('{') && content_trimmed.ends_with('}'))
            || content_trimmed.contains("@context")
            || content_trimmed.contains("@graph")
            || (content_trimmed.starts_with('{') && content_trimmed.contains("\"@id\""))
        {
            Some(Box::new(JsonLdParser::with_config(config.clone())))
        }
        // Check for XML-based formats first (RDF/XML, OWL/XML) to avoid false positives
        else if content_trimmed.starts_with("<?xml") || content_trimmed.starts_with("<rdf:RDF") {
            // RDF/XML has rdf:RDF root and uses XML namespaces
            if content.contains("<rdf:RDF") || content.contains("<rdf:Description") {
                Some(Box::new(RdfXmlParser::with_config(config.clone())))
            } else if content.contains("<Ontology") || content.contains("<owl:Ontology") {
                // Could be OWL/XML or RDF/XML with Ontology element
                Some(Box::new(RdfXmlParser::with_config(config.clone())))
            } else {
                Some(Box::new(OwlXmlParser::with_config(config.clone())))
            }
        } else if content_trimmed.starts_with("<rdf:Description") {
            Some(Box::new(RdfXmlParser::with_config(config.clone())))
        }
        // Check for Manchester Syntax (high priority for readability)
        else if content_trimmed.starts_with("Prefix:")
            || content_trimmed.contains("Class:")
            || content_trimmed.contains("ObjectProperty:")
            || content_trimmed.contains("Individual:")
        {
            Some(Box::new(ManchesterParser::with_config(config.clone())))
        }
        // Check for OWL Functional Syntax - must NOT be XML, starts with Prefix(
        else if content_trimmed.starts_with("Prefix(")
            || (content_trimmed.starts_with("Ontology(") && !content_trimmed.starts_with("<"))
            || (content_trimmed.starts_with("Document(") && content_trimmed.contains("Prefix("))
        {
            Some(Box::new(OwlFunctionalSyntaxParser::with_config(config.clone())))
        } else if content_trimmed.starts_with("@prefix") || content_trimmed.starts_with("PREFIX") {
            Some(Box::new(TurtleParser::with_config(config)))
        } else if content
            .lines()
            .next()
            .is_some_and(|line| line.contains("> <") && line.contains(" ."))
        {
            Some(Box::new(NtriplesParser::new()))
        } else {
            None
        }
    }
}

/// N-Triples parser implementing W3C N-Triples specification
pub struct NtriplesParser {
    #[allow(dead_code)]
    config: ParserConfig,
}

impl Default for NtriplesParser {
    fn default() -> Self {
        Self::new()
    }
}

impl NtriplesParser {
    /// Creates a new N-Triples parser with default configuration.
    ///
    /// # Returns
    /// Returns a new `NtriplesParser` instance with default settings.
    pub fn new() -> Self {
        Self {
            config: ParserConfig::default(),
        }
    }

    /// Creates a new N-Triples parser with custom configuration.
    ///
    /// # Parameters
    /// - `config`: The parser configuration to use
    ///
    /// # Returns
    /// Returns a new `NtriplesParser` instance with the specified configuration.
    pub fn with_config(config: ParserConfig) -> Self {
        Self { config }
    }
}

impl OntologyParser for NtriplesParser {
    fn parse_str(&self, content: &str) -> OwlResult<Ontology> {
        let mut ontology = Ontology::new();
        let mut line_num = 0;

        for line in content.lines() {
            line_num += 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match self.parse_ntriples_line(line) {
                Ok(triple) => {
                    if let Err(e) = self.add_triple_to_ontology(&mut ontology, &triple) {
                        return Err(crate::core::error::OwlError::ParseError(format!(
                            "Error at line {}: {}",
                            line_num, e
                        )));
                    }
                }
                Err(e) => {
                    return Err(crate::core::error::OwlError::ParseError(format!(
                        "Parse error at line {}: {}",
                        line_num, e
                    )));
                }
            }
        }

        Ok(ontology)
    }

    fn parse_file(&self, path: &std::path::Path) -> OwlResult<Ontology> {
        if Self::should_use_streaming(path) {
            return self.parse_file_streaming(path);
        }

        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        self.parse_str(&content)
    }

    fn format_name(&self) -> &'static str {
        "N-Triples"
    }
}

impl NtriplesParser {
    fn env_truthy(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn should_use_streaming(path: &std::path::Path) -> bool {
        if Self::env_truthy("OWL2_REASONER_LARGE_PARSE") {
            return true;
        }
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.len() > 32 * 1024 * 1024;
        }
        false
    }

    fn parse_file_streaming(&self, path: &std::path::Path) -> OwlResult<Ontology> {
        use rio_api::parser::TriplesParser;
        use rio_turtle::NTriplesParser as RioNTriplesParser;
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;
        let reader = BufReader::new(file);
        let mut parser = RioNTriplesParser::new(reader);

        let mut ontology = Ontology::new();
        let mut handler = |triple: rio_api::model::Triple| -> Result<(), TurtleError> {
            let subject = Self::rio_subject_to_term(&triple.subject)?;
            let predicate = NtriplesTerm::IRI(
                IRI::new(triple.predicate.iri).map_err(|e| {
                    let io_err = std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{:?}", e),
                    );
                    TurtleError::from(io_err)
                })?,
            );
            let object = Self::rio_term_to_term(&triple.object)?;
            let ntriple = NtriplesTriple {
                subject,
                predicate,
                object,
            };
            self.add_triple_to_ontology(&mut ontology, &ntriple)
                .map_err(|err| {
                    let io_err = std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{:?}", err),
                    );
                    TurtleError::from(io_err)
                })
        };

        parser.parse_all(&mut handler).map_err(|err| {
            crate::core::error::OwlError::ParseError(format!("N-Triples parse error: {}", err))
        })?;

        Ok(ontology)
    }

    fn rio_subject_to_term(subject: &RioSubject) -> Result<NtriplesTerm, TurtleError> {
        match subject {
            RioSubject::NamedNode(node) => Ok(NtriplesTerm::IRI(
                IRI::new(node.iri).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                })?,
            )),
            RioSubject::BlankNode(node) => Ok(NtriplesTerm::BlankNode(node.id.to_string())),
            RioSubject::Triple(triple) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                Self::rio_subject_key(&triple.subject).hash(&mut hasher);
                triple.predicate.iri.hash(&mut hasher);
                Self::rio_term_key(&triple.object).hash(&mut hasher);
                let id = format!("_:reified_{}", hasher.finish());
                Ok(NtriplesTerm::IRI(
                    IRI::new(id).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                    })?,
                ))
            }
        }
    }

    fn rio_term_to_term(term: &RioTerm) -> Result<NtriplesTerm, TurtleError> {
        match term {
            RioTerm::NamedNode(node) => Ok(NtriplesTerm::IRI(
                IRI::new(node.iri).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                })?,
            )),
            RioTerm::BlankNode(node) => Ok(NtriplesTerm::BlankNode(node.id.to_string())),
            RioTerm::Literal(literal) => Ok(match literal {
                RioLiteral::Simple { value } => NtriplesTerm::Literal {
                    value: (*value).to_string(),
                    language: None,
                    datatype: None,
                },
                RioLiteral::LanguageTaggedString { value, language } => NtriplesTerm::Literal {
                    value: (*value).to_string(),
                    language: Some((*language).to_string()),
                    datatype: None,
                },
                RioLiteral::Typed { value, datatype } => NtriplesTerm::Literal {
                    value: (*value).to_string(),
                    language: None,
                    datatype: Some(
                        IRI::new(datatype.iri).map_err(|e| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("{:?}", e),
                            )
                        })?,
                    ),
                },
            }),
            RioTerm::Triple(triple) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                Self::rio_subject_key(&triple.subject).hash(&mut hasher);
                triple.predicate.iri.hash(&mut hasher);
                Self::rio_term_key(&triple.object).hash(&mut hasher);
                let id = format!("_:reified_{}", hasher.finish());
                Ok(NtriplesTerm::IRI(
                    IRI::new(id).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                    })?,
                ))
            }
        }
    }

    fn rio_subject_key(subject: &RioSubject) -> String {
        match subject {
            RioSubject::NamedNode(node) => node.iri.to_string(),
            RioSubject::BlankNode(node) => format!("_:{:?}", node.id),
            RioSubject::Triple(triple) => format!(
                "triple:{}:{}:{}",
                Self::rio_subject_key(&triple.subject),
                triple.predicate.iri,
                Self::rio_term_key(&triple.object)
            ),
        }
    }

    fn rio_term_key(term: &RioTerm) -> String {
        match term {
            RioTerm::NamedNode(node) => node.iri.to_string(),
            RioTerm::BlankNode(node) => format!("_:{:?}", node.id),
            RioTerm::Literal(literal) => match literal {
                RioLiteral::Simple { value } => format!("\"{}\"", value),
                RioLiteral::LanguageTaggedString { value, language } => {
                    format!("\"{}\"@{}", value, language)
                }
                RioLiteral::Typed { value, datatype } => {
                    format!("\"{}\"^^{}", value, datatype.iri)
                }
            },
            RioTerm::Triple(triple) => format!(
                "triple:{}:{}:{}",
                Self::rio_subject_key(&triple.subject),
                triple.predicate.iri,
                Self::rio_term_key(&triple.object)
            ),
        }
    }
}

impl NtriplesParser {
    /// Parses a single N-Triples line into a triple.
    ///
    /// This method parses a line of N-Triples format according to the W3C specification,
    /// extracting the subject, predicate, and object terms.
    ///
    /// # Parameters
    /// - `line`: The N-Triples line to parse (without comments or whitespace)
    ///
    /// # Returns
    /// Returns an `OwlResult` containing the parsed triple or an error.
    ///
    /// # Errors
    /// Returns an error if the line is malformed or doesn't follow N-Triples syntax.
    fn parse_ntriples_line(&self, line: &str) -> OwlResult<NtriplesTriple> {
        let mut chars = line.char_indices();

        // Parse subject
        let subject = self.parse_ntriples_term(&mut chars)?;

        // Skip whitespace
        self.skip_whitespace(&mut chars);

        // Parse predicate
        let predicate = self.parse_ntriples_term(&mut chars)?;

        // Skip whitespace
        self.skip_whitespace(&mut chars);

        // Parse object
        let object = self.parse_ntriples_term(&mut chars)?;

        // Skip whitespace
        self.skip_whitespace(&mut chars);

        // Expect trailing '.'
        if let Some((_, c)) = chars.next() {
            if c != '.' {
                return Err(crate::core::error::OwlError::ParseError(
                    "Expected '.' at end of triple".to_string(),
                ));
            }
        }

        Ok(NtriplesTriple {
            subject,
            predicate,
            object,
        })
    }

    /// Parses an N-Triples term (IRI, literal, or blank node) from the character iterator.
    ///
    /// This method parses individual terms according to N-Triples syntax rules,
    /// handling IRIs in angle brackets, literals in quotes, and blank nodes with _: prefixes.
    ///
    /// # Parameters
    /// - `chars`: A mutable reference to the character iterator positioned at the start of the term
    ///
    /// # Returns
    /// Returns an `OwlResult` containing the parsed term or an error.
    ///
    /// # Errors
    /// Returns an error if the term is malformed or incomplete.
    fn parse_ntriples_term(
        &self,
        chars: &mut std::str::CharIndices<'_>,
    ) -> OwlResult<NtriplesTerm> {
        self.skip_whitespace(chars);

        if let Some((_, c)) = chars.next() {
            match c {
                '<' => {
                    // IRI
                    let mut iri_str = String::new();
                    for (_, next_c) in chars.by_ref() {
                        if next_c == '>' {
                            break;
                        }
                        iri_str.push(next_c);
                    }

                    if iri_str.is_empty() {
                        return Err(crate::core::error::OwlError::ParseError("Empty IRI".to_string()));
                    }

                    // Validate and create IRI
                    let iri = IRI::new(&iri_str).map_err(|e| {
                        crate::core::error::OwlError::ParseError(format!(
                            "Invalid IRI '{}': {}",
                            iri_str, e
                        ))
                    })?;

                    Ok(NtriplesTerm::IRI(iri))
                }
                '"' => {
                    // Literal
                    let mut literal_str = String::new();
                    let mut lang_tag = None;
                    let mut datatype = None;

                    // Parse literal content
                    while let Some((_, next_c)) = chars.next() {
                        if next_c == '"' {
                            break;
                        }
                        if next_c == '\\' {
                            if let Some((_, esc_c)) = chars.next() {
                                match esc_c {
                                    't' => literal_str.push('\t'),
                                    'b' => literal_str.push('\x08'),
                                    'n' => literal_str.push('\n'),
                                    'r' => literal_str.push('\r'),
                                    'f' => literal_str.push('\x0c'),
                                    '"' => literal_str.push('"'),
                                    '\'' => literal_str.push('\''),
                                    '\\' => literal_str.push('\\'),
                                    'u' => {
                                        // Unicode escape \uXXXX
                                        let mut hex = String::new();
                                        for _ in 0..4 {
                                            if let Some((_, h)) = chars.next() {
                                                hex.push(h);
                                            }
                                        }
                                        if let Ok(code) = u16::from_str_radix(&hex, 16) {
                                            literal_str
                                                .push(char::from_u32(code as u32).unwrap_or('?'));
                                        }
                                    }
                                    'U' => {
                                        // Unicode escape \UXXXXXXXX
                                        let mut hex = String::new();
                                        for _ in 0..8 {
                                            if let Some((_, h)) = chars.next() {
                                                hex.push(h);
                                            }
                                        }
                                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                            literal_str.push(char::from_u32(code).unwrap_or('?'));
                                        }
                                    }
                                    _ => literal_str.push(esc_c),
                                }
                            }
                        } else {
                            literal_str.push(next_c);
                        }
                    }

                    // Check for language tag or datatype
                    self.skip_whitespace(chars);
                    if let Some((_, next_c)) = chars.clone().next() {
                        if next_c == '@' {
                            // Language tag
                            chars.next(); // consume '@'
                            let mut tag = String::new();
                            while let Some((_, c)) = chars.clone().next() {
                                if c.is_alphanumeric() || c == '-' {
                                    tag.push(c);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            if !tag.is_empty() {
                                lang_tag = Some(tag);
                            }
                        } else if next_c == '^' {
                            // Datatype
                            chars.next(); // consume '^'
                            if let Some((_, c)) = chars.next() {
                                if c == '^' {
                                    chars.next(); // consume second '^'
                                    if let Some((_, c2)) = chars.next() {
                                        if c2 == '<' {
                                            let mut dt_iri = String::new();
                                            for (_, dt_c) in chars.by_ref() {
                                                if dt_c == '>' {
                                                    break;
                                                }
                                                dt_iri.push(dt_c);
                                            }
                                            if !dt_iri.is_empty() {
                                                datatype =
                                                    Some(IRI::new(&dt_iri).map_err(|e| {
                                                        crate::core::error::OwlError::ParseError(format!(
                                                            "Invalid datatype IRI '{}': {}",
                                                            dt_iri, e
                                                        ))
                                                    })?);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Ok(NtriplesTerm::Literal {
                        value: literal_str,
                        language: lang_tag,
                        datatype,
                    })
                }
                '_' => {
                    // Blank node
                    if let Some((_, c)) = chars.next() {
                        if c == ':' {
                            let mut bnode_id = String::new();
                            while let Some((_, next_c)) = chars.clone().next() {
                                if next_c.is_alphanumeric() || next_c == '-' || next_c == '_' {
                                    bnode_id.push(next_c);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            if bnode_id.is_empty() {
                                return Err(crate::core::error::OwlError::ParseError(
                                    "Empty blank node ID".to_string(),
                                ));
                            }
                            Ok(NtriplesTerm::BlankNode(bnode_id))
                        } else {
                            Err(crate::core::error::OwlError::ParseError(
                                "Expected ':' after '_' for blank node".to_string(),
                            ))
                        }
                    } else {
                        Err(crate::core::error::OwlError::ParseError(
                            "Incomplete blank node".to_string(),
                        ))
                    }
                }
                _ => Err(crate::core::error::OwlError::ParseError(format!(
                    "Unexpected character '{}' at start of term",
                    c
                ))),
            }
        } else {
            Err(crate::core::error::OwlError::ParseError(
                "Unexpected end of input while parsing term".to_string(),
            ))
        }
    }

    /// Skips whitespace characters in the character iterator.
    ///
    /// This method advances the iterator past any consecutive whitespace characters,
    /// positioning it at the next non-whitespace character or end of input.
    ///
    /// # Parameters
    /// - `chars`: A mutable reference to the character iterator to advance
    fn skip_whitespace(&self, chars: &mut std::str::CharIndices<'_>) {
        while let Some((_, c)) = chars.clone().next() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
    }

    /// Adds an N-Triples triple to the ontology by converting it to appropriate OWL axioms.
    ///
    /// This method interprets common RDF patterns and converts them to OWL axioms such as
    /// class assertions, subclass relationships, and property assertions.
    ///
    /// # Parameters
    /// - `ontology`: A mutable reference to the ontology to add axioms to
    /// - `triple`: The N-Triples triple to convert and add
    ///
    /// # Returns
    /// Returns an `OwlResult` indicating success or failure.
    ///
    /// # Errors
    /// Returns an error if adding axioms to the ontology fails.
    fn add_triple_to_ontology(
        &self,
        ontology: &mut Ontology,
        triple: &NtriplesTriple,
    ) -> OwlResult<()> {
        

        // Convert N-Triples triple to OWL axioms based on common patterns
        match (&triple.subject, &triple.predicate, &triple.object) {
            (
                NtriplesTerm::IRI(subject_iri),
                NtriplesTerm::IRI(predicate_iri),
                NtriplesTerm::IRI(object_iri),
            ) => {
                // Handle common RDF/RDFS/OWL patterns
                if predicate_iri.as_str() == RDF_TYPE {
                    // Class assertion: subject rdf:type object
                    let subject_class = Class::new(subject_iri.clone());
                    let object_class = Class::new(object_iri.clone());

                    ontology.add_class(subject_class.clone())?;
                    ontology.add_class(object_class.clone())?;

                    let class_assertion = crate::logic::axioms::ClassAssertionAxiom::new(
                        Arc::new(subject_iri.clone()),
                        crate::logic::axioms::ClassExpression::Class(subject_class),
                    );
                    ontology.add_class_assertion(class_assertion)?;
                } else if predicate_iri.as_str() == RDFS_SUBCLASSOF {
                    // Subclass axiom: subject rdfs:subClassOf object
                    let subject_class = Class::new(subject_iri.clone());
                    let object_class = Class::new(object_iri.clone());

                    ontology.add_class(subject_class.clone())?;
                    ontology.add_class(object_class.clone())?;

                    let subclass_axiom = crate::logic::axioms::SubClassOfAxiom::new(
                        crate::logic::axioms::ClassExpression::Class(subject_class),
                        crate::logic::axioms::ClassExpression::Class(object_class),
                    );
                    ontology.add_subclass_axiom(subclass_axiom)?;
                } else {
                    // Generic property assertion
                    let subject_individual =
                        crate::core::entities::NamedIndividual::new(subject_iri.clone());
                    ontology.add_named_individual(subject_individual)?;

                    // Create object property if it looks like one
                    if predicate_iri.as_str().starts_with("http://")
                        && !predicate_iri.as_str().contains("#")
                    {
                        let obj_prop = crate::core::entities::ObjectProperty::new(predicate_iri.clone());
                        ontology.add_object_property(obj_prop)?;
                    }
                }
            }
            (
                NtriplesTerm::IRI(subject_iri),
                NtriplesTerm::IRI(predicate_iri),
                NtriplesTerm::Literal {
                    value,
                    language: _,
                    datatype: _,
                },
            ) => {
                // Literal property assertion
                let subject_individual = crate::core::entities::NamedIndividual::new(subject_iri.clone());
                ontology.add_named_individual(subject_individual)?;

                // Could add data property assertion here in the future
                // For now, we'll just note that we've seen this pattern
                log::debug!(
                    "Skipping literal property assertion: {} {} {}",
                    subject_iri,
                    predicate_iri,
                    value
                );
            }
            _ => {
                // Other patterns (blank nodes, etc.) not yet implemented
                log::debug!(
                    "Skipping triple with unsupported pattern: {:?} {:?} {:?}",
                    triple.subject,
                    triple.predicate,
                    triple.object
                );
            }
        }

        Ok(())
    }
}

/// N-Triples term types
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
enum NtriplesTerm {
    IRI(IRI),
    Literal {
        value: String,
        language: Option<String>,
        datatype: Option<IRI>,
    },
    BlankNode(String),
}

/// N-Triples triple
#[derive(Debug, Clone, PartialEq)]
struct NtriplesTriple {
    subject: NtriplesTerm,
    predicate: NtriplesTerm,
    object: NtriplesTerm,
}

/// Parser configuration
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Maximum file size to parse (in bytes)
    pub max_file_size: usize,
    /// Whether to validate strict syntax
    pub strict_validation: bool,
    /// Whether to resolve base IRIs
    pub resolve_base_iri: bool,
    /// Custom prefix mappings
    pub prefixes: std::collections::HashMap<String, String>,
    /// Whether to use arena allocation for parsing
    pub use_arena_allocation: bool,
    /// Initial arena capacity in bytes (if arena allocation is enabled)
    pub arena_capacity: usize,
    /// Maximum arena size in bytes (if arena allocation is enabled)
    pub max_arena_size: usize,
    /// Whether to automatically resolve imports during parsing
    pub resolve_imports: bool,
    /// Whether to follow import resolution errors or continue without imports
    pub ignore_import_errors: bool,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024, // 100MB
            // Default to non-strict validation to use the modern rio-xml parser
            strict_validation: false,
            resolve_base_iri: false,
            prefixes: std::collections::HashMap::new(),
            // Enable arena allocation by default for better performance
            use_arena_allocation: true,
            // Start with 1MB arena capacity
            arena_capacity: 1024 * 1024,
            // Maximum arena size of 10MB
            max_arena_size: 10 * 1024 * 1024,
            // Default to not resolving imports automatically during parsing
            resolve_imports: false,
            // Default to ignoring import errors to allow parsing to continue
            ignore_import_errors: true,
        }
    }
}
