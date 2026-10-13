//! Grammar rules for OWL Functional Syntax
//!
//! This module defines the grammar rules and production handlers for
//! parsing OWL Functional Syntax documents.
//!
//! ## Class-expression coverage (#194)
//!
//! Every class-expression constructor of the OWL 2 functional syntax
//! that the staged ORE 2015 corpus exercises is accepted in every
//! class-expression position: named classes (full IRIs, default-prefix
//! `:name`, and `prefix:name` forms), the boolean constructors, and the
//! object/data restriction families (`ObjectAllValuesFrom`,
//! `ObjectHasValue`, `ObjectHasSelf`, `Object{Min,Max,Exact}Cardinality`,
//! `ObjectOneOf`, `Data{Some,All}ValuesFrom`, `DataHasValue`,
//! `Data{Min,Max,Exact}Cardinality`), plus `DatatypeRestriction` data
//! ranges. Axiom annotations (`Annotation(property value)` parts that
//! every axiom production may carry) are consumed without model
//! emission: they are non-logical by the OWL 2 structural
//! specification. DL-safe rules (`DLSafeRule`) are consumed
//! structurally and skipped with a runtime warning — see
//! [`GrammarParser::parse_dl_safe_rule`].
//!
//! ## Model wiring, not model change
//!
//! Axiom forms whose model types only accept named entities are fed
//! through the same lowerings the RDF/XML streaming parser applies:
//! complex `EquivalentClasses` arguments become bidirectional
//! `SubClassOf` pairs, complex domain/range arguments become the
//! existential GCIs, and inverse property-chain members mint the same
//! `urn:tableauxx:inverse-role:*` auxiliary roles. No semantic-model
//! type was added or changed.

use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::iri::IRI;
use crate::parser::owl_functional::error::FunctionalSyntaxResult;
use crate::parser::owl_functional::syntax::*;
use crate::parser::owl_functional::tokenizer::{Token, TokenType};
use smallvec;
use std::collections::HashMap;
use std::sync::Arc;

const NS_OWL: &str = "http://www.w3.org/2002/07/owl#";
const NS_RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// Grammar parser for OWL Functional Syntax
pub struct GrammarParser {
    /// The tokens to parse
    tokens: Vec<Token>,
    /// Current token index
    current: usize,
    /// Prefix mappings
    prefixes: HashMap<String, String>,
    /// Local IRI interner: full IRI string to the shared `Arc<IRI>`.
    ///
    /// Corpus documents repeat the same IRIs across millions of
    /// axioms; the global IRI cache's per-lookup bookkeeping is far
    /// heavier than a plain map hit, so the grammar parser interns
    /// locally (the structural XML parser keeps a comparable dedicated
    /// interner). Fresh IRIs are constructed with
    /// `IRI::new_unchecked`, whose contract — trusted, already
    /// deduplicated strings — this map upholds.
    iri_interner: HashMap<String, Arc<IRI>>,
    /// Auxiliary roles minted for inverse property-chain members
    /// (auxiliary IRI, wrapped property IRI), mirroring the RDF/XML
    /// streaming parser's chain materialization.
    inverse_roles: Vec<(Arc<IRI>, Arc<IRI>)>,
    /// Sentinel returned by `peek` past the end of the token stream,
    /// so truncated input produces a grammar error instead of an
    /// out-of-bounds panic.
    eof_token: Token,
    /// Number of DL-safe rules consumed and skipped; reported once
    /// after parsing (see `parse_dl_safe_rule`).
    skipped_dl_safe_rules: usize,
}

