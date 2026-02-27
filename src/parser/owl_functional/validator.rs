//! Semantic validation for OWL Functional Syntax
//!
//! This module implements semantic validation for parsed OWL Functional Syntax
//! documents, ensuring that the parsed content conforms to OWL2 semantics.

use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::owl_functional::error::{validation_error, FunctionalSyntaxResult};
use crate::parser::owl_functional::syntax::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Validator for OWL Functional Syntax documents
pub struct FunctionalSyntaxValidator {
    /// Configuration for validation
    strict_mode: bool,
    /// Cache of known entities
    entities: HashMap<Arc<IRI>, EntityType>,
}

/// Types of entities that can be declared
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum EntityType {
    Class,
    ObjectProperty,
    DataProperty,
    NamedIndividual,
    AnnotationProperty,
}

impl FunctionalSyntaxValidator {
    /// Create a new validator with default settings
    pub fn new() -> Self {
        Self {
            strict_mode: true,
            entities: HashMap::new(),
        }
    }

    /// Create a new validator with specified strict mode
    pub fn with_strict_mode(strict_mode: bool) -> Self {
        Self {
            strict_mode,
            entities: HashMap::new(),
        }
    }

    /// Validate an entire document
    pub fn validate_document(
        &mut self,
        document: &FunctionalSyntaxAST,
    ) -> FunctionalSyntaxResult<()> {
        // First pass: collect all entity declarations
        self.collect_entities(document)?;

        // Second pass: validate all content
        self.validate_content(document)?;

        // Validate document structure
        self.validate_document_structure(document)?;

        Ok(())
    }

    /// Validate that an ontology is well-formed
    pub fn validate_ontology(&self, ontology: &Ontology) -> FunctionalSyntaxResult<()> {
        // Check that ontology has some content
        if ontology.classes().is_empty()
            && ontology.object_properties().is_empty()
            && ontology.data_properties().is_empty()
            && ontology.named_individuals().is_empty()
            && ontology.axioms().is_empty()
            && ontology.imports().is_empty()
            && self.strict_mode
        {
            return Err(validation_error(
                "Ontology contains no entities, axioms, or imports".to_string(),
            ));
        }

        // Validate entity declarations vs. usage
        self.validate_entity_usage(ontology)?;

        // Validate axiom consistency
        self.validate_axiom_consistency(ontology)?;

        Ok(())
    }

    /// Collect all entity declarations from the document
    fn collect_entities(&mut self, document: &FunctionalSyntaxAST) -> FunctionalSyntaxResult<()> {
        for content in document.content() {
            if let OntologyContent::Declaration(declaration) = content {
                match declaration {
                    EntityDeclaration::Class(class) => {
                        self.entities.insert(class.iri().clone(), EntityType::Class);
                    }
                    EntityDeclaration::ObjectProperty(prop) => {
                        self.entities
                            .insert(prop.iri().clone(), EntityType::ObjectProperty);
                    }
                    EntityDeclaration::DataProperty(prop) => {
                        self.entities
                            .insert(prop.iri().clone(), EntityType::DataProperty);
                    }
                    EntityDeclaration::NamedIndividual(individual) => {
                        self.entities
                            .insert(individual.iri().clone(), EntityType::NamedIndividual);
                    }
                    EntityDeclaration::AnnotationProperty(prop) => {
                        self.entities
                            .insert(prop.iri().clone(), EntityType::AnnotationProperty);
                    }
                    EntityDeclaration::AnonymousIndividual(_) => {
                        // Anonymous individuals don't need global registration
                    }
                }
            }
        }

        // Add built-in OWL vocabulary
        self.add_built_in_entities();

        Ok(())
    }

    /// Add built-in OWL entities to the entity cache
    fn add_built_in_entities(&mut self) {
        // OWL built-in classes
        let owl_classes = vec![
            "http://www.w3.org/2002/07/owl#Thing",
            "http://www.w3.org/2002/07/owl#Nothing",
        ];

        for class_iri in owl_classes {
            if let Ok(iri) = IRI::new(class_iri) {
                self.entities.insert(Arc::new(iri), EntityType::Class);
            }
        }

        // OWL built-in properties
        let owl_properties = vec![
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            "http://www.w3.org/2002/07/owl#equivalentClass",
            "http://www.w3.org/2002/07/owl#disjointWith",
            "http://www.w3.org/2002/07/owl#sameAs",
            "http://www.w3.org/2002/07/owl#differentFrom",
        ];

        for prop_iri in owl_properties {
            if let Ok(iri) = IRI::new(prop_iri) {
                self.entities
                    .insert(Arc::new(iri), EntityType::ObjectProperty);
            }
        }

        // RDF and RDFS built-ins
        let rdfs_classes = vec![
            "http://www.w3.org/2000/01/rdf-schema#Class",
            "http://www.w3.org/2000/01/rdf-schema#Resource",
        ];

        for class_iri in rdfs_classes {
            if let Ok(iri) = IRI::new(class_iri) {
                self.entities.insert(Arc::new(iri), EntityType::Class);
            }
        }
    }

