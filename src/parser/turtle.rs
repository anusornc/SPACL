//! Turtle/TTL format parser for OWL2 ontologies
//!
//! Implements parsing of the Terse RDF Triple Language format.
#![allow(dead_code)]

use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::{OntologyParser, ParserArenaBuilder, ParserArenaTrait, ParserConfig};
use hashbrown::HashMap;
use smallvec::SmallVec;
use std::path::Path;
use std::sync::Arc;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rio_api::model::{Literal as RioLiteral, Subject as RioSubject, Term as RioTerm, Triple as RioTriple};
use rio_api::parser::TriplesParser;
use rio_turtle::{TurtleError, TurtleParser as RioTurtleParser};

/// Static string constants to avoid allocations
static PREFIX_OWL: &str = "owl";
static PREFIX_RDF: &str = "rdf";
static PREFIX_RDFS: &str = "rdfs";
static PREFIX_XSD: &str = "xsd";

static NS_OWL: &str = "http://www.w3.org/2002/07/owl#";
static NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
static NS_RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
static NS_XSD: &str = "http://www.w3.org/2001/XMLSchema#";

static ERR_EMPTY_ONTOLOGY: &str = "Ontology contains no entities or imports";
static ERR_EXPECTED_DOT: &str = "Expected '.' at end of statement";
static ERR_MALFORMED_PREFIX: &str = "Malformed @prefix: missing trailing ':'";
static ERR_MALFORMED_PREFIX_NS: &str = "Malformed @prefix: namespace must be <...>";
static ERR_MALFORMED_PREFIX_DECL: &str = "Malformed @prefix declaration";

/// Turtle format parser
pub struct TurtleParser {
    config: ParserConfig,
    prefixes: HashMap<String, String>, // TODO: Could be optimized to use Cow<str>
    /// Arena allocator for efficient string and object allocation
    arena: Option<Box<dyn ParserArenaTrait>>,
}

impl TurtleParser {
    /// Helper function to convert Arc<IRI> to IRI with minimal overhead
    fn arc_to_iri(arc_iri: OwlResult<Arc<IRI>>) -> OwlResult<IRI> {
        arc_iri.map(|arc| (*arc).clone())
    }

    /// Create a new Turtle parser with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new Turtle parser with custom configuration
    #[must_use]
    pub fn with_config(config: ParserConfig) -> Self {
        let mut prefixes = HashMap::new();
        for (prefix, namespace) in &config.prefixes {
            prefixes.insert(prefix.clone(), namespace.clone());
        }

        // Add standard OWL/RDF prefixes by default for robustness
        prefixes.insert(PREFIX_OWL.to_string(), NS_OWL.to_string());
        prefixes.insert(PREFIX_RDF.to_string(), NS_RDF.to_string());
        prefixes.insert(PREFIX_RDFS.to_string(), NS_RDFS.to_string());
        prefixes.insert(PREFIX_XSD.to_string(), NS_XSD.to_string());

        // Initialize arena allocator if enabled
        let arena = if config.use_arena_allocation {
            Some(
                ParserArenaBuilder::new()
                    .with_capacity(config.arena_capacity)
                    .build(),
            )
        } else {
            None
        };

        TurtleParser {
            config,
            prefixes,
            arena,
        }
    }

    /// Get a reference to the arena allocator
    fn arena(&self) -> Option<&dyn ParserArenaTrait> {
        self.arena.as_deref()
    }

    /// Allocate a string in the arena if available, otherwise return the original
    fn alloc_string<'a>(&'a self, s: &'a str) -> &'a str {
        if let Some(arena) = self.arena() {
            arena.arena().alloc_str(s)
        } else {
            s
        }
    }

    /// Allocate a string in the arena if available, otherwise clone
    fn alloc_string_clone(&self, s: &str) -> String {
        if let Some(arena) = self.arena() {
            arena.arena().alloc_str(s).to_string()
        } else {
            s.to_string()
        }
    }

    /// Parse Turtle content and build an ontology using arena allocation
    fn parse_content(&mut self, content: &str) -> OwlResult<Ontology> {
        // Comprehensive input validation
        self.validate_parser_input(content)?;

        if self.config.strict_validation && content.trim().is_empty() {
            return Err(OwlError::ValidationError(
                self.alloc_string_clone(ERR_EMPTY_ONTOLOGY),
            ));
        }
        let mut ontology = Ontology::new();

        // Process compound statements with semicolon continuation
        let mut current_subject: Option<IRI> = None;

        for raw_line in content.lines() {
            let line = self.alloc_string(raw_line.trim());
            if line.is_empty() || line.starts_with('#') {
                continue; // Skip empty lines and comments
            }

            // Parse prefix declarations
            if line.starts_with("@prefix") {
                let (prefix, namespace) = self.parse_prefix_declaration(line)?;
                self.prefixes.insert(prefix, namespace);
                continue;
            }

            // Strip inline comments for validation - use arena allocation
            let stmt = line.split('#').next().unwrap_or("").trim_end();
            if stmt.is_empty() {
                continue;
            }

            // In strict mode, require statements to end with a dot or continue characters
            if self.config.strict_validation
                && !(stmt.ends_with('.') || stmt.ends_with(';') || stmt.ends_with(','))
            {
                return Err(crate::core::error::OwlError::ParseError(
                    self.alloc_string_clone(ERR_EXPECTED_DOT),
                ));
            }

            // Handle compound statements - use arena allocation for clean statement
            let ends_with_dot = stmt.ends_with('.');
            let ends_with_semicolon = stmt.ends_with(';');
            let clean_stmt = self.alloc_string(stmt.trim_end_matches(['.', ';', ',']));

            // Handle compound statement predicate-object pairs
            if let Some(ref current_subj) = current_subject {
                // Try to parse as predicate-object pair for compound statements
                if let Some((predicate, object)) = self.parse_predicate_object_pair(clean_stmt) {
                    self.process_triple(&mut ontology, current_subj.clone(), predicate, object)?;

                    // Reset current subject at end of statement
                    if ends_with_dot {
                        current_subject = None;
                    }
                    continue;
                }
            }

            // Parse complete triple
            if let Some((subject, predicate, object)) = self.parse_triple(clean_stmt) {
                // Update current subject for compound statements
                if current_subject.is_none() || ends_with_dot {
                    current_subject = Some(subject.clone());
                }

                // Use current subject for compound statements
                let actual_subject = if ends_with_semicolon {
                    if let Some(ref current) = current_subject {
                        current.clone()
                    } else {
                        subject
                    }
                } else {
                    subject
                };

                self.process_triple(&mut ontology, actual_subject, predicate, object)?;

                // Reset current subject at end of statement
                if ends_with_dot {
                    current_subject = None;
                }
            }
            // Leniently skip lines we can't parse (multi-line constructs), strictness enforced by other checks
        }

        if self.config.strict_validation {
            self.validate_ontology(&ontology)?;
        }

        // Resolve imports if configured to do so
        if self.config.resolve_imports {
            if let Err(e) = ontology.resolve_imports() {
                if self.config.ignore_import_errors {
                    log::warn!("Import resolution failed: {e}");
                } else {
                    return Err(e);
                }
            }
        }

        Ok(ontology)
    }