impl GrammarParser {
    /// Create a new grammar parser
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            prefixes: HashMap::new(),
            iri_interner: HashMap::new(),
            inverse_roles: Vec::new(),
            eof_token: eof_sentinel(),
            skipped_dl_safe_rules: 0,
        }
    }

    /// Create a new grammar parser seeded with prefix mappings
    /// (default OWL/RDF/RDFS/XSD namespaces plus any configured ones).
    pub fn with_prefixes(tokens: Vec<Token>, prefixes: HashMap<String, String>) -> Self {
        Self {
            tokens,
            current: 0,
            prefixes,
            iri_interner: HashMap::new(),
            inverse_roles: Vec::new(),
            eof_token: eof_sentinel(),
            skipped_dl_safe_rules: 0,
        }
    }

    /// Intern a full IRI string, returning the shared `Arc<IRI>`.
    fn intern_iri(&mut self, full_iri: &str) -> FunctionalSyntaxResult<Arc<IRI>> {
        if let Some(existing) = self.iri_interner.get(full_iri) {
            return Ok(existing.clone());
        }
        let iri = Arc::new(IRI::new_unchecked(full_iri.to_string()).map_err(|e| {
            crate::parser::owl_functional::error::invalid_iri_error(format!(
                "Invalid IRI '{}': {}",
                full_iri, e
            ))
        })?);
        self.iri_interner
            .insert(full_iri.to_string(), iri.clone());
        Ok(iri)
    }

    /// Parse the entire document
    pub fn parse_document(&mut self) -> FunctionalSyntaxResult<FunctionalSyntaxAST> {
        let mut document = FunctionalSyntaxAST::new();

        while !self.is_at_end() {
            if self.match_token(TokenType::Prefix) {
                self.parse_prefix_declaration(&mut document)?;
            } else if self.match_token(TokenType::Ontology) {
                self.parse_ontology_declaration(&mut document)?;
            } else {
                for content in self.parse_ontology_content()? {
                    document.add_content(content);
                }
            }
        }

        // Emit the auxiliary inverse-role declarations and inverse
        // axioms collected while parsing property chains.
        for (auxiliary, wrapped) in std::mem::take(&mut self.inverse_roles) {
            document.add_content(OntologyContent::Declaration(
                EntityDeclaration::ObjectProperty(ObjectProperty::from_shared_iri(
                    auxiliary.clone(),
                )),
            ));
            document.add_content(OntologyContent::Axiom(Axiom::InverseObjectProperties(
                Box::new(InverseObjectPropertiesAxiom::new(auxiliary, wrapped)),
            )));
        }

        if self.skipped_dl_safe_rules > 0 {
            eprintln!(
                "[owl-functional] skipped {} DLSafeRule axiom(s): no rule-engine wiring in this build (rule-derived inferences are not evaluated)",
                self.skipped_dl_safe_rules
            );
        }

        Ok(document)
    }

    /// Parse a prefix declaration
    fn parse_prefix_declaration(
        &mut self,
        document: &mut FunctionalSyntaxAST,
    ) -> FunctionalSyntaxResult<()> {
        self.consume(TokenType::LeftParen, "Expected '(' after Prefix")?;

        let prefix = if let TokenType::Identifier = self.peek().token_type {
            let name = self.advance().lexeme.clone();
            // Check if there's a trailing colon (as a separate token)
            if self.match_token(TokenType::Colon) {
                name
            } else {
                return Err(crate::parser::owl_functional::error::grammar_error(
                    "Expected ':' after prefix name".to_string(),
                ));
            }
        } else if self.match_token(TokenType::Colon) {
            // Handle the case where prefix is just ":" (default prefix)
            "".to_string()
        } else {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "Expected prefix identifier".to_string(),
            ));
        };

        self.consume(TokenType::Equals, "Expected '=' after prefix")?;

        let namespace = if self.match_token(TokenType::IRI) {
            let iri_lexeme = self.previous().lexeme.clone();
            // Remove angle brackets
            iri_lexeme[1..iri_lexeme.len() - 1].to_string()
        } else {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "Expected namespace IRI".to_string(),
            ));
        };

        self.consume(
            TokenType::RightParen,
            "Expected ')' after prefix declaration",
        )?;

        // Store the prefix mapping
        self.prefixes.insert(prefix.clone(), namespace.clone());
        document.add_prefix(prefix, namespace);

        Ok(())
    }

    /// Parse an ontology declaration
    fn parse_ontology_declaration(
        &mut self,
        document: &mut FunctionalSyntaxAST,
    ) -> FunctionalSyntaxResult<()> {
        self.consume(TokenType::LeftParen, "Expected '(' after Ontology")?;

        let ontology_iri = if self.match_token(TokenType::IRI) {
            let iri_lexeme = self.previous().lexeme.clone();
            Some(iri_lexeme[1..iri_lexeme.len() - 1].to_string())
        } else {
            None
        };

        // Optional ontology version IRI between the ontology IRI and
        // the content; it carries no reasoning semantics here.
        if ontology_iri.is_some() && self.match_token(TokenType::IRI) {
            // Consumed as the version IRI.
        }

        // Update document's ontology IRI
        if let Some(iri) = ontology_iri {
            let FunctionalSyntaxAST::OntologyDocument {
                ontology_iri: doc_iri,
                ..
            } = document;
            *doc_iri = Some(iri);
        }

        // Parse ontology content (declarations and axioms) until closing ')'
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            for content in self.parse_ontology_content()? {
                document.add_content(content);
            }
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ontology declaration",
        )?;
        Ok(())
    }

    /// Parse ontology content (declarations and axioms)
    fn parse_ontology_content(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        if self.match_token(TokenType::Declaration) {
            self.parse_entity_declaration()
        } else if self.match_token(TokenType::Import) {
            self.parse_import_declaration()
        } else {
            // Parse various axiom types
            self.parse_axiom()
        }
    }

    /// Parse an entity declaration
    fn parse_entity_declaration(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.consume(TokenType::LeftParen, "Expected '(' after Declaration")?;

        let declaration = if self.match_token(TokenType::Class) {
            self.consume(TokenType::LeftParen, "Expected '(' after Class")?;
            let class = self.parse_class()?;
            self.consume(TokenType::RightParen, "Expected ')' after class")?;
            EntityDeclaration::Class(class)
        } else if self.match_token(TokenType::ObjectProperty) {
            self.consume(TokenType::LeftParen, "Expected '(' after ObjectProperty")?;
            let prop = self.parse_object_property()?;
            self.consume(TokenType::RightParen, "Expected ')' after object property")?;
            EntityDeclaration::ObjectProperty(prop)
        } else if self.match_token(TokenType::DataProperty) {
            self.consume(TokenType::LeftParen, "Expected '(' after DataProperty")?;
            let prop = self.parse_data_property()?;
            self.consume(TokenType::RightParen, "Expected ')' after data property")?;
            EntityDeclaration::DataProperty(prop)
        } else if self.match_token(TokenType::NamedIndividual) {
            self.consume(TokenType::LeftParen, "Expected '(' after NamedIndividual")?;
            let individual = self.parse_named_individual()?;
            self.consume(TokenType::RightParen, "Expected ')' after named individual")?;
            EntityDeclaration::NamedIndividual(individual)
        } else if self.match_token(TokenType::AnonymousIndividual) {
            self.consume(TokenType::LeftParen, "Expected '(' after AnonymousIndividual")?;
            let individual = self.parse_anonymous_individual()?;
            self.consume(TokenType::RightParen, "Expected ')' after anonymous individual")?;
            EntityDeclaration::AnonymousIndividual(individual)
        } else if self.match_token(TokenType::AnnotationProperty) {
            self.consume(TokenType::LeftParen, "Expected '(' after AnnotationProperty")?;
            let prop = self.parse_annotation_property()?;
            self.consume(TokenType::RightParen, "Expected ')' after annotation property")?;
            EntityDeclaration::AnnotationProperty(prop)
        } else if self.peek().token_type == TokenType::Identifier
            && self.peek().lexeme == "Datatype"
        {
            // `Declaration(Datatype(<IRI>))` appears in the ORE corpus.
            // The reasoner has no datatype registry, and declarations
            // carry no DL semantics, so the entity is consumed without
            // registration rather than failing the whole document.
            self.advance();
            self.consume(TokenType::LeftParen, "Expected '(' after Datatype")?;
            self.parse_iri_reference("datatype IRI")?;
            self.consume(TokenType::RightParen, "Expected ')' after datatype")?;
            self.consume(TokenType::RightParen, "Expected ')' after declaration")?;
            return Ok(Vec::new());
        } else {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "Expected entity type in declaration".to_string(),
            ));
        };

        self.consume(TokenType::RightParen, "Expected ')' after declaration")?;
        Ok(vec![OntologyContent::Declaration(declaration)])
    }

    /// Parse an import declaration
    fn parse_import_declaration(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.consume(TokenType::LeftParen, "Expected '(' after Import")?;

        let import_iri = if self.match_token(TokenType::IRI) {
            let iri_lexeme = self.previous().lexeme.clone();
            let iri_str = &iri_lexeme[1..iri_lexeme.len() - 1];
            IRI::new_optimized(iri_str).map_err(|e| {
                crate::parser::owl_functional::error::invalid_iri_error(format!(
                    "Invalid import IRI: {}",
                    e
                ))
            })?
        } else {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "Expected IRI in import declaration".to_string(),
            ));
        };

        self.consume(
            TokenType::RightParen,
            "Expected ')' after import declaration",
        )?;
        Ok(vec![OntologyContent::Import(ImportDeclaration::new(
            (*import_iri).clone(),
        ))])
    }

    /// Parse various axiom types
    fn parse_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        let token = self.peek();
        match token.token_type {
            TokenType::SubClassOf => {
                self.advance(); // consume SubClassOf
                self.parse_subclass_of_axiom()
            }
            TokenType::EquivalentClasses => {
                self.advance();
                self.parse_equivalent_classes_axiom()
            }
            TokenType::DisjointClasses => {
                self.advance();
                self.parse_disjoint_classes_axiom()
            }
            TokenType::DisjointUnion => {
                self.advance();
                self.parse_disjoint_union_axiom()
            }
            TokenType::SubObjectPropertyOf => {
                self.advance();
                self.parse_sub_object_property_of_axiom()
            }
            TokenType::EquivalentObjectProperties => {
                self.advance();
                self.parse_equivalent_object_properties_axiom()
            }
            TokenType::DisjointObjectProperties => {
                self.advance();
                self.parse_disjoint_object_properties_axiom()
            }
            TokenType::ObjectPropertyDomain => {
                self.advance();
                self.parse_object_property_domain_axiom()
            }
            TokenType::ObjectPropertyRange => {
                self.advance();
                self.parse_object_property_range_axiom()
            }
            TokenType::ClassAssertion => {
                self.advance();
                self.parse_class_assertion_axiom()
            }
            TokenType::ObjectPropertyAssertion => {
                self.advance();
                self.parse_object_property_assertion_axiom()
            }
            TokenType::DataPropertyAssertion => {
                self.advance();
                self.parse_data_property_assertion_axiom()
            }
            TokenType::NegativeObjectPropertyAssertion => {
                self.advance();
                self.parse_negative_object_property_assertion_axiom()
            }
            TokenType::SameIndividual => {
                self.advance();
                self.parse_same_individual_axiom()
            }
            TokenType::DifferentIndividuals => {
                self.advance();
                self.parse_different_individuals_axiom()
            }
            // Property characteristics
            TokenType::TransitiveObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "TransitiveObjectProperty",
                    |property| {
                        Axiom::TransitiveProperty(Box::new(TransitivePropertyAxiom::new(property)))
                    },
                )
            }
            TokenType::AsymmetricObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "AsymmetricObjectProperty",
                    |property| {
                        Axiom::AsymmetricProperty(Box::new(AsymmetricPropertyAxiom::new(property)))
                    },
                )
            }
            TokenType::IrreflexiveObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "IrreflexiveObjectProperty",
                    |property| {
                        Axiom::IrreflexiveProperty(Box::new(IrreflexivePropertyAxiom::new(
                            property,
                        )))
                    },
                )
            }
            TokenType::ReflexiveObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "ReflexiveObjectProperty",
                    |property| {
                        Axiom::ReflexiveProperty(Box::new(ReflexivePropertyAxiom::new(property)))
                    },
                )
            }
            TokenType::SymmetricObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "SymmetricObjectProperty",
                    |property| {
                        Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(property)))
                    },
                )
            }
            TokenType::FunctionalObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "FunctionalObjectProperty",
                    |property| {
                        Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(
                            property,
                        )))
                    },
                )
            }
            TokenType::InverseFunctionalObjectProperty => {
                self.advance();
                self.parse_property_characteristic_axiom(
                    "InverseFunctionalObjectProperty",
                    |property| {
                        Axiom::InverseFunctionalProperty(Box::new(
                            InverseFunctionalPropertyAxiom::new(property),
                        ))
                    },
                )
            }
            // Data property axioms
            TokenType::SubDataPropertyOf => {
                self.advance();
                self.parse_sub_data_property_of_axiom()
            }
            TokenType::EquivalentDataProperties => {
                self.advance();
                self.parse_equivalent_data_properties_axiom()
            }
            TokenType::DisjointDataProperties => {
                self.advance();
                self.parse_disjoint_data_properties_axiom()
            }
            TokenType::DataPropertyDomain => {
                self.advance();
                self.parse_data_property_domain_axiom()
            }
            TokenType::DataPropertyRange => {
                self.advance();
                self.parse_data_property_range_axiom()
            }
            TokenType::FunctionalDataProperty => {
                self.advance();
                self.parse_functional_data_property_axiom()
            }
            // Inverse properties
            TokenType::InverseObjectProperties => {
                self.advance();
                self.parse_inverse_object_properties_axiom()
            }
            // DL-safe rules (SWRL) appear in the ORE corpus; they are
            // parsed structurally and skipped — see parse_dl_safe_rule.
            TokenType::Identifier if token.lexeme == "DLSafeRule" => {
                self.advance();
                self.parse_dl_safe_rule()
            }
            _ => Err(crate::parser::owl_functional::error::grammar_error(
                format!("Unknown axiom type: {}", token.lexeme),
            )),
        }
    }

    /// Consume an axiom's opening parenthesis and any number of
    /// leading axiom annotations. Every OWL 2 axiom production accepts
    /// zero or more `Annotation(property value)` parts before its
    /// arguments; the ORE corpus exercises them on `SubClassOf`,
    /// `ObjectPropertyDomain` and `ObjectPropertyRange`. Annotations
    /// are non-logical by the structural specification and the
    /// decorated axiom carries no reference to them, so they are
    /// consumed without model emission.
    fn open_axiom(&mut self, keyword: &str) -> FunctionalSyntaxResult<()> {
        self.consume(
            TokenType::LeftParen,
            &format!("Expected '(' after {}", keyword),
        )?;
        while self.peek().token_type == TokenType::Identifier
            && self.peek().lexeme == "Annotation"
        {
            self.advance();
            self.consume(TokenType::LeftParen, "Expected '(' after Annotation")?;
            self.skip_balanced_construct("Annotation")?;
        }
        Ok(())
    }

    /// Skip tokens up to and including the closing parenthesis that
    /// balances a construct whose opening parenthesis was already
    /// consumed. String literals and IRIs tokenize atomically, so
    /// parenthesized characters inside them cannot unbalance the
    /// count.
    fn skip_balanced_construct(&mut self, construct: &str) -> FunctionalSyntaxResult<()> {
        let mut depth = 1usize;
        while depth > 0 {
            if self.is_at_end() {
                return Err(crate::parser::owl_functional::error::grammar_error(format!(
                    "Unexpected end of input inside {}",
                    construct
                )));
            }
            match self.advance().token_type {
                TokenType::LeftParen => depth += 1,
                TokenType::RightParen => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// Parse and skip a DL-safe rule:
    /// `DLSafeRule(Annotation* Body(atoms) Head(atoms))`.
    ///
    /// DL-safe rules appear in the ORE corpus. Tableauxx has no
    /// rule-engine wiring for SWRL rules, so the rule block is
    /// consumed structurally and skipped with a warning instead of
    /// failing the whole document: every DL axiom of the document
    /// stays parseable, and rule-derived inferences are
    /// under-approximated (the comparative lane adjudicates no
    /// correctness, ADR 0024).
    fn parse_dl_safe_rule(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.consume(TokenType::LeftParen, "Expected '(' after DLSafeRule")?;
        self.skip_balanced_construct("DLSafeRule")?;
        self.skipped_dl_safe_rules += 1;
        Ok(Vec::new())
    }

    /// Parse SubClassOf axiom
    fn parse_subclass_of_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("SubClassOf")?;

        let sub_class = self.parse_class_expression()?;
        let super_class = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after SubClassOf axiom",
        )?;

        let axiom = Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(sub_class, super_class)));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse the class-expression argument list shared by
    /// EquivalentClasses, DisjointClasses and DisjointUnion members.
    /// Whitespace is not tokenized and an optional comma between
    /// arguments is tolerated.
    fn parse_class_expression_list(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<crate::logic::axioms::class_expressions::ClassExpression>> {
        let mut expressions = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            expressions.push(self.parse_class_expression()?);
            self.match_token(TokenType::Comma);
        }
        Ok(expressions)
    }

    /// Parse EquivalentClasses axiom.
    ///
    /// All-named argument lists feed the n-ary `EquivalentClassesAxiom`
    /// directly; any complex argument lowers to bidirectional
    /// `SubClassOf` pairs over adjacent arguments — the same
    /// materialization the RDF/XML parser applies to deferred
    /// equivalentClass edges.
    fn parse_equivalent_classes_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("EquivalentClasses")?;

        let expressions = self.parse_class_expression_list()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after EquivalentClasses axiom",
        )?;

        if expressions.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "EquivalentClasses requires at least 2 classes".to_string(),
            ));
        }

        let named: Option<Vec<Arc<IRI>>> = expressions
            .iter()
            .map(named_iri_of)
            .collect();

        if let Some(class_iris) = named {
            let axiom =
                Axiom::EquivalentClasses(Box::new(EquivalentClassesAxiom::new(class_iris)));
            return Ok(vec![OntologyContent::Axiom(axiom)]);
        }

        let mut contents = Vec::new();
        for pair in expressions.windows(2) {
            let left = pair[0].clone();
            let right = pair[1].clone();
            contents.push(OntologyContent::Axiom(Axiom::SubClassOf(Box::new(
                SubClassOfAxiom::new(left.clone(), right.clone()),
            ))));
            contents.push(OntologyContent::Axiom(Axiom::SubClassOf(Box::new(
                SubClassOfAxiom::new(right, left),
            ))));
        }
        Ok(contents)
    }

    /// Parse DisjointClasses axiom.
    ///
    /// All-named argument lists feed the n-ary `DisjointClassesAxiom`
    /// directly; any complex argument lowers each pair to the GCI
    /// `Ci and Cj subClassOf owl:Nothing`, the general shape of
    /// disjointness.
    fn parse_disjoint_classes_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DisjointClasses")?;

        let expressions = self.parse_class_expression_list()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DisjointClasses axiom",
        )?;

        if expressions.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "DisjointClasses requires at least 2 classes".to_string(),
            ));
        }

        self.lower_disjoint_expressions(expressions)
    }

    /// Lower a disjointness over class expressions: named-only lists
    /// keep the n-ary axiom; otherwise each pair lowers to the GCI
    /// `Ci and Cj subClassOf owl:Nothing`.
    fn lower_disjoint_expressions(
        &mut self,
        expressions: Vec<crate::logic::axioms::class_expressions::ClassExpression>,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        let named: Option<Vec<Arc<IRI>>> = expressions
            .iter()
            .map(named_iri_of)
            .collect();

        if let Some(class_iris) = named {
            let axiom = Axiom::DisjointClasses(Box::new(DisjointClassesAxiom::new(class_iris)));
            return Ok(vec![OntologyContent::Axiom(axiom)]);
        }

        let nothing = owl_nothing_expression()?;
        let mut contents = Vec::new();
        for i in 0..expressions.len() {
            for expression in expressions.iter().skip(i + 1) {
                let pair = smallvec::SmallVec::from_vec(vec![
                    Box::new(expressions[i].clone()),
                    Box::new(expression.clone()),
                ]);
                contents.push(OntologyContent::Axiom(Axiom::SubClassOf(Box::new(
                    SubClassOfAxiom::new(
                        crate::logic::axioms::class_expressions::ClassExpression::ObjectIntersectionOf(
                            pair,
                        ),
                        nothing.clone(),
                    ),
                ))));
            }
        }
        Ok(contents)
    }

    /// Parse DisjointUnion axiom.
    ///
    /// The model has no DisjointUnion type, so this mirrors the
    /// RDF/XML streaming parser's materialization: the named class is
    /// made equivalent to the union of the members, and the members
    /// are declared pairwise disjoint.
    fn parse_disjoint_union_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DisjointUnion")?;

        let class = self.parse_class()?;
        let members = self.parse_class_expression_list()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DisjointUnion axiom",
        )?;

        if members.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "DisjointUnion requires at least 2 classes".to_string(),
            ));
        }

        let named = crate::logic::axioms::class_expressions::ClassExpression::Class(class);
        let union = crate::logic::axioms::class_expressions::ClassExpression::ObjectUnionOf(
            smallvec::SmallVec::from_vec(members.clone().into_iter().map(Box::new).collect()),
        );

        let mut contents = vec![
            OntologyContent::Axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                named.clone(),
                union.clone(),
            )))),
            OntologyContent::Axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                union, named,
            )))),
        ];
        contents.extend(self.lower_disjoint_expressions(members)?);
        Ok(contents)
    }

    /// Parse SubObjectPropertyOf axiom, including the
    /// `ObjectPropertyChain(p1 ... pn) super` form.
    fn parse_sub_object_property_of_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("SubObjectPropertyOf")?;

        if self.match_token(TokenType::ObjectPropertyChain) {
            self.consume(
                TokenType::LeftParen,
                "Expected '(' after ObjectPropertyChain",
            )?;

            let mut chain = Vec::new();
            while !self.check(TokenType::RightParen) && !self.is_at_end() {
                let member = self.parse_object_property_expression()?;
                chain.push(self.chain_member_iri(member)?);
            }
            self.consume(
                TokenType::RightParen,
                "Expected ')' after ObjectPropertyChain",
            )?;

            if chain.is_empty() {
                return Err(crate::parser::owl_functional::error::grammar_error(
                    "ObjectPropertyChain requires at least one property".to_string(),
                ));
            }

            let super_prop = self.parse_object_property_expression()?.to_iri()?;
            self.consume(
                TokenType::RightParen,
                "Expected ')' after SubObjectPropertyOf axiom",
            )?;

            let axiom = Axiom::SubPropertyChainOf(Box::new(SubPropertyChainOfAxiom::new(
                chain,
                super_prop,
            )));
            return Ok(vec![OntologyContent::Axiom(axiom)]);
        }

        let sub_prop = self.parse_object_property_expression()?.to_iri()?;
        let super_prop = self.parse_object_property_expression()?.to_iri()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after SubObjectPropertyOf axiom",
        )?;

        let axiom =
            Axiom::SubObjectProperty(Box::new(SubObjectPropertyAxiom::new(sub_prop, super_prop)));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Map a chain member property expression to its chain IRI.
    /// Inverse members mint the same auxiliary role as the RDF/XML
    /// parser (`urn:tableauxx:inverse-role:<fnv1a>`) declared inverse
    /// to the wrapped property.
    fn chain_member_iri(
        &mut self,
        member: crate::logic::axioms::property_expressions::ObjectPropertyExpression,
    ) -> FunctionalSyntaxResult<Arc<IRI>> {
        match member {
            crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectProperty(
                prop,
            ) => Ok(prop.iri().clone()),
            crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectInverseOf(
                inner,
            ) => {
                let wrapped = self.chain_member_iri(*inner)?;
                let auxiliary = inverse_property_expression_role(&wrapped)?;
                if !self
                    .inverse_roles
                    .iter()
                    .any(|(aux, _)| aux.as_ref() == auxiliary.as_ref())
                {
                    self.inverse_roles.push((auxiliary.clone(), wrapped));
                }
                Ok(auxiliary)
            }
        }
    }

    /// Parse EquivalentObjectProperties axiom
    fn parse_equivalent_object_properties_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("EquivalentObjectProperties")?;

        let properties = self.parse_object_property_iri_list("EquivalentObjectProperties")?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after EquivalentObjectProperties axiom",
        )?;

        let axiom = Axiom::EquivalentObjectProperties(Box::new(
            EquivalentObjectPropertiesAxiom::new(properties),
        ));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse DisjointObjectProperties axiom
    fn parse_disjoint_object_properties_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DisjointObjectProperties")?;

        let properties = self.parse_object_property_iri_list("DisjointObjectProperties")?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DisjointObjectProperties axiom",
        )?;

        let axiom = Axiom::DisjointObjectProperties(Box::new(
            DisjointObjectPropertiesAxiom::new(properties),
        ));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse an object-property argument list (at least two).
    fn parse_object_property_iri_list(
        &mut self,
        axiom_name: &str,
    ) -> FunctionalSyntaxResult<Vec<Arc<IRI>>> {
        let mut properties = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            properties.push(self.parse_object_property_expression()?.to_iri()?);
            self.match_token(TokenType::Comma);
        }
        if properties.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(format!(
                "{} requires at least 2 properties",
                axiom_name
            )));
        }
        Ok(properties)
    }

    /// Parse ObjectPropertyDomain axiom.
    ///
    /// Named-class domains feed `ObjectPropertyDomainAxiom` directly;
    /// complex domains lower to the existential GCI
    /// `exists p. Thing subClassOf C`, the domain reading that mirrors
    /// the RDF/XML parser's inverse-existential range lowering.
    fn parse_object_property_domain_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("ObjectPropertyDomain")?;

        let prop = self.parse_object_property_expression()?;
        let domain_expr = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectPropertyDomain axiom",
        )?;

        match named_iri_of(&domain_expr) {
            Some(domain_iri) => {
                let axiom = Axiom::ObjectPropertyDomain(Box::new(ObjectPropertyDomainAxiom::new(
                    prop.to_iri()?,
                    domain_iri,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
            None => {
                let thing = owl_thing_expression()?;
                let existential =
                    crate::logic::axioms::class_expressions::ClassExpression::ObjectSomeValuesFrom(
                        Box::new(prop),
                        Box::new(thing),
                    );
                let axiom = Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    existential,
                    domain_expr,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
        }
    }

    /// Parse ObjectPropertyRange axiom.
    ///
    /// Named-class ranges feed `ObjectPropertyRangeAxiom` directly;
    /// complex ranges lower to the inverse-existential GCI
    /// `exists p-. Thing subClassOf C` — the identical lowering the
    /// RDF/XML streaming parser applies to deferred range edges (#150).
    fn parse_object_property_range_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("ObjectPropertyRange")?;

        let prop = self.parse_object_property_expression()?;
        let range_expr = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectPropertyRange axiom",
        )?;

        match named_iri_of(&range_expr) {
            Some(range_iri) => {
                let axiom = Axiom::ObjectPropertyRange(Box::new(ObjectPropertyRangeAxiom::new(
                    Arc::new((*prop.to_iri()?).clone()),
                    range_iri,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
            None => {
                let thing = owl_thing_expression()?;
                let inverse =
                    crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectInverseOf(
                        Box::new(prop),
                    );
                let existential =
                    crate::logic::axioms::class_expressions::ClassExpression::ObjectSomeValuesFrom(
                        Box::new(inverse),
                        Box::new(thing),
                    );
                let axiom = Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    existential,
                    range_expr,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
        }
    }

    /// Parse ClassAssertion axiom.
    ///
    /// OWL 2 functional syntax order is `ClassAssertion(C I)`: class
    /// expression first, individual second (spec section 2.1:
    /// `ClassAssertion(a:Person a:Peter)`). Anonymous individuals are
    /// minted as `_:nodeid` IRIs exactly like the RDF/XML parser's
    /// blank-node individuals.
    fn parse_class_assertion_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("ClassAssertion")?;

        let class_expr = self.parse_class_expression()?;
        let individual = self.parse_individual()?;
        let individual_iri = self.individual_iri_or_mint(individual, "ClassAssertion")?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ClassAssertion axiom",
        )?;

        let axiom =
            Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(individual_iri, class_expr)));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse ObjectPropertyAssertion axiom:
    /// `ObjectPropertyAssertion(p source target)`. An anonymous target
    /// feeds `PropertyAssertionAxiom::new_with_anonymous`; an anonymous
    /// source mints a `_:nodeid` IRI (the model's subject slot is an
    /// IRI).
    fn parse_object_property_assertion_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("ObjectPropertyAssertion")?;

        let prop = self.parse_object_property_expression()?.to_iri()?;
        let source_individual = self.parse_individual()?;
        let source =
            self.individual_iri_or_mint(source_individual, "ObjectPropertyAssertion")?;
        let target = self.parse_individual()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectPropertyAssertion axiom",
        )?;

        let axiom = match target {
            crate::core::entities::Individual::Named(named) => {
                Axiom::PropertyAssertion(Box::new(PropertyAssertionAxiom::new(
                    source,
                    prop,
                    named.iri().clone(),
                )))
            }
            crate::core::entities::Individual::Anonymous(anon) => {
                Axiom::PropertyAssertion(Box::new(PropertyAssertionAxiom::new_with_anonymous(
                    source, prop, anon,
                )))
            }
        };
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse DataPropertyAssertion axiom:
    /// `DataPropertyAssertion(dp source literal)`.
    fn parse_data_property_assertion_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DataPropertyAssertion")?;

        let prop = self.parse_data_property_expression()?;
        let prop_iri = data_property_iri(&prop);
        let source_individual = self.parse_individual()?;
        let source = self.individual_iri_or_mint(source_individual, "DataPropertyAssertion")?;
        let value = self.parse_literal()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataPropertyAssertion axiom",
        )?;

        let axiom = Axiom::DataPropertyAssertion(Box::new(DataPropertyAssertionAxiom::new(
            source, prop_iri, value,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse NegativeObjectPropertyAssertion axiom. The model's object
    /// slot only carries named individuals, so an anonymous target
    /// mints a `_:nodeid` IRI.
    fn parse_negative_object_property_assertion_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("NegativeObjectPropertyAssertion")?;

        let prop = self.parse_object_property_expression()?.to_iri()?;
        let source_individual = self.parse_individual()?;
        let source = self.individual_iri_or_mint(
            source_individual,
            "NegativeObjectPropertyAssertion",
        )?;
        let target_individual = self.parse_individual()?;
        let target = self.individual_iri_or_mint(
            target_individual,
            "NegativeObjectPropertyAssertion",
        )?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after NegativeObjectPropertyAssertion axiom",
        )?;

        let axiom = Axiom::NegativeObjectPropertyAssertion(Box::new(
            NegativeObjectPropertyAssertionAxiom::new(source, prop, target),
        ));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse SameIndividual axiom
    fn parse_same_individual_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("SameIndividual")?;

        let mut individuals = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            let individual = self.parse_individual()?;
            individuals.push(self.individual_iri_or_mint(individual, "SameIndividual")?);
            self.match_token(TokenType::Comma);
        }
        if individuals.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "SameIndividual requires at least 2 individuals".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after SameIndividual axiom",
        )?;

        let axiom = Axiom::SameIndividual(Box::new(SameIndividualAxiom::new(individuals)));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse DifferentIndividuals axiom
    fn parse_different_individuals_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DifferentIndividuals")?;

        let mut individuals = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            let individual = self.parse_individual()?;
            individuals.push(self.individual_iri_or_mint(individual, "DifferentIndividuals")?);
            self.match_token(TokenType::Comma);
        }
        if individuals.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "DifferentIndividuals requires at least 2 individuals".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DifferentIndividuals axiom",
        )?;

        let axiom = Axiom::DifferentIndividuals(Box::new(DifferentIndividualsAxiom::new(
            individuals,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse SubDataPropertyOf axiom
    fn parse_sub_data_property_of_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("SubDataPropertyOf")?;

        let sub_prop = data_property_iri(&self.parse_data_property_expression()?);
        let super_prop = data_property_iri(&self.parse_data_property_expression()?);

        self.consume(
            TokenType::RightParen,
            "Expected ')' after SubDataPropertyOf axiom",
        )?;

        let axiom =
            Axiom::SubDataProperty(Box::new(SubDataPropertyOfAxiom::new(sub_prop, super_prop)));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse EquivalentDataProperties axiom
    fn parse_equivalent_data_properties_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("EquivalentDataProperties")?;

        let mut properties = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            properties.push(data_property_iri(&self.parse_data_property_expression()?));
            self.match_token(TokenType::Comma);
        }
        if properties.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "EquivalentDataProperties requires at least 2 properties".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after EquivalentDataProperties axiom",
        )?;

        let axiom = Axiom::EquivalentDataProperties(Box::new(EquivalentDataPropertiesAxiom::new(
            properties,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse DisjointDataProperties axiom
    fn parse_disjoint_data_properties_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DisjointDataProperties")?;

        let mut properties = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            properties.push(data_property_iri(&self.parse_data_property_expression()?));
            self.match_token(TokenType::Comma);
        }
        if properties.len() < 2 {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "DisjointDataProperties requires at least 2 properties".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DisjointDataProperties axiom",
        )?;

        let axiom = Axiom::DisjointDataProperties(Box::new(DisjointDataPropertiesAxiom::new(
            properties,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse DataPropertyDomain axiom.
    ///
    /// Named-class domains feed `DataPropertyDomainAxiom`; complex
    /// domains lower to `exists dp. rdfs:Literal subClassOf C`.
    fn parse_data_property_domain_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DataPropertyDomain")?;

        let prop = self.parse_data_property_expression()?;
        let domain_expr = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataPropertyDomain axiom",
        )?;

        match named_iri_of(&domain_expr) {
            Some(domain_iri) => {
                let axiom = Axiom::DataPropertyDomain(Box::new(DataPropertyDomainAxiom::new(
                    data_property_iri(&prop),
                    domain_iri,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
            None => {
                let literal_range = crate::logic::axioms::class_expressions::DataRange::Datatype(
                    IRI::new(NS_RDFS_LITERAL).map_err(|e| {
                        crate::parser::owl_functional::error::invalid_iri_error(format!(
                            "Invalid rdfs:Literal IRI: {}",
                            e
                        ))
                    })?,
                );
                let existential =
                    crate::logic::axioms::class_expressions::ClassExpression::DataSomeValuesFrom(
                        Box::new(prop),
                        Box::new(literal_range),
                    );
                let axiom = Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                    existential,
                    domain_expr,
                )));
                Ok(vec![OntologyContent::Axiom(axiom)])
            }
        }
    }

    /// Parse DataPropertyRange axiom.
    ///
    /// A datatype feeds `DataPropertyRangeAxiom`; a `DataOneOf`
    /// enumeration feeds `DataPropertyEnumeratedRangeAxiom`; a
    /// `DatatypeRestriction` lowers to its base datatype,
    /// under-approximating the facets — the same split the RDF/XML
    /// parser's deferred range materialization applies (#150).
    /// Compound data ranges have no axiom form and fail with an
    /// explicit grammar error.
    fn parse_data_property_range_axiom(&mut self) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("DataPropertyRange")?;

        let prop_iri = data_property_iri(&self.parse_data_property_expression()?);
        let range = self.parse_data_range()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataPropertyRange axiom",
        )?;

        let axiom = match range {
            crate::logic::axioms::class_expressions::DataRange::Datatype(datatype) => {
                Axiom::DataPropertyRange(Box::new(DataPropertyRangeAxiom::new(
                    prop_iri,
                    Arc::new(datatype),
                )))
            }
            crate::logic::axioms::class_expressions::DataRange::DataOneOf(values) => {
                Axiom::DataPropertyEnumeratedRange(Box::new(
                    DataPropertyEnumeratedRangeAxiom::new(prop_iri, values),
                ))
            }
            crate::logic::axioms::class_expressions::DataRange::DatatypeRestriction(
                datatype,
                _facets,
            ) => Axiom::DataPropertyRange(Box::new(DataPropertyRangeAxiom::new(
                prop_iri,
                Arc::new(datatype),
            ))),
            _ => {
                return Err(crate::parser::owl_functional::error::grammar_error(
                    "Compound data property ranges have no axiom form yet".to_string(),
                ))
            }
        };
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse FunctionalDataProperty axiom
    fn parse_functional_data_property_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("FunctionalDataProperty")?;

        let prop_iri = data_property_iri(&self.parse_data_property_expression()?);

        self.consume(
            TokenType::RightParen,
            "Expected ')' after FunctionalDataProperty axiom",
        )?;

        let axiom = Axiom::FunctionalDataProperty(Box::new(FunctionalDataPropertyAxiom::new(
            prop_iri,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse a property characteristic axiom: a single object property
    /// wrapped in the axiom's constructor.
    fn parse_property_characteristic_axiom(
        &mut self,
        axiom_name: &str,
        build: impl Fn(Arc<IRI>) -> Axiom,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom(axiom_name)?;

        let property = self.parse_object_property_expression()?.to_iri()?;

        self.consume(
            TokenType::RightParen,
            &format!("Expected ')' after {} axiom", axiom_name),
        )?;

        Ok(vec![OntologyContent::Axiom(build(property))])
    }

    /// Parse InverseObjectProperties axiom
    fn parse_inverse_object_properties_axiom(
        &mut self,
    ) -> FunctionalSyntaxResult<Vec<OntologyContent>> {
        self.open_axiom("InverseObjectProperties")?;

        let first_property = self.parse_object_property_expression()?;
        let second_property = self.parse_object_property_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after InverseObjectProperties axiom",
        )?;

        let axiom = Axiom::InverseObjectProperties(Box::new(InverseObjectPropertiesAxiom::new(
            first_property.to_iri()?,
            second_property.to_iri()?,
        )));
        Ok(vec![OntologyContent::Axiom(axiom)])
    }

    /// Parse a class expression
    fn parse_class_expression(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        let token = self.peek().clone();
        match token.token_type {
            TokenType::Class => {
                self.advance();
                let class = self.parse_class()?;
                Ok(ClassExpression::Class(class))
            }
            TokenType::ObjectIntersectionOf => {
                self.advance();
                self.parse_object_intersection_of()
            }
            TokenType::ObjectUnionOf => {
                self.advance();
                self.parse_object_union_of()
            }
            TokenType::ObjectComplementOf => {
                self.advance();
                self.parse_object_complement_of()
            }
            TokenType::ObjectSomeValuesFrom => {
                self.advance();
                self.parse_object_some_values_from()
            }
            TokenType::ObjectAllValuesFrom => {
                self.advance();
                self.parse_object_all_values_from()
            }
            TokenType::ObjectHasValue => {
                self.advance();
                self.parse_object_has_value()
            }
            TokenType::ObjectHasSelf => {
                self.advance();
                self.parse_object_has_self()
            }
            TokenType::ObjectMinCardinality => {
                self.advance();
                self.parse_object_cardinality(ObjectCardinalityKind::Min)
            }
            TokenType::ObjectMaxCardinality => {
                self.advance();
                self.parse_object_cardinality(ObjectCardinalityKind::Max)
            }
            TokenType::ObjectExactCardinality => {
                self.advance();
                self.parse_object_cardinality(ObjectCardinalityKind::Exact)
            }
            TokenType::ObjectOneOf => {
                self.advance();
                self.parse_object_one_of()
            }
            TokenType::DataSomeValuesFrom => {
                self.advance();
                self.parse_data_some_values_from()
            }
            TokenType::DataAllValuesFrom => {
                self.advance();
                self.parse_data_all_values_from()
            }
            TokenType::DataHasValue => {
                self.advance();
                self.parse_data_has_value()
            }
            TokenType::DataMinCardinality => {
                self.advance();
                self.parse_data_cardinality(DataCardinalityKind::Min)
            }
            TokenType::DataMaxCardinality => {
                self.advance();
                self.parse_data_cardinality(DataCardinalityKind::Max)
            }
            TokenType::DataExactCardinality => {
                self.advance();
                self.parse_data_cardinality(DataCardinalityKind::Exact)
            }
            TokenType::IRI => {
                // Direct IRI reference like <http://...>
                let iri = self.resolve_iri(&token.lexeme)?;
                self.advance();
                Ok(ClassExpression::Class(Class::from_shared_iri(iri)))
            }
            TokenType::Colon => {
                // Prefixed name like :A (default prefix)
                let iri = self.parse_iri_reference("class expression")?;
                Ok(ClassExpression::Class(Class::from_shared_iri(iri)))
            }
            TokenType::Identifier => {
                // Prefixed name like owl:Thing
                let iri = self.parse_iri_reference("class expression")?;
                Ok(ClassExpression::Class(Class::from_shared_iri(iri)))
            }
            _ => Err(crate::parser::owl_functional::error::grammar_error(
                format!("Expected class expression, found: {}", token.lexeme),
            )),
        }
    }

    /// Parse ObjectIntersectionOf expression
    fn parse_object_intersection_of(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after ObjectIntersectionOf",
        )?;

        let mut operands = Vec::new();

        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            operands.push(self.parse_class_expression()?);
            self.match_token(TokenType::Comma);
        }

        if operands.is_empty() {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "ObjectIntersectionOf requires at least one operand".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectIntersectionOf",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::ObjectIntersectionOf(
                smallvec::SmallVec::from_vec(operands.into_iter().map(Box::new).collect()),
            ),
        )
    }

    /// Parse ObjectUnionOf expression
    fn parse_object_union_of(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(TokenType::LeftParen, "Expected '(' after ObjectUnionOf")?;

        let mut operands = Vec::new();

        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            operands.push(self.parse_class_expression()?);
            self.match_token(TokenType::Comma);
        }

        if operands.is_empty() {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "ObjectUnionOf requires at least one operand".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectUnionOf",
        )?;

        Ok(crate::logic::axioms::class_expressions::ClassExpression::ObjectUnionOf(
            smallvec::SmallVec::from_vec(operands.into_iter().map(Box::new).collect()),
        ))
    }

    /// Parse ObjectComplementOf expression
    fn parse_object_complement_of(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after ObjectComplementOf",
        )?;

        let operand = Box::new(self.parse_class_expression()?);

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectComplementOf",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::ObjectComplementOf(
                operand,
            ),
        )
    }

    /// Parse ObjectSomeValuesFrom expression
    fn parse_object_some_values_from(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after ObjectSomeValuesFrom",
        )?;

        let property = self.parse_object_property_expression()?;
        let class_expr = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectSomeValuesFrom",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::ObjectSomeValuesFrom(
                Box::new(property),
                Box::new(class_expr),
            ),
        )
    }

    /// Parse ObjectAllValuesFrom expression
    fn parse_object_all_values_from(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after ObjectAllValuesFrom",
        )?;

        let property = self.parse_object_property_expression()?;
        let class_expr = self.parse_class_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectAllValuesFrom",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::ObjectAllValuesFrom(
                Box::new(property),
                Box::new(class_expr),
            ),
        )
    }

    /// Parse ObjectHasValue expression; the value individual may be
    /// anonymous (`_:genid` in the corpus).
    fn parse_object_has_value(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(TokenType::LeftParen, "Expected '(' after ObjectHasValue")?;

        let property = self.parse_object_property_expression()?;
        let individual = self.parse_individual()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectHasValue",
        )?;

        Ok(crate::logic::axioms::class_expressions::ClassExpression::ObjectHasValue(
            Box::new(property),
            individual,
        ))
    }

    /// Parse ObjectHasSelf expression
    fn parse_object_has_self(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(TokenType::LeftParen, "Expected '(' after ObjectHasSelf")?;

        let property = self.parse_object_property_expression()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectHasSelf",
        )?;

        Ok(crate::logic::axioms::class_expressions::ClassExpression::ObjectHasSelf(
            Box::new(property),
        ))
    }

    /// Parse an object cardinality restriction. The qualifier class
    /// expression is optional (unqualified cardinalities count all
    /// role successors).
    fn parse_object_cardinality(
        &mut self,
        kind: ObjectCardinalityKind,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        let name = kind.name();
        self.consume(
            TokenType::LeftParen,
            &format!("Expected '(' after {}", name),
        )?;

        let cardinality = self.parse_cardinality_number(name)?;
        let property = self.parse_object_property_expression()?;
        let qualifier = if self.check(TokenType::RightParen) {
            None
        } else {
            Some(Box::new(self.parse_class_expression()?))
        };

        self.consume(
            TokenType::RightParen,
            &format!("Expected ')' after {}", name),
        )?;

        Ok(match kind {
            ObjectCardinalityKind::Min => {
                ClassExpression::ObjectMinCardinality(cardinality, Box::new(property), qualifier)
            }
            ObjectCardinalityKind::Max => {
                ClassExpression::ObjectMaxCardinality(cardinality, Box::new(property), qualifier)
            }
            ObjectCardinalityKind::Exact => {
                ClassExpression::ObjectExactCardinality(cardinality, Box::new(property), qualifier)
            }
        })
    }

    /// Parse a data cardinality restriction; the data range qualifier
    /// is optional.
    fn parse_data_cardinality(
        &mut self,
        kind: DataCardinalityKind,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        let name = kind.name();
        self.consume(
            TokenType::LeftParen,
            &format!("Expected '(' after {}", name),
        )?;

        let cardinality = self.parse_cardinality_number(name)?;
        let property = self.parse_data_property_expression()?;
        let qualifier = if self.check(TokenType::RightParen) {
            None
        } else {
            Some(Box::new(self.parse_data_range()?))
        };

        self.consume(
            TokenType::RightParen,
            &format!("Expected ')' after {}", name),
        )?;

        Ok(match kind {
            DataCardinalityKind::Min => {
                ClassExpression::DataMinCardinality(cardinality, Box::new(property), qualifier)
            }
            DataCardinalityKind::Max => {
                ClassExpression::DataMaxCardinality(cardinality, Box::new(property), qualifier)
            }
            DataCardinalityKind::Exact => {
                ClassExpression::DataExactCardinality(cardinality, Box::new(property), qualifier)
            }
        })
    }

    /// Parse the non-negative integer cardinality of a cardinality
    /// restriction.
    fn parse_cardinality_number(&mut self, name: &str) -> FunctionalSyntaxResult<u32> {
        let token = self.peek().clone();
        if token.token_type == TokenType::NumberLiteral {
            self.advance();
            let trimmed = token.lexeme.trim_start_matches('+');
            trimmed.parse::<u32>().map_err(|_| {
                crate::parser::owl_functional::error::grammar_error(format!(
                    "Expected non-negative integer cardinality in {}, found: {}",
                    name, token.lexeme
                ))
            })
        } else {
            Err(crate::parser::owl_functional::error::grammar_error(format!(
                "Expected cardinality number in {}, found: {}",
                name, token.lexeme
            )))
        }
    }

    /// Parse ObjectOneOf expression: a list of individuals (named or
    /// anonymous).
    fn parse_object_one_of(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(TokenType::LeftParen, "Expected '(' after ObjectOneOf")?;

        let mut individuals = smallvec::SmallVec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            individuals.push(self.parse_individual()?);
            self.match_token(TokenType::Comma);
        }

        if individuals.is_empty() {
            return Err(crate::parser::owl_functional::error::grammar_error(
                "ObjectOneOf requires at least one individual".to_string(),
            ));
        }

        self.consume(
            TokenType::RightParen,
            "Expected ')' after ObjectOneOf",
        )?;

        Ok(crate::logic::axioms::class_expressions::ClassExpression::ObjectOneOf(
            Box::new(individuals),
        ))
    }

    /// Parse DataSomeValuesFrom expression
    fn parse_data_some_values_from(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after DataSomeValuesFrom",
        )?;

        let property = self.parse_data_property_expression()?;
        let data_range = self.parse_data_range()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataSomeValuesFrom",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::DataSomeValuesFrom(
                Box::new(property),
                Box::new(data_range),
            ),
        )
    }

    /// Parse DataAllValuesFrom expression
    fn parse_data_all_values_from(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(
            TokenType::LeftParen,
            "Expected '(' after DataAllValuesFrom",
        )?;

        let property = self.parse_data_property_expression()?;
        let data_range = self.parse_data_range()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataAllValuesFrom",
        )?;

        Ok(
            crate::logic::axioms::class_expressions::ClassExpression::DataAllValuesFrom(
                Box::new(property),
                Box::new(data_range),
            ),
        )
    }

    /// Parse DataHasValue expression
    fn parse_data_has_value(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
        self.consume(TokenType::LeftParen, "Expected '(' after DataHasValue")?;

        let property = self.parse_data_property_expression()?;
        let value = self.parse_literal()?;

        self.consume(
            TokenType::RightParen,
            "Expected ')' after DataHasValue",
        )?;

        Ok(crate::logic::axioms::class_expressions::ClassExpression::DataHasValue(
            Box::new(property),
            value,
        ))
    }

    /// Parse a data range: a datatype reference (full IRI or prefixed
    /// name such as xsd:integer), or one of the DataOneOf /
    /// DataIntersectionOf / DataUnionOf / DataComplementOf
    /// constructors. `DatatypeRestriction` parses with its facet
    /// restrictions retained; in `DataPropertyRange` position it
    /// under-approximates to its base datatype.
    fn parse_data_range(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::DataRange> {
        use crate::logic::axioms::class_expressions::DataRange;

        let token = self.peek().clone();
        match token.token_type {
            TokenType::IRI | TokenType::Colon | TokenType::Identifier => {
                let iri = self.parse_iri_reference("datatype")?;
                Ok(DataRange::Datatype((*iri).clone()))
            }
            TokenType::DataOneOf => {
                self.advance();
                self.consume(TokenType::LeftParen, "Expected '(' after DataOneOf")?;
                let mut values = Vec::new();
                while !self.check(TokenType::RightParen) && !self.is_at_end() {
                    values.push(self.parse_literal()?);
                    self.match_token(TokenType::Comma);
                }
                self.consume(
                    TokenType::RightParen,
                    "Expected ')' after DataOneOf",
                )?;
                if values.is_empty() {
                    return Err(crate::parser::owl_functional::error::grammar_error(
                        "DataOneOf requires at least one literal".to_string(),
                    ));
                }
                Ok(DataRange::DataOneOf(values))
            }
            TokenType::DataIntersectionOf => {
                self.advance();
                let ranges = self.parse_data_range_list("DataIntersectionOf")?;
                Ok(DataRange::DataIntersectionOf(ranges))
            }
            TokenType::DataUnionOf => {
                self.advance();
                let ranges = self.parse_data_range_list("DataUnionOf")?;
                Ok(DataRange::DataUnionOf(ranges))
            }
            TokenType::DataComplementOf => {
                self.advance();
                self.consume(
                    TokenType::LeftParen,
                    "Expected '(' after DataComplementOf",
                )?;
                let inner = self.parse_data_range()?;
                self.consume(
                    TokenType::RightParen,
                    "Expected ')' after DataComplementOf",
                )?;
                Ok(DataRange::DataComplementOf(Box::new(inner)))
            }
            TokenType::DatatypeRestriction => {
                self.advance();
                self.consume(
                    TokenType::LeftParen,
                    "Expected '(' after DatatypeRestriction",
                )?;
                let datatype = self.parse_iri_reference("datatype")?;
                let mut facets = Vec::new();
                while !self.check(TokenType::RightParen) && !self.is_at_end() {
                    let facet = self.parse_iri_reference("constraining facet")?;
                    let value = self.parse_literal()?;
                    facets.push(
                        crate::logic::axioms::class_expressions::FacetRestriction::new(
                            (*facet).clone(),
                            value,
                        ),
                    );
                    self.match_token(TokenType::Comma);
                }
                self.consume(
                    TokenType::RightParen,
                    "Expected ')' after DatatypeRestriction",
                )?;
                if facets.is_empty() {
                    return Err(crate::parser::owl_functional::error::grammar_error(
                        "DatatypeRestriction requires at least one facet".to_string(),
                    ));
                }
                Ok(DataRange::DatatypeRestriction((*datatype).clone(), facets))
            }
            _ => Err(crate::parser::owl_functional::error::grammar_error(format!(
                "Expected data range, found: {}",
                token.lexeme
            ))),
        }
    }

    /// Parse a parenthesized data-range list.
    fn parse_data_range_list(
        &mut self,
        name: &str,
    ) -> FunctionalSyntaxResult<Vec<crate::logic::axioms::class_expressions::DataRange>> {
        self.consume(TokenType::LeftParen, &format!("Expected '(' after {}", name))?;
        let mut ranges = Vec::new();
        while !self.check(TokenType::RightParen) && !self.is_at_end() {
            ranges.push(self.parse_data_range()?);
            self.match_token(TokenType::Comma);
        }
        self.consume(
            TokenType::RightParen,
            &format!("Expected ')' after {}", name),
        )?;
        if ranges.is_empty() {
            return Err(crate::parser::owl_functional::error::grammar_error(format!(
                "{} requires at least one data range",
                name
            )));
        }
        Ok(ranges)
    }

    /// Parse a literal: plain `"..."`, language-tagged `"..."@en`, or
    /// typed `"..."^^datatype` with a full or prefixed datatype IRI.
    fn parse_literal(&mut self) -> FunctionalSyntaxResult<crate::core::entities::Literal> {
        let token = self.peek().clone();
        if token.token_type != TokenType::StringLiteral {
            return Err(crate::parser::owl_functional::error::grammar_error(format!(
                "Expected literal, found: {}",
                token.lexeme
            )));
        }
        self.advance();
        let lexical = token.lexeme[1..token.lexeme.len() - 1].to_string();

        // Typed literal: ^^datatype
        if self.match_token(TokenType::Caret) {
            self.consume(
                TokenType::Caret,
                "Expected '^^' before literal datatype",
            )?;
            let datatype_iri = self.parse_iri_reference("literal datatype")?;
            return Ok(crate::core::entities::Literal::typed(
                lexical,
                (*datatype_iri).clone(),
            ));
        }

        // Language-tagged literal: @lang
        if self.match_token(TokenType::At) {
            let lang = self.peek().clone();
            if lang.token_type != TokenType::Identifier {
                return Err(crate::parser::owl_functional::error::grammar_error(format!(
                    "Expected language tag after '@', found: {}",
                    lang.lexeme
                )));
            }
            self.advance();
            return Ok(crate::core::entities::Literal::lang_tagged(
                lexical, lang.lexeme,
            ));
        }

        Ok(crate::core::entities::Literal::simple(lexical))
    }

    /// Parse an object property expression: a property reference or
    /// `ObjectInverseOf(pe)`.
    fn parse_object_property_expression(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::property_expressions::ObjectPropertyExpression>
    {
        use crate::logic::axioms::property_expressions::ObjectPropertyExpression;

        let token = self.peek().clone();
        if token.token_type == TokenType::ObjectInverseOf {
            self.advance();
            self.consume(
                TokenType::LeftParen,
                "Expected '(' after ObjectInverseOf",
            )?;
            let inner = self.parse_object_property_expression()?;
            self.consume(
                TokenType::RightParen,
                "Expected ')' after ObjectInverseOf",
            )?;
            return Ok(ObjectPropertyExpression::ObjectInverseOf(Box::new(inner)));
        }

        let iri = self.parse_iri_reference("object property expression")?;
        Ok(ObjectPropertyExpression::ObjectProperty(Box::new(
            ObjectProperty::from_shared_iri(iri),
        )))
    }

    /// Parse a data property expression (named data properties only).
    fn parse_data_property_expression(
        &mut self,
    ) -> FunctionalSyntaxResult<crate::logic::axioms::property_expressions::DataPropertyExpression>
    {
        let iri = self.parse_iri_reference("data property expression")?;
        Ok(
            crate::logic::axioms::property_expressions::DataPropertyExpression::DataProperty(
                DataProperty::from_shared_iri(iri),
            ),
        )
    }

    /// Parse an IRI reference in any of its three surface forms:
    /// `<full IRI>`, `:name` (default prefix), `prefix:name`.
    fn parse_iri_reference(&mut self, what: &str) -> FunctionalSyntaxResult<Arc<IRI>> {
        let token = self.peek().clone();
        match token.token_type {
            TokenType::IRI => {
                self.advance();
                self.resolve_iri(&token.lexeme)
            }
            TokenType::Colon => {
                // Default-prefixed name like :A
                self.advance();
                let name_token = self.peek().clone();
                if name_token.token_type == TokenType::Identifier {
                    self.advance();
                    self.resolve_iri(&format!(":{}", name_token.lexeme))
                } else {
                    Err(crate::parser::owl_functional::error::grammar_error(format!(
                        "Expected identifier after ':', found: {}",
                        name_token.lexeme
                    )))
                }
            }
            TokenType::Identifier => {
                // Prefixed name like owl:Thing: Identifier ':' Identifier
                let next = self.tokens.get(self.current + 1).cloned();
                let after = self.tokens.get(self.current + 2).cloned();
                match (next, after) {
                    (
                        Some(next_token),
                        Some(after_token),
                    ) if next_token.token_type == TokenType::Colon
                        && after_token.token_type == TokenType::Identifier =>
                    {
                        self.advance(); // identifier
                        self.advance(); // colon
                        self.advance(); // local name
                        self.resolve_iri(&format!("{}:{}", token.lexeme, after_token.lexeme))
                    }
                    _ => Err(crate::parser::owl_functional::error::grammar_error(format!(
                        "Expected {}, found: {}",
                        what, token.lexeme
                    ))),
                }
            }
            _ => Err(crate::parser::owl_functional::error::grammar_error(format!(
                "Expected {}, found: {}",
                what, token.lexeme
            ))),
        }
    }

    /// Parse an individual: a named reference (full IRI, `:name`,
    /// `prefix:name`) or an anonymous individual `_:nodeid`.
    fn parse_individual(&mut self) -> FunctionalSyntaxResult<crate::core::entities::Individual> {
        let token = self.peek().clone();

        // Anonymous individual _:nodeid — tokenized as Identifier("_"),
        // Colon, Identifier(nodeid).
        if token.token_type == TokenType::Identifier && token.lexeme == "_" {
            self.advance();
            self.consume(
                TokenType::Colon,
                "Expected ':' after '_' in anonymous individual",
            )?;
            let name_token = self.peek().clone();
            if name_token.token_type != TokenType::Identifier {
                return Err(crate::parser::owl_functional::error::grammar_error(format!(
                    "Expected node ID after '_:', found: {}",
                    name_token.lexeme
                )));
            }
            self.advance();
            return Ok(crate::core::entities::Individual::Anonymous(
                AnonymousIndividual::new(name_token.lexeme),
            ));
        }

        let iri = self.parse_iri_reference("individual")?;
        Ok(crate::core::entities::Individual::Named(
            NamedIndividual::from_shared_iri(iri),
        ))
    }

    /// Convert an individual to an IRI, minting the `_:nodeid` IRI for
    /// anonymous individuals the same way the RDF/XML parser's
    /// blank-node individuals are represented.
    fn individual_iri_or_mint(
        &mut self,
        individual: crate::core::entities::Individual,
        context: &str,
    ) -> FunctionalSyntaxResult<Arc<IRI>> {
        match individual {
            crate::core::entities::Individual::Named(named) => Ok(named.iri().clone()),
            crate::core::entities::Individual::Anonymous(anon) => {
                let label = format!("_:{}", anon.node_id());
                IRI::new_optimized(&label).map_err(|e| {
                    crate::parser::owl_functional::error::invalid_iri_error(format!(
                        "Invalid anonymous individual in {}: {} ({})",
                        context, label, e
                    ))
                })
            }
        }
    }

    /// Parse a class
    fn parse_class(&mut self) -> FunctionalSyntaxResult<Class> {
        let iri = self.parse_iri_reference("class IRI")?;
        Ok(Class::from_shared_iri(iri))
    }

    /// Parse an object property
    fn parse_object_property(&mut self) -> FunctionalSyntaxResult<ObjectProperty> {
        let iri = self.parse_iri_reference("object property IRI")?;
        Ok(ObjectProperty::from_shared_iri(iri))
    }

    /// Parse a data property
    fn parse_data_property(&mut self) -> FunctionalSyntaxResult<DataProperty> {
        let iri = self.parse_iri_reference("data property IRI")?;
        Ok(DataProperty::from_shared_iri(iri))
    }

    /// Parse a named individual
    fn parse_named_individual(&mut self) -> FunctionalSyntaxResult<NamedIndividual> {
        let iri = self.parse_iri_reference("named individual IRI")?;
        Ok(NamedIndividual::from_shared_iri(iri))
    }

    /// Parse an anonymous individual `_:nodeid`.
    fn parse_anonymous_individual(&mut self) -> FunctionalSyntaxResult<AnonymousIndividual> {
        let individual = self.parse_individual()?;
        match individual {
            crate::core::entities::Individual::Anonymous(anon) => Ok(anon),
            crate::core::entities::Individual::Named(_) => {
                Err(crate::parser::owl_functional::error::grammar_error(
                    "Expected anonymous individual _:nodeid".to_string(),
                ))
            }
        }
    }

    /// Parse an annotation property
    fn parse_annotation_property(&mut self) -> FunctionalSyntaxResult<AnnotationProperty> {
        let token = self.peek().clone();
        if token.token_type == TokenType::IRI {
            self.advance();
            let iri = self.resolve_iri(&token.lexeme)?;
            Ok(AnnotationProperty::new((*iri).clone()))
        } else {
            Err(crate::parser::owl_functional::error::grammar_error(format!(
                "Expected annotation property IRI, found: {}",
                token.lexeme
            )))
        }
    }

    /// Resolve an IRI from token lexeme
    fn resolve_iri(&mut self, lexeme: &str) -> FunctionalSyntaxResult<Arc<IRI>> {
        if lexeme.starts_with('<') && lexeme.ends_with('>') {
            // Full IRI
            let iri_content = &lexeme[1..lexeme.len() - 1];
            self.intern_iri(iri_content)
        } else if lexeme.contains(':') {
            // Prefixed name
            let mut parts = lexeme.splitn(2, ':');
            let prefix = parts.next().unwrap_or("");
            let local_name = parts.next().unwrap_or("");

            // Empty prefix means default prefix, stored as "" in the map
            if let Some(namespace) = self.prefixes.get(prefix) {
                let full_iri = format!("{}{}", namespace, local_name);
                self.intern_iri(&full_iri).map_err(|e| {
                    crate::parser::owl_functional::error::invalid_iri_error(format!(
                        "Invalid prefixed IRI '{}': {}",
                        lexeme, e
                    ))
                })
            } else {
                Err(crate::parser::owl_functional::error::invalid_iri_error(format!(
                    "Unknown prefix: '{}'",
                    prefix
                )))
            }
        } else {
            Err(crate::parser::owl_functional::error::invalid_iri_error(format!(
                "Invalid IRI format: {}",
                lexeme
            )))
        }
    }

    // Token manipulation methods
    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.current).unwrap_or(&self.eof_token)
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn match_token(&mut self, token_type: TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, token_type: TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            self.peek().token_type == token_type
        }
    }

    fn consume(&mut self, token_type: TokenType, message: &str) -> FunctionalSyntaxResult<()> {
        if self.check(token_type) {
            self.advance();
            Ok(())
        } else {
            Err(crate::parser::owl_functional::error::grammar_error(
                message.to_string(),
            ))
        }
    }
}