    /// Validate all content in the document
    fn validate_content(&self, document: &FunctionalSyntaxAST) -> FunctionalSyntaxResult<()> {
        for content in document.content() {
            match content {
                OntologyContent::Declaration(_) => {
                    // Declarations are already handled in collect_entities
                }
                OntologyContent::Import(import) => {
                    self.validate_import(import)?;
                }
                OntologyContent::Axiom(axiom) => {
                    self.validate_axiom(axiom)?;
                }
            }
        }

        Ok(())
    }

    /// Validate an import declaration
    fn validate_import(&self, import: &ImportDeclaration) -> FunctionalSyntaxResult<()> {
        let iri_str = import.import_iri.as_str();

        // Check that the IRI is valid
        if iri_str.is_empty() {
            return Err(validation_error("Import IRI cannot be empty".to_string()));
        }

        // Check for common import patterns
        if iri_str.starts_with("http://") || iri_str.starts_with("https://") {
            // HTTP/HTTPS imports are valid
        } else if iri_str.starts_with("file://") {
            // File imports are valid
        } else if self.strict_mode {
            return Err(validation_error(format!(
                "Import IRI uses potentially unsupported scheme: {}",
                iri_str
            )));
        }

        Ok(())
    }

    /// Validate an axiom
    fn validate_axiom(&self, axiom: &Axiom) -> FunctionalSyntaxResult<()> {
        match axiom {
            Axiom::SubClassOf(subclass_axiom) => {
                self.validate_class_expression(subclass_axiom.sub_class())?;
                self.validate_class_expression(subclass_axiom.super_class())?;
            }
            Axiom::EquivalentClasses(equiv_axiom) => {
                for class_iri in equiv_axiom.classes() {
                    self.validate_entity_reference(class_iri, EntityType::Class)?;
                }
            }
            Axiom::DisjointClasses(disjoint_axiom) => {
                for class_iri in disjoint_axiom.classes() {
                    self.validate_entity_reference(class_iri, EntityType::Class)?;
                }
            }
            Axiom::SubObjectProperty(subprop_axiom) => {
                self.validate_entity_reference(
                    subprop_axiom.sub_property(),
                    EntityType::ObjectProperty,
                )?;
                self.validate_entity_reference(
                    subprop_axiom.super_property(),
                    EntityType::ObjectProperty,
                )?;
            }
            Axiom::ObjectPropertyDomain(domain_axiom) => {
                self.validate_entity_reference(domain_axiom.property(), EntityType::ObjectProperty)?;
                // Domain is an Arc<IRI>, not a ClassExpression - skip validation
            }
            Axiom::ObjectPropertyRange(range_axiom) => {
                self.validate_entity_reference(range_axiom.property(), EntityType::ObjectProperty)?;
                // Range is an Arc<IRI>, not a ClassExpression - skip validation
            }
            Axiom::ClassAssertion(assertion) => {
                self.validate_entity_reference(
                    assertion.individual(),
                    EntityType::NamedIndividual,
                )?;
                self.validate_class_expression(assertion.class_expr())?;
            }
            Axiom::PropertyAssertion(assertion) => {
                self.validate_entity_reference(assertion.subject(), EntityType::NamedIndividual)?;
                self.validate_entity_reference(assertion.property(), EntityType::ObjectProperty)?;
                if let Some(object_iri) = assertion.object_iri() {
                    self.validate_entity_reference(object_iri, EntityType::NamedIndividual)?;
                }
            }
            // Other axiom types can be added as needed
            _ => {
                // For now, accept other axiom types without detailed validation
            }
        }

        Ok(())
    }

    /// Validate a class expression
    fn validate_class_expression(
        &self,
        class_expr: &crate::logic::axioms::class_expressions::ClassExpression,
    ) -> FunctionalSyntaxResult<()> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        match class_expr {
            ClassExpression::Class(class) => {
                self.validate_entity_reference(class.iri(), EntityType::Class)?;
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                for operand in operands {
                    self.validate_class_expression(operand)?;
                }
            }
            ClassExpression::ObjectUnionOf(operands) => {
                for operand in operands {
                    self.validate_class_expression(operand)?;
                }
            }
            ClassExpression::ObjectComplementOf(operand) => {
                self.validate_class_expression(operand)?;
            }
            // Other class expression types can be added as needed
            _ => {
                // For now, accept other class expression types
            }
        }