    /// Parse a prefix declaration using arena allocation
    fn parse_prefix_declaration(&self, line: &str) -> OwlResult<(String, String)> {
        let arena_line = self.alloc_string(line);
        let parts: Vec<&str> = arena_line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "@prefix" {
            let prefix_token = self.alloc_string(parts[1]);
            let ns_token = self.alloc_string(parts[2]);

            // Validate prefix token ends with ':'
            if !prefix_token.ends_with(':') {
                return Err(crate::core::error::OwlError::ParseError(
                    self.alloc_string_clone(ERR_MALFORMED_PREFIX),
                ));
            }
            let prefix = self.alloc_string(prefix_token.trim_end_matches(':'));

            // Namespace must be enclosed in <>
            if !(ns_token.starts_with('<') && ns_token.ends_with('>')) {
                return Err(crate::core::error::OwlError::ParseError(
                    self.alloc_string_clone(ERR_MALFORMED_PREFIX_NS),
                ));
            }
            let namespace = self.alloc_string(ns_token.trim_matches('<').trim_matches('>'));

            // Use arena allocation for prefix and namespace strings
            let prefix_str = self.alloc_string_clone(prefix);
            let namespace_str = self.alloc_string_clone(namespace);

            return Ok((prefix_str, namespace_str));
        }
        if self.config.strict_validation {
            return Err(crate::core::error::OwlError::ParseError(
                self.alloc_string_clone(ERR_MALFORMED_PREFIX_DECL),
            ));
        }
        Err(crate::core::error::OwlError::ParseError(
            self.alloc_string_clone(ERR_MALFORMED_PREFIX_DECL),
        ))
    }

    /// Parse a predicate-object pair for compound statements using arena allocation
    fn parse_predicate_object_pair(&self, line: &str) -> Option<(IRI, ObjectValue)> {
        let arena_line = self.alloc_string(line);
        let tokens = self.tokenize_turtle_line(arena_line);

        if tokens.len() < 2 {
            return None;
        }

        let predicate = self.parse_predicate(&tokens[0])?;
        let (object, _remaining_tokens) = self.parse_object(&tokens[1..])?;

        Some((predicate, object))
    }

    /// Parse a Turtle triple with support for complex constructs using arena allocation
    fn parse_triple(&self, line: &str) -> Option<(IRI, IRI, ObjectValue)> {
        let arena_line = self.alloc_string(line.trim_end_matches(['.', ';', ',']));
        let tokens = self.tokenize_turtle_line(arena_line);

        if tokens.len() < 3 {
            return None;
        }

        let subject = self.parse_subject(&tokens[0])?;
        let predicate = self.parse_predicate(&tokens[1])?;
        let (object, _remaining_tokens) = self.parse_object(&tokens[2..])?;

        Some((subject, predicate, object))
    }

    /// Tokenize a Turtle line handling quotes and nested structures with arena allocation
    fn tokenize_turtle_line(&self, line: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut in_blank_node = false;
        let mut bracket_depth = 0;
        let chars = line.chars().peekable();

        for c in chars {
            match c {
                '"' => {
                    in_quotes = !in_quotes;
                    current.push(c);
                }
                '[' if !in_quotes => {
                    if bracket_depth == 0 {
                        if !current.trim().is_empty() {
                            let token = self.alloc_string_clone(current.trim());
                            tokens.push(token);
                            current.clear();
                        }
                        in_blank_node = true;
                    }
                    bracket_depth += 1;
                    current.push(c);
                }
                ']' if !in_quotes && in_blank_node => {
                    bracket_depth -= 1;
                    current.push(c);
                    if bracket_depth == 0 {
                        let token = self.alloc_string_clone(&current);
                        tokens.push(token);
                        current.clear();
                        in_blank_node = false;
                    }
                }
                '(' if !in_quotes => {
                    bracket_depth += 1;
                    current.push(c);
                }
                ')' if !in_quotes => {
                    bracket_depth -= 1;
                    current.push(c);
                }
                ' ' | '\t' if !in_quotes && bracket_depth == 0 => {
                    if !current.trim().is_empty() {
                        let token = self.alloc_string_clone(current.trim());
                        tokens.push(token);
                        current.clear();
                    }
                }
                _ => {
                    current.push(c);
                }
            }
        }

        if !current.trim().is_empty() {
            let token = self.alloc_string_clone(current.trim());
            tokens.push(token);
        }

        tokens
    }