/// Build the end-of-stream sentinel token for `peek`.
fn eof_sentinel() -> Token {
    Token::new(TokenType::EOF, String::new(), 0, 0, 0)
}

/// Which object cardinality restriction is being parsed.
enum ObjectCardinalityKind {
    Min,
    Max,
    Exact,
}

impl ObjectCardinalityKind {
    fn name(&self) -> &'static str {
        match self {
            ObjectCardinalityKind::Min => "ObjectMinCardinality",
            ObjectCardinalityKind::Max => "ObjectMaxCardinality",
            ObjectCardinalityKind::Exact => "ObjectExactCardinality",
        }
    }
}

/// Which data cardinality restriction is being parsed.
enum DataCardinalityKind {
    Min,
    Max,
    Exact,
}

impl DataCardinalityKind {
    fn name(&self) -> &'static str {
        match self {
            DataCardinalityKind::Min => "DataMinCardinality",
            DataCardinalityKind::Max => "DataMaxCardinality",
            DataCardinalityKind::Exact => "DataExactCardinality",
        }
    }
}

/// owl:Thing as a class expression.
fn owl_thing_expression(
) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
    let iri = IRI::new_optimized(&format!("{}Thing", NS_OWL)).map_err(|e| {
        crate::parser::owl_functional::error::invalid_iri_error(format!(
            "Invalid owl:Thing IRI: {}",
            e
        ))
    })?;
    Ok(crate::logic::axioms::class_expressions::ClassExpression::Class(
        Class::from_shared_iri(iri),
    ))
}