        Ok(())
    }

    /// Validate that an entity reference is valid
    fn validate_entity_reference(
        &self,
        iri: &Arc<IRI>,
        expected_type: EntityType,
    ) -> FunctionalSyntaxResult<()> {
        if let Some(actual_type) = self.entities.get(iri) {
            if *actual_type != expected_type {
                return Err(validation_error(format!(
                    "Entity {} is declared as {:?} but used as {:?}",
                    iri, actual_type, expected_type
                )));
            }
        } else if self.strict_mode {
            return Err(validation_error(format!(
                "Entity {} of type {:?} is not declared",
                iri, expected_type
            )));
        }

        Ok(())
    }

    /// Validate document structure
    fn validate_document_structure(
        &self,
        document: &FunctionalSyntaxAST,
    ) -> FunctionalSyntaxResult<()> {
        // Check that if we have an ontology IRI, it's valid
        if let Some(ontology_iri) = match document {
            FunctionalSyntaxAST::OntologyDocument { ontology_iri, .. } => ontology_iri,
        } {
            if !ontology_iri.is_empty() {
                // Basic IRI validation
                if !ontology_iri.starts_with("http://")
                    && !ontology_iri.starts_with("https://")
                    && !ontology_iri.starts_with("urn:")
                    && self.strict_mode
                {
                    return Err(validation_error(format!(
                        "Ontology IRI uses potentially unsupported scheme: {}",
                        ontology_iri
                    )));
                }
            }
        }

        // Check for duplicate prefixes
        let mut seen_prefixes = std::collections::HashSet::new();
        for prefix_decl in document.prefixes() {
            if seen_prefixes.contains(&prefix_decl.prefix) {
                return Err(validation_error(format!(
                    "Duplicate prefix declaration: {}",
                    prefix_decl.prefix
                )));
            }
            seen_prefixes.insert(prefix_decl.prefix.clone());
        }

        Ok(())
    }

    /// Validate entity usage in an ontology
    fn validate_entity_usage(&self, ontology: &Ontology) -> FunctionalSyntaxResult<()> {
        // Validate that all used classes are declared
        for class in ontology.classes() {
            self.validate_entity_reference(class.iri(), EntityType::Class)?;
        }

        // Validate that all used object properties are declared
        for prop in ontology.object_properties() {
            self.validate_entity_reference(prop.iri(), EntityType::ObjectProperty)?;
        }

        // Validate that all used data properties are declared
        for prop in ontology.data_properties() {
            self.validate_entity_reference(prop.iri(), EntityType::DataProperty)?;
        }

        // Validate that all used named individuals are declared
        for individual in ontology.named_individuals() {
            self.validate_entity_reference(individual.iri(), EntityType::NamedIndividual)?;
        }

        Ok(())
    }

    /// Validate axiom consistency in an ontology
    fn validate_axiom_consistency(&self, ontology: &Ontology) -> FunctionalSyntaxResult<()> {
        // Check for obvious inconsistencies
        let mut class_pairs = std::collections::HashSet::new();
        let empty_iri =
            Arc::new(IRI::new("http://example.org/empty").expect("Valid example.org empty IRI"));

        for axiom in ontology.axioms() {
            match &**axiom {
                Axiom::SubClassOf(subclass_axiom) => {
                    // Check for circular subclass relationships
                    let sub_iri = subclass_axiom.sub_class().as_iri().unwrap_or(&empty_iri);
                    let super_iri = subclass_axiom.super_class().as_iri().unwrap_or(&empty_iri);

                    if sub_iri == super_iri {
                        return Err(validation_error(format!(
                            "Class {} cannot be a subclass of itself",
                            sub_iri
                        )));
                    }

                    // Record the relationship for circularity check
                    class_pairs.insert((sub_iri.clone(), super_iri.clone()));
                }
                Axiom::DisjointClasses(disjoint_axiom) => {
                    // Check that disjoint classes are not equivalent
                    for i in 0..disjoint_axiom.classes().len() {
                        for j in i + 1..disjoint_axiom.classes().len() {
                            let class1 = &disjoint_axiom.classes()[i];
                            let class2 = &disjoint_axiom.classes()[j];

                            // Check if they're declared equivalent
                            for axiom in ontology.axioms() {
                                if let Axiom::EquivalentClasses(equiv_axiom) = &**axiom {
                                    if equiv_axiom.classes().contains(class1)
                                        && equiv_axiom.classes().contains(class2)
                                    {
                                        return Err(validation_error(format!(
                                            "Classes {} and {} are both disjoint and equivalent",
                                            class1, class2
                                        )));
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Other axiom types can be validated as needed
                }
            }
        }

        // Check for circular subclass relationships (simple check)
        for (sub_iri, super_iri) in &class_pairs {
            if class_pairs.contains(&(super_iri.clone(), sub_iri.clone())) {
                return Err(validation_error(format!(
                    "Circular subclass relationship between {} and {}",
                    sub_iri, super_iri
                )));
            }
        }

        Ok(())
    }
}

impl Default for FunctionalSyntaxValidator {
    fn default() -> Self {
        Self::new()
    }
}

// Helper extension trait for ClassExpression
trait ClassExpressionExt {
    fn as_iri(&self) -> Option<&Arc<IRI>>;
}

impl ClassExpressionExt for crate::logic::axioms::class_expressions::ClassExpression {
    fn as_iri(&self) -> Option<&Arc<IRI>> {
        if let crate::logic::axioms::class_expressions::ClassExpression::Class(class) = self {
            Some(class.iri())
        } else {
            None
        }
    }
}