    /// Parse a subject (IRI or blank node) using arena allocation
    fn parse_subject(&self, token: &str) -> Option<IRI> {
        if let Some(stripped) = token.strip_prefix("_:") {
            // Blank node - generate temporary IRI for processing using arena allocation
            let blank_iri = self.alloc_string(stripped);
            Self::arc_to_iri(IRI::new_optimized(format!("http://blank.node/{blank_iri}")))
            .ok()
        } else {
            let arena_token = self.alloc_string(token);
            self.parse_curie_or_iri(arena_token).ok()
        }
    }

    /// Parse a predicate (handle "a" keyword) using arena allocation
    fn parse_predicate(&self, token: &str) -> Option<IRI> {
        if token == "a" {
            // Use arena allocation for the type IRI
            let type_iri = self.alloc_string("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
            Self::arc_to_iri(IRI::new_optimized(type_iri)).ok()
        } else {
            let arena_token = self.alloc_string(token);
            self.parse_curie_or_iri(arena_token).ok()
        }
    }

    /// Parse an object with support for complex structures using arena allocation
    fn parse_object(&self, tokens: &[String]) -> Option<(ObjectValue, Vec<String>)> {
        if tokens.is_empty() {
            return None;
        }

        let first_token = self.alloc_string(&tokens[0]);

        if let Some(stripped) = first_token.strip_prefix("_:") {
            // Blank node - use arena allocation for the blank node ID
            let arena_stripped = self.alloc_string(stripped);
            Some((
                ObjectValue::BlankNode(arena_stripped.to_string()),
                Vec::from(&tokens[1..]),
            ))
        } else if first_token.starts_with('"') {
            // Literal
            let literal = self.parse_literal(first_token)?;
            Some((ObjectValue::Literal(literal), tokens[1..].to_vec()))
        } else if first_token.starts_with('[') {
            // Blank node with properties (nested structure)
            let (nested_object, consumed) = self.parse_blank_node_structure(first_token)?;
            Some((
                ObjectValue::Nested(Box::new(nested_object)),
                Vec::from(&tokens[consumed..]),
            ))
        } else if first_token.starts_with('(') {
            // Collection (list)
            let (list_items, consumed) = self.parse_collection(tokens)?;
            let nested_object = NestedObject {
                object_type: self.alloc_string_clone("Collection"),
                properties: HashMap::new(),
                list_items,
            };
            Some((
                ObjectValue::Nested(Box::new(nested_object)),
                Vec::from(&tokens[consumed..]),
            ))
        } else {
            // Simple IRI
            let iri = self.parse_curie_or_iri(first_token).ok()?;
            Some((ObjectValue::IRI(iri), tokens[1..].to_vec()))
        }
    }

    /// Parse a literal value using arena allocation
    fn parse_literal(&self, token: &str) -> Option<Literal> {
        if !token.starts_with('"') || !token.ends_with('"') {
            return None;
        }

        let content = &token[1..token.len() - 1];
        // Use arena allocation for literal content
        let arena_content = self.alloc_string(content);
        let literal = Literal::simple(arena_content.to_string());
        Some(literal)
    }

    /// Parse blank node structure [ ... ] using arena allocation
    #[allow(clippy::unused_self)]
    fn parse_blank_node_structure(&self, content: &str) -> Option<(NestedObject, usize)> {
        // Generate a unique ID for this blank node based on content hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let _unique_id = format!("nested_{}", hasher.finish());

        // Parse the content to extract properties
        let clean_content =
            self.alloc_string(content.trim_start_matches('[').trim_end_matches(']').trim());
        let tokens = self.tokenize_turtle_line(clean_content);

        let mut properties = HashMap::new();

        // Simple parsing for property pairs inside the blank node
        let mut i = 0;
        while i < tokens.len() {
            if let Some(predicate) = self.parse_predicate(&tokens[i]) {
                if i + 1 < tokens.len() {
                    if let Some((object, _remaining_tokens)) = self.parse_object(&tokens[i + 1..]) {
                        // Store the property using arena-allocated key
                        properties.insert(predicate.to_string(), object);
                        i += 1; // Move to next token after the object
                        continue;
                    }
                }
            }
            i += 1;
        }

        let nested_object = NestedObject {
            object_type: self.alloc_string_clone("BlankNode"),
            properties,
            list_items: Vec::new(),
        };

        Some((nested_object, 1))
    }

    /// Parse collection ( ... ) using arena allocation
    #[allow(clippy::unused_self)]
    fn parse_collection(&self, tokens: &[String]) -> Option<(Vec<ObjectValue>, usize)> {
        let mut items = Vec::new();
        let mut consumed = 0;

        for token in tokens {
            consumed += 1;
            if token == ")" {
                break;
            }

            if token != "(" {
                if let Ok(iri) = self.parse_curie_or_iri(token) {
                    items.push(ObjectValue::IRI(iri));
                }
            }
        }

        Some((items, consumed))
    }

    /// Parse a CURIE or IRI using arena allocation
    fn parse_curie_or_iri(&self, s: &str) -> OwlResult<IRI> {
        if s.starts_with('<') && s.ends_with('>') {
            // Full IRI - use arena allocation for the content
            let iri_content = self.alloc_string(&s[1..s.len() - 1]);
            Self::arc_to_iri(IRI::new_optimized(iri_content))
        } else if let Some(colon_pos) = s.find(':') {
            // CURIE
            let prefix = self.alloc_string(&s[..colon_pos]);
            let local = self.alloc_string(&s[colon_pos + 1..]);

            if let Some(namespace) = self.prefixes.get(prefix.to_string().as_str()) {
                // Use arena allocation for the constructed IRI string
                let iri_string = format!("{namespace}{local}");
                let arena_iri_string = self.alloc_string(&iri_string);
                Self::arc_to_iri(IRI::new_optimized(arena_iri_string))
            } else if self.config.strict_validation {
                Err(crate::core::error::OwlError::ParseError(format!("Undefined prefix: {prefix}")))
            } else {
                // Treat as full IRI in non-strict mode
                let arena_s = self.alloc_string(s);
                Self::arc_to_iri(IRI::new_optimized(arena_s))
            }
        } else {
            // Treat as full IRI - use arena allocation
            let arena_s = self.alloc_string(s);
            Self::arc_to_iri(IRI::new_optimized(arena_s))
        }
    }

    /// Process a single triple with comprehensive OWL2 support
    #[allow(clippy::too_many_lines)]
    fn process_triple(
        &self,
        ontology: &mut Ontology,
        subject: IRI,
        predicate: IRI,
        object: ObjectValue,
    ) -> OwlResult<()> {
        match predicate.as_str() {
            // RDF type declarations (entity declarations)
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" => {
                self.process_type_declaration(ontology, &subject, object)?;
            }

            // RDFS subclass relationships
            // RDFS subclass relationships
            "http://www.w3.org/2000/01/rdf-schema#subClassOf" => {
                if let ObjectValue::IRI(super_class_iri) = object {
                    // Automatically add both subject and super class to the ontology
                    ontology.add_class(Class::new(subject.clone()))?;
                    ontology.add_class(Class::new(super_class_iri.clone()))?;

                    let subclass_axiom = SubClassOfAxiom::new(
                        ClassExpression::Class(Class::new(subject)),
                        ClassExpression::Class(Class::new(super_class_iri)),
                    );
                    ontology.add_axiom(Axiom::SubClassOf(Box::new(subclass_axiom)))?;
                }
            }

            // OWL equivalent classes
            "http://www.w3.org/2002/07/owl#equivalentClass" => {
                if let ObjectValue::IRI(equiv_class_iri) = object {
                    let equiv_axiom = EquivalentClassesAxiom::new(vec![
                        Arc::new(subject.clone()),
                        Arc::new(equiv_class_iri.clone()),
                    ]);
                    ontology.add_axiom(Axiom::EquivalentClasses(Box::new(equiv_axiom)))?;
                } else if let ObjectValue::Nested(nested) = object {
                    // Handle complex equivalent class expressions (restrictions, intersections, etc.)
                    if let Some(class_expr) = self.parse_nested_class_expression(&nested) {
                        // For complex expressions, we need to use two SubClassOf axioms
                        let subclass_axiom1 = SubClassOfAxiom::new(
                            ClassExpression::Class(Class::new(subject.clone())),
                            class_expr.clone(),
                        );
                        let subclass_axiom2 = SubClassOfAxiom::new(
                            class_expr,
                            ClassExpression::Class(Class::new(subject.clone())),
                        );
                        ontology.add_axiom(Axiom::SubClassOf(Box::new(subclass_axiom1)))?;
                        ontology.add_axiom(Axiom::SubClassOf(Box::new(subclass_axiom2)))?;
                    }
                }
            }

            // OWL disjoint classes
            "http://www.w3.org/2002/07/owl#disjointWith" => {
                if let ObjectValue::IRI(disjoint_class_iri) = object {
                    let disjoint_axiom = DisjointClassesAxiom::new(vec![
                        Arc::new(subject.clone()),
                        Arc::new(disjoint_class_iri.clone()),
                    ]);
                    ontology.add_axiom(Axiom::DisjointClasses(Box::new(disjoint_axiom)))?;
                }
            }

            // OWL property characteristics
            "http://www.w3.org/2002/07/owl#equivalentProperty" => {
                if let ObjectValue::IRI(equiv_prop_iri) = object {
                    let equiv_axiom = EquivalentObjectPropertiesAxiom::new(vec![
                        Arc::new(subject.clone()),
                        Arc::new(equiv_prop_iri.clone()),
                    ]);
                    ontology.add_axiom(Axiom::EquivalentObjectProperties(Box::new(equiv_axiom)))?;
                }
            }

            "http://www.w3.org/2002/07/owl#inverseOf" => {
                if let ObjectValue::IRI(inverse_prop_iri) = object {
                    let inverse_axiom = InverseObjectPropertiesAxiom::new(
                        Arc::new(subject),
                        Arc::new(inverse_prop_iri),
                    );
                    ontology.add_axiom(Axiom::InverseObjectProperties(Box::new(inverse_axiom)))?;
                }
            }

            // Property domain and range
            "http://www.w3.org/2000/01/rdf-schema#domain" => {
                if let ObjectValue::IRI(domain_iri) = object {
                    // Add domain as a subclass axiom: ∀p.Domain ⊑ Domain
                    let domain_class = ClassExpression::Class(Class::new(domain_iri));
                    let property_expr = ObjectPropertyExpression::ObjectProperty(Box::new(
                        ObjectProperty::new(subject.clone()),
                    ));
                    let restriction = ClassExpression::ObjectAllValuesFrom(
                        Box::new(property_expr),
                        Box::new(domain_class),
                    );

                    let subclass_axiom = SubClassOfAxiom::new(
                        ClassExpression::ObjectSomeValuesFrom(
                            Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
                                ObjectProperty::new(subject),
                            ))),
                            Box::new(ClassExpression::Class(Class::new(IRI::new(
                                "http://www.w3.org/2002/07/owl#Thing",
                            )?))),
                        ),
                        restriction,
                    );
                    ontology.add_axiom(Axiom::SubClassOf(Box::new(subclass_axiom)))?;
                }
            }

            "http://www.w3.org/2000/01/rdf-schema#range" => {
                if let ObjectValue::IRI(range_iri) = object {
                    // Add range constraint: ∀p.∃range ⊑ Range
                    let range_class = ClassExpression::Class(Class::new(range_iri));
                    let property_expr = ObjectPropertyExpression::ObjectProperty(Box::new(
                        ObjectProperty::new(subject.clone()),
                    ));

                    let subclass_axiom = SubClassOfAxiom::new(
                        ClassExpression::ObjectAllValuesFrom(
                            Box::new(property_expr),
                            Box::new(range_class),
                        ),
                        ClassExpression::Class(Class::new(IRI::new(
                            "http://www.w3.org/2002/07/owl#Thing",
                        )?)),
                    );
                    ontology.add_axiom(Axiom::SubClassOf(Box::new(subclass_axiom)))?;
                }
            }

            // OWL imports
            "http://www.w3.org/2002/07/owl#imports" => {
                if let ObjectValue::IRI(import_iri) = object {
                    ontology.add_import(import_iri);
                }
            }

            // Property assertions (individual relationships)
            _ => {
                // Handle as property assertion between individuals
                self.process_property_assertion(ontology, subject, predicate, object)?;
            }
        }

        Ok(())
    }

    /// Process RDF type declarations
    fn process_type_declaration(
        &self,
        ontology: &mut Ontology,
        subject: &IRI,
        object: ObjectValue,
    ) -> OwlResult<()> {
        if let ObjectValue::IRI(type_iri) = object {
            match type_iri.as_str() {
                "http://www.w3.org/2002/07/owl#Ontology" => {
                    ontology.set_iri(subject.clone());
                }
                "http://www.w3.org/2002/07/owl#Class"
                | "http://www.w3.org/2000/01/rdf-schema#Class" => {
                    ontology.add_class(Class::new(subject.clone()))?;
                }
                "http://www.w3.org/2002/07/owl#ObjectProperty" => {
                    ontology.add_object_property(ObjectProperty::new(subject.clone()))?;
                }
                "http://www.w3.org/2002/07/owl#DataProperty" => {
                    ontology.add_data_property(DataProperty::new(subject.clone()))?;
                }
                "http://www.w3.org/2002/07/owl#NamedIndividual" => {
                    let individual = NamedIndividual::new(subject.clone());
                    ontology.add_named_individual(individual.clone())?;

                    // Create class assertion
                    let class_assertion = ClassAssertionAxiom::new(
                        Arc::new(subject.clone()),
                        ClassExpression::Class(Class::new(type_iri.clone())),
                    );
                    ontology.add_axiom(Axiom::ClassAssertion(Box::new(class_assertion)))?;
                }
                // Handle property declarations
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property" => {
                    // Add as object property (default)
                    let property = ObjectProperty::new(subject.clone());
                    ontology.add_object_property(property)?;
                }
                // Handle other types as potential named individuals
                _ => {
                    // Check if this is a well-known OWL type that should not be treated as individual
                    if type_iri.as_str().contains("Property") {
                        // Handle as property declaration
                        if type_iri.as_str().contains("ObjectProperty") {
                            let property = ObjectProperty::new(subject.clone());
                            ontology.add_object_property(property)?;
                        } else if type_iri.as_str().contains("DataProperty") {
                            let property = DataProperty::new(subject.clone());
                            ontology.add_data_property(property)?;
                        } else {
                            // Default to object property
                            let property = ObjectProperty::new(subject.clone());
                            ontology.add_object_property(property)?;
                        }
                    } else {
                        // Add as individual and create class assertion
                        let individual = NamedIndividual::new(subject.clone());
                        ontology.add_named_individual(individual.clone())?;

                        let class_assertion = ClassAssertionAxiom::new(
                            Arc::new(subject.clone()),
                            ClassExpression::Class(Class::new(type_iri)),
                        );
                        ontology.add_axiom(Axiom::ClassAssertion(Box::new(class_assertion)))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Process property assertions between individuals
    fn process_property_assertion(
        &self,
        ontology: &mut Ontology,
        subject: IRI,
        predicate: IRI,
        object: ObjectValue,
    ) -> OwlResult<()> {
        // Create or ensure subject individual exists
        let subject_individual = NamedIndividual::new(subject.clone());
        ontology.add_named_individual(subject_individual.clone())?;

        match object {
            ObjectValue::IRI(object_iri) => {
                // Object property assertion
                let object_individual = NamedIndividual::new(object_iri.clone());
                ontology.add_named_individual(object_individual.clone())?;

                let property_assertion = PropertyAssertionAxiom::new(
                    subject_individual.iri().clone(),
                    Arc::new(predicate),
                    object_individual.iri().clone(),
                );
                ontology.add_axiom(Axiom::PropertyAssertion(Box::new(property_assertion)))?;
            }
            ObjectValue::Literal(literal) => {
                // Data property assertion with literal values
                let data_property_assertion = DataPropertyAssertionAxiom::new(
                    subject_individual.iri().clone(),
                    Arc::new(predicate.clone()),
                    literal.clone(),
                );
                ontology.add_data_property_assertion(data_property_assertion)?;
            }
            ObjectValue::BlankNode(node_id) => {
                // Create anonymous individual for blank node
                let anon_individual = AnonymousIndividual::new(node_id);
                ontology.add_anonymous_individual(anon_individual.clone())?;

                // Create property assertion with anonymous individual as object
                let property_assertion = PropertyAssertionAxiom::new_with_anonymous(
                    subject_individual.iri().clone(),
                    Arc::new(predicate),
                    anon_individual,
                );
                ontology.add_axiom(Axiom::PropertyAssertion(Box::new(property_assertion)))?;
            }
            ObjectValue::Nested(nested) => {
                // Handle RDF collections and other nested structures
                if nested.object_type == "Collection" || nested.object_type == "RDFList" {
                    self.process_rdf_collection(
                        ontology,
                        subject_individual.iri(),
                        predicate,
                        &nested,
                    )?;
                } else if nested.object_type == "BlankNode" {
                    // Create anonymous individual for nested blank node
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};

                    let mut hasher = DefaultHasher::new();
                    format!("{nested:?}").hash(&mut hasher);
                    let anon_id = format!("nested_{}", hasher.finish());
                    let anon_individual = AnonymousIndividual::new(anon_id);
                    ontology.add_anonymous_individual(anon_individual.clone())?;

                    // Create property assertion with anonymous individual
                    let property_assertion = PropertyAssertionAxiom::new_with_anonymous(
                        subject_individual.iri().clone(),
                        Arc::new(predicate),
                        anon_individual,
                    );
                    ontology.add_axiom(Axiom::PropertyAssertion(Box::new(property_assertion)))?;

                    // Also process any properties defined inside the nested blank node
                    for (prop_str, _obj_str) in &nested.properties {
                        if let Ok(_prop_iri) = self.parse_curie_or_iri(prop_str) {
                            // For each property, create a new property assertion
                            // This is simplified - in a full implementation, we'd parse the actual objects
                            if prop_str.contains("name") {
                                // Create a literal for the name property
                                let _name_literal = Literal::simple("Anonymous Person".to_string());
                                // Note: This is a data property assertion which needs a different axiom type
                                // For now, we'll just acknowledge that properties exist
                            }
                        }
                    }
                } else {
                    // Handle other nested object types
                    // For now, skip complex nested structures
                }
            }
        }
        Ok(())
    }

    /// Process RDF collections (rdf:first, rdf:rest, rdf:nil linked lists)
    #[allow(clippy::unused_self)]
    fn process_rdf_collection(
        &self,
        ontology: &mut Ontology,
        _subject: &IRI,
        _predicate: IRI,
        nested: &NestedObject,
    ) -> OwlResult<()> {
        let mut items = Vec::new();

        // Process list items
        for item in &nested.list_items {
            match item {
                ObjectValue::IRI(iri) => {
                    items.push(CollectionItem::Named(Arc::new(iri.clone())));
                }
                ObjectValue::BlankNode(node_id) => {
                    let anon_individual = AnonymousIndividual::new(node_id);
                    ontology.add_anonymous_individual(anon_individual.clone())?;
                    items.push(CollectionItem::Anonymous(Box::new(anon_individual)));
                }
                ObjectValue::Literal(lit) => {
                    items.push(CollectionItem::Literal(lit.clone()));
                }
                ObjectValue::Nested(_) => {
                    // Skip nested collections for now
                }
            }
        }

        if !items.is_empty() {
            // Create collection axiom
            let collection_axiom = CollectionAxiom::new(items);

            // Also store the collection axiom for future reference
            ontology.add_axiom(Axiom::Collection(Box::new(collection_axiom)))?;
        }

        Ok(())
    }

    /// Parse nested class expressions from complex structures
    #[allow(clippy::unused_self)]
    fn parse_nested_class_expression(&self, nested: &NestedObject) -> Option<ClassExpression> {
        match nested.object_type.as_str() {
            "Collection" => {
                // Handle intersectionOf, unionOf, oneOf
                if !nested.list_items.is_empty() {
                    // Default to intersection for collections
                    let classes: SmallVec<[Box<ClassExpression>; 4]> = nested
                        .list_items
                        .iter()
                        .filter_map(|item| {
                            if let ObjectValue::IRI(iri) = item {
                                Some(Box::new(ClassExpression::Class(Class::new(iri.clone()))))
                            } else {
                                None
                            }
                        })
                        .collect();

                    if classes.len() >= 2 {
                        return Some(ClassExpression::ObjectIntersectionOf(classes));
                    }
                }
                None
            }
            "BlankNode" => {
                // Check for restriction patterns in properties
                if let Some(ObjectValue::IRI(prop_iri)) = nested
                    .properties
                    .get("http://www.w3.org/2002/07/owl#onProperty")
                {
                    let property_expr = ObjectPropertyExpression::ObjectProperty(Box::new(
                        ObjectProperty::new(prop_iri.clone()),
                    ));

                    // Check for someValuesFrom
                    if let Some(ObjectValue::IRI(range_iri)) = nested
                        .properties
                        .get("http://www.w3.org/2002/07/owl#someValuesFrom")
                    {
                        return Some(ClassExpression::ObjectSomeValuesFrom(
                            Box::new(property_expr),
                            Box::new(ClassExpression::Class(Class::new(range_iri.clone()))),
                        ));
                    }

                    // Check for allValuesFrom
                    if let Some(ObjectValue::IRI(range_iri)) = nested
                        .properties
                        .get("http://www.w3.org/2002/07/owl#allValuesFrom")
                    {
                        return Some(ClassExpression::ObjectAllValuesFrom(
                            Box::new(property_expr),
                            Box::new(ClassExpression::Class(Class::new(range_iri.clone()))),
                        ));
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Validate the parsed ontology
    #[allow(clippy::unused_self)]
    fn validate_ontology(&self, ontology: &Ontology) -> OwlResult<()> {
        // Basic validation checks - allow ontologies with only imports
        if ontology.classes().is_empty()
            && ontology.object_properties().is_empty()
            && ontology.data_properties().is_empty()
            && ontology.named_individuals().is_empty()
            && ontology.imports().is_empty()
        {
            return Err(crate::core::error::OwlError::ValidationError(
                "Ontology contains no entities or imports".to_string(),
            ));
        }

        Ok(())
    }
}

impl OntologyParser for TurtleParser {
    fn parse_str(&self, content: &str) -> OwlResult<Ontology> {
        // Create a mutable copy for parsing
        let mut parser_copy = TurtleParser::with_config(self.config.clone());
        parser_copy.parse_content(content)
    }

    fn parse_file(&self, path: &Path) -> OwlResult<Ontology> {
        use std::fs;
        use std::io::Read;

        // Check file size
        if self.config.max_file_size > 0 {
            let metadata = fs::metadata(path)?;
            if metadata.len() > self.config.max_file_size as u64 {
                return Err(crate::core::error::OwlError::ParseError(format!(
                    "File size exceeds maximum allowed size: {} bytes",
                    self.config.max_file_size
                )));
            }
        }

        if self.should_use_streaming(path) {
            return self.parse_file_streaming(path);
        }

        let mut file = fs::File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        self.parse_str(&content)
    }

    fn format_name(&self) -> &'static str {
        "Turtle"
    }
}

impl Default for TurtleParser {
    fn default() -> Self {
        Self::new()
    }
}

impl TurtleParser {
    fn env_truthy(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn should_use_streaming(&self, path: &Path) -> bool {
        if Self::env_truthy("OWL2_REASONER_LARGE_PARSE") {
            return true;
        }
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.len() > 32 * 1024 * 1024;
        }
        false
    }

    fn parse_file_streaming(&self, path: &Path) -> OwlResult<Ontology> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;
        let reader = BufReader::new(file);

        let base_iri = oxiri::Iri::parse(format!("file:{}", path.display())).ok();
        let mut parser = RioTurtleParser::new(reader, base_iri);

        let mut ontology = Ontology::new();
        let mut handler = |triple: RioTriple| -> Result<(), TurtleError> {
            self.process_rio_triple(&mut ontology, triple).map_err(|err| {
                let io_err = std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{:?}", err),
                );
                TurtleError::from(io_err)
            })
        };

        parser.parse_all(&mut handler).map_err(|err| {
            crate::core::error::OwlError::ParseError(format!("Turtle parse error: {}", err))
        })?;

        self.validate_ontology(&ontology)?;
        Ok(ontology)
    }

    fn process_rio_triple(&self, ontology: &mut Ontology, triple: RioTriple) -> OwlResult<()> {
        let subject = self.rio_subject_to_iri(&triple.subject)?;
        let predicate = IRI::new(triple.predicate.iri)?;
        let object = self.rio_term_to_object(&triple.object)?;
        self.process_triple(ontology, subject, predicate, object)
    }

    fn rio_subject_to_iri(&self, subject: &RioSubject) -> OwlResult<IRI> {
        match subject {
            RioSubject::NamedNode(node) => IRI::new(node.iri),
            RioSubject::BlankNode(node) => IRI::new(format!("_:{}", node.id)),
            RioSubject::Triple(triple) => self.rio_reified_iri(triple),
        }
    }

    fn rio_term_to_object(&self, term: &RioTerm) -> OwlResult<ObjectValue> {
        match term {
            RioTerm::NamedNode(node) => Ok(ObjectValue::IRI(IRI::new(node.iri)?)),
            RioTerm::BlankNode(node) => Ok(ObjectValue::BlankNode(node.id.to_string())),
            RioTerm::Literal(literal) => Ok(ObjectValue::Literal(self.rio_literal_to_literal(literal)?)),
            RioTerm::Triple(triple) => Ok(ObjectValue::IRI(self.rio_reified_iri(triple)?)),
        }
    }

    fn rio_literal_to_literal(&self, literal: &RioLiteral) -> OwlResult<Literal> {
        Ok(match literal {
            RioLiteral::Simple { value } => Literal::simple((*value).to_string()),
            RioLiteral::LanguageTaggedString { value, language } => {
                Literal::lang_tagged((*value).to_string(), (*language).to_string())
            }
            RioLiteral::Typed { value, datatype } => {
                let datatype_iri = IRI::new(datatype.iri)?;
                Literal::typed((*value).to_string(), datatype_iri)
            }
        })
    }

    fn rio_reified_iri(&self, triple: &RioTriple) -> OwlResult<IRI> {
        let mut hasher = DefaultHasher::new();
        self.rio_subject_key(&triple.subject).hash(&mut hasher);
        triple.predicate.iri.hash(&mut hasher);
        self.rio_term_key(&triple.object).hash(&mut hasher);
        let id = format!("_:reified_{}", hasher.finish());
        IRI::new(id)
    }

    fn rio_subject_key(&self, subject: &RioSubject) -> String {
        match subject {
            RioSubject::NamedNode(node) => node.iri.to_string(),
            RioSubject::BlankNode(node) => format!("_:{:?}", node.id),
            RioSubject::Triple(triple) => format!(
                "triple:{}:{}:{}",
                self.rio_subject_key(&triple.subject),
                triple.predicate.iri,
                self.rio_term_key(&triple.object)
            ),
        }
    }

    fn rio_term_key(&self, term: &RioTerm) -> String {
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
                self.rio_subject_key(&triple.subject),
                triple.predicate.iri,
                self.rio_term_key(&triple.object)
            ),
        }
    }

    /// Comprehensive parser input validation
    fn validate_parser_input(&self, content: &str) -> OwlResult<()> {
        // Size validation
        self.validate_input_size(content)?;
        // Security validation
        self.validate_malformed_content(content)?;
        // Structure validation
        self.validate_turtle_structure(content)?;
        Ok(())
    }

    /// Validate input size constraints
    fn validate_input_size(&self, content: &str) -> OwlResult<()> {
        let max_file_size = if self.config.max_file_size == 0 {
            usize::MAX
        } else {
            self.config.max_file_size
        };
        let max_line_length = if max_file_size == usize::MAX {
            usize::MAX
        } else {
            max_file_size
        };

        if content.len() > max_file_size {
            return Err(OwlError::ResourceLimitExceeded {
                resource_type: "file_size".to_string(),
                limit: max_file_size,
                message: format!(
                    "Input size {} exceeds maximum allowed size {}",
                    content.len(),
                    max_file_size
                ),
            });
        }

        // Check for extremely long lines that could indicate parsing issues
        if max_line_length != usize::MAX {
            for (line_num, line) in content.lines().enumerate() {
                if line.len() > max_line_length {
                    return Err(OwlError::ParseError(format!(
                        "Line {} exceeds maximum length of {} characters",
                        line_num + 1,
                        max_line_length
                    )));
                }
            }
        }
        Ok(())
    }

    /// Validate for potentially malicious content
    fn validate_malformed_content(&self, content: &str) -> OwlResult<()> {
        let content_lower = content.to_lowercase();

        // Check for injection attempts
        let suspicious_patterns = [
            "<script>",
            "javascript:",
            "vbscript:",
            "data:text/html",
            "file:///",
            "ftp://",
            "telnet:",
            "gopher:",
            "<?xml",
            "<!ENTITY",
            "SYSTEM",
            "PUBLIC",
        ];

        for pattern in &suspicious_patterns {
            if content_lower.contains(pattern) {
                return Err(OwlError::ParseError(format!(
                    "Potentially unsafe content pattern detected: '{pattern}'"
                )));
            }
        }

        // Check for excessive nesting depth
        let max_brace_depth = self.calculate_max_brace_depth(content);
        if max_brace_depth > 100 {
            return Err(OwlError::ParseError(format!(
                "Excessive nesting depth detected: {max_brace_depth}"
            )));
        }

        // Check for excessive quote levels
        let max_quote_depth = self.calculate_max_quote_depth(content);
        if max_quote_depth > 50 {
            return Err(OwlError::ParseError(format!(
                "Excessive quote nesting detected: {max_quote_depth}"
            )));
        }

        Ok(())
    }

    /// Validate basic Turtle structure
    fn validate_turtle_structure(&self, content: &str) -> OwlResult<()> {
        let mut line_count = 0;
        let mut statement_count = 0;
        let mut prefix_count = 0;

        for line in content.lines() {
            line_count += 1;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Count different types of statements
            if trimmed.starts_with("@prefix") {
                prefix_count += 1;
            } else if trimmed.contains('<') && trimmed.contains('>') {
                statement_count += 1;
            }

            // Basic syntax validation
            if line_count == 1 && trimmed.starts_with('<') {
                // First line should typically be a base or prefix declaration
                continue;
            }

            // Check for unbalanced brackets
            if !self.validate_balanced_brackets(trimmed) {
                return Err(OwlError::ParseError(format!(
                    "Unbalanced brackets in line {line_count}: {trimmed}"
                )));
            }
        }

        // Validate reasonable content ratios
        if line_count > 0 && statement_count == 0 && prefix_count == 0 {
            return Err(OwlError::ParseError(
                "No valid Turtle statements found".to_string(),
            ));
        }

        // Check for excessive prefix declarations
        if prefix_count > 1000 {
            return Err(OwlError::ParseError(format!(
                "Excessive number of prefix declarations: {prefix_count}"
            )));
        }

        Ok(())
    }

    /// Calculate maximum brace nesting depth
    #[must_use]
    fn calculate_max_brace_depth(&self, content: &str) -> usize {
        let mut max_depth = 0;
        let mut current_depth: usize = 0;
        for c in content.chars() {
            match c {
                '[' | '{' | '(' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                ']' | '}' | ')' => {
                    current_depth = current_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        max_depth
    }

    /// Calculate maximum quote nesting depth
    #[must_use]
    fn calculate_max_quote_depth(&self, content: &str) -> usize {
        let mut max_depth = 0;
        let mut current_depth: usize = 0;
        let mut in_string = false;
        let mut escape_next = false;
        for c in content.chars() {
            if escape_next {
                escape_next = false;
                continue;
            }
            match c {
                '\\' => escape_next = true,
                '"' => {
                    if in_string {
                        current_depth = current_depth.saturating_sub(1);
                        in_string = false;
                    } else {
                        current_depth += 1;
                        max_depth = max_depth.max(current_depth);
                        in_string = true;
                    }
                }
                _ => {}
            }
        }
        max_depth
    }

    /// Validate balanced brackets
    #[must_use]
    fn validate_balanced_brackets(&self, line: &str) -> bool {
        let mut stack = Vec::new();
        for c in line.chars() {
            match c {
                '[' | '{' | '(' => stack.push(c),
                ']' => {
                    if stack.pop() != Some('[') {
                        return false;
                    }
                }
                '}' => {
                    if stack.pop() != Some('{') {
                        return false;
                    }
                }
                ')' => {
                    if stack.pop() != Some('(') {
                        return false;
                    }
                }
                _ => {}
            }
        }

        stack.is_empty()
    }
}

/// Object values in Turtle (IRI, Literal, Blank Node, or nested structure)
#[derive(Debug, Clone)]
#[allow(clippy::upper_case_acronyms)]
enum ObjectValue {
    IRI(IRI),
    Literal(Literal),
    BlankNode(String),
    /// For complex nested structures like restrictions, intersections, etc.
    Nested(Box<NestedObject>),
}

/// Complex nested objects in Turtle (restrictions, class expressions, etc.)
#[derive(Debug, Clone)]
struct NestedObject {
    object_type: String,
    properties: HashMap<String, ObjectValue>,
    /// For list-like structures (intersectionOf, oneOf, etc.)
    list_items: Vec<ObjectValue>,
}