/// owl:Nothing as a class expression.
fn owl_nothing_expression(
) -> FunctionalSyntaxResult<crate::logic::axioms::class_expressions::ClassExpression> {
    let iri = IRI::new_optimized(&format!("{}Nothing", NS_OWL)).map_err(|e| {
        crate::parser::owl_functional::error::invalid_iri_error(format!(
            "Invalid owl:Nothing IRI: {}",
            e
        ))
    })?;
    Ok(crate::logic::axioms::class_expressions::ClassExpression::Class(
        Class::from_shared_iri(iri),
    ))
}

/// The IRI of a class expression when it is a plain named class.
fn named_iri_of(
    expression: &crate::logic::axioms::class_expressions::ClassExpression,
) -> Option<Arc<IRI>> {
    match expression {
        crate::logic::axioms::class_expressions::ClassExpression::Class(class) => {
            Some(class.iri().clone())
        }
        _ => None,
    }
}

/// The IRI of a named data property expression.
fn data_property_iri(
    expression: &crate::logic::axioms::property_expressions::DataPropertyExpression,
) -> Arc<IRI> {
    match expression {
        crate::logic::axioms::property_expressions::DataPropertyExpression::DataProperty(prop) => {
            prop.iri().clone()
        }
    }
}

/// Mint the auxiliary role IRI for an inverse property expression,
/// identical to the RDF/XML streaming parser's scheme
/// (`urn:tableauxx:inverse-role:<fnv1a64>`), so both parsers derive
/// the same role for the same wrapped property.
fn inverse_property_expression_role(wrapped: &IRI) -> FunctionalSyntaxResult<Arc<IRI>> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in wrapped.as_str().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    IRI::new_optimized(&format!("urn:tableauxx:inverse-role:{:016x}", hash)).map_err(|e| {
        crate::parser::owl_functional::error::invalid_iri_error(format!(
            "Invalid auxiliary inverse-role IRI: {}",
            e
        ))
    })
}

// Helper trait extension for ObjectPropertyExpression
trait ObjectPropertyExpressionExt {
    fn to_iri(&self) -> FunctionalSyntaxResult<Arc<IRI>>;
}

impl ObjectPropertyExpressionExt
    for crate::logic::axioms::property_expressions::ObjectPropertyExpression
{
    fn to_iri(&self) -> FunctionalSyntaxResult<Arc<IRI>> {
        match self {
            crate::logic::axioms::property_expressions::ObjectPropertyExpression::ObjectProperty(prop) => {
                Ok((*prop).iri().clone())
            }
            _ => Err(crate::parser::owl_functional::error::grammar_error(
                "Complex property expressions not supported in this context".to_string(),
            )),
        }
    }
}
