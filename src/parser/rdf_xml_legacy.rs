//! Legacy RDF/XML parser for strict validation mode

use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml_common::{
    initialize_namespaces, ResourceInfo, XmlDocument, XmlElement, ERR_EMPTY_ONTOLOGY, RDF_ABOUT,
    RDF_RESOURCE,
};
use crate::parser::{ParserArenaBuilder, ParserArenaTrait, ParserConfig};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Legacy RDF/XML parser for strict validation and compatibility
pub struct RdfXmlLegacyParser {
    pub config: ParserConfig,
    pub namespaces: HashMap<String, String>,
    pub base_iri: Option<IRI>,
    pub blank_node_counter: u32,
    pub resource_map: HashMap<String, ResourceInfo>,
    pub arena: Option<Box<dyn ParserArenaTrait>>,
}

impl RdfXmlLegacyParser {
    /// Create a new legacy parser
    pub fn new(config: ParserConfig) -> Self {
        let namespaces = initialize_namespaces(&config.prefixes);

        let arena = if config.use_arena_allocation {
            Some(
                ParserArenaBuilder::new()
                    .with_capacity(config.arena_capacity)
                    .build(),
            )
        } else {
            None
        };

        Self {
            config,
            namespaces,
            base_iri: None,
            blank_node_counter: 0,
            resource_map: HashMap::new(),
            arena,
        }
    }

    /// Parse RDF/XML content using legacy approach
    pub fn parse_content(&mut self, content: &str) -> OwlResult<Ontology> {
        if self.config.strict_validation && content.trim().is_empty() {
            return Err(crate::core::error::OwlError::ValidationError(
                ERR_EMPTY_ONTOLOGY.to_string(),
            ));
        }

        let mut ontology = Ontology::new();
        let document = self.parse_xml_document(content)?;
        self.process_rdf_document(&mut ontology, &document)?;
        self.process_resource_map(&mut ontology)?;

        if self.config.strict_validation {
            self.validate_ontology(&ontology)?;
        }

        Ok(ontology)
    }

    /// Parse RDF/XML file using legacy approach
    pub fn parse_file(&mut self, path: &Path) -> OwlResult<Ontology> {
        use std::fs;

        let content = fs::read_to_string(path).map_err(crate::core::error::OwlError::IoError)?;

        // Check file size
        if content.len() > self.config.max_file_size {
            return Err(crate::core::error::OwlError::ValidationError(
                "File size exceeds maximum allowed size".to_string(),
            ));
        }

        self.parse_content(&content)
    }

    /// Parse XML document structure
    pub fn parse_xml_document(&mut self, content: &str) -> OwlResult<XmlDocument> {
        let mut chars = content.char_indices();
        let mut document = XmlDocument::default();

        // Parse XML declaration if present
        if content.starts_with("<?xml") {
            self.parse_xml_declaration(&mut chars, &mut document)?;
        }

        // Parse doctype if present
        if let Some(_pos) = content.find("<!DOCTYPE") {
            // This is simplified - in practice, you'd parse the doctype properly
        }

        // Parse root element
        if let Some(element) = self.parse_element(&mut chars)? {
            document.root = Some(element);
        }

        Ok(document)
    }

    /// Parse XML declaration
    fn parse_xml_declaration(
        &self,
        _chars: &mut std::str::CharIndices<'_>,
        document: &mut XmlDocument,
    ) -> OwlResult<()> {
        // This is a simplified implementation
        // In practice, you'd parse version, encoding, and standalone attributes
        document.xml_version = Some("1.0".to_string());
        document.encoding = Some("UTF-8".to_string());
        Ok(())
    }

    /// Parse XML element
    fn parse_element(
        &self,
        _chars: &mut std::str::CharIndices<'_>,
    ) -> OwlResult<Option<XmlElement>> {
        // Simplified element parsing
        // In practice, this would be much more comprehensive
        Ok(None)
    }

    /// Process RDF document and build ontology
    fn process_rdf_document(
        &mut self,
        ontology: &mut Ontology,
        document: &XmlDocument,
    ) -> OwlResult<()> {
        if let Some(root) = &document.root {
            if root.name == "RDF" || root.name.ends_with(":RDF") {
                for child in &root.children {
                    self.process_rdf_element(ontology, child)?;
                }
            } else {
                // Handle documents without explicit RDF root
                self.process_rdf_element(ontology, root)?;
            }
        }
        Ok(())
    }

    /// Process individual RDF element
    fn process_rdf_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        match element.name.as_str() {
            "Ontology" | "owl:Ontology" => {
                self.process_ontology_element(ontology, element)?;
            }
            "Class" | "owl:Class" => {
                self.process_class_element(ontology, element)?;
            }
            "ObjectProperty" | "owl:ObjectProperty" => {
                self.process_object_property_element(ontology, element)?;
            }
            "DatatypeProperty" | "owl:DatatypeProperty" => {
                self.process_datatype_property_element(ontology, element)?;
            }
            "NamedIndividual" | "owl:NamedIndividual" => {
                self.process_named_individual_element(ontology, element)?;
            }
            "Description" | "rdf:Description" => {
                self.process_description_element(ontology, element)?;
            }
            _ => {
                // Handle unknown elements
                if self.config.strict_validation {
                    return Err(crate::core::error::OwlError::ParseError(format!(
                        "Unknown element: {}",
                        element.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Process ontology element
    fn process_ontology_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        // Extract ontology IRI
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;
            ontology.set_iri(iri);
        }

        // Process imports
        for child in &element.children {
            if child.name == "imports" || child.name == "owl:imports" {
                if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                    let import_iri = IRI::new(resource)?;
                    ontology.add_import(import_iri);
                }
            }
        }

        Ok(())
    }

    /// Process class element
    fn process_class_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;
            let class = Class::new(iri.clone());
            ontology.add_class(class.clone())?;

            // Process subclass relationships
            for child in &element.children {
                if child.name == "subClassOf" || child.name == "rdfs:subClassOf" {
                    if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                        let superclass_iri = IRI::new(resource)?;
                        let superclass = Class::new(superclass_iri);
                        let axiom = SubClassOfAxiom::new(
                            ClassExpression::Class(class.clone()),
                            ClassExpression::Class(superclass),
                        );
                        ontology.add_subclass_axiom(axiom)?;
                    }
                }

                // Process equivalent classes
                if child.name == "equivalentClass" || child.name == "owl:equivalentClass" {
                    if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                        let equivalent_class_iri = IRI::new(resource)?;
                        let _equivalent_class = Class::new(equivalent_class_iri.clone());
                        let axiom = EquivalentClassesAxiom::new(vec![
                            Arc::new(iri.clone()),
                            Arc::new(equivalent_class_iri.clone()),
                        ]);
                        ontology.add_equivalent_classes_axiom(axiom)?;
                    }
                }

                // Process disjoint classes
                if child.name == "disjointWith" || child.name == "owl:disjointWith" {
                    if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                        let disjoint_class_iri = IRI::new(resource)?;
                        let _disjoint_class = Class::new(disjoint_class_iri.clone());
                        let axiom = DisjointClassesAxiom::new(vec![
                            Arc::new(iri.clone()),
                            Arc::new(disjoint_class_iri.clone()),
                        ]);
                        ontology.add_disjoint_classes_axiom(axiom)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Process object property element
    fn process_object_property_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;
            let property = ObjectProperty::new(iri);
            ontology.add_object_property(property)?;

            // Process property characteristics
            for child in &element.children {
                self.process_property_characteristic(ontology, child)?;
            }
        }

        Ok(())
    }

    /// Process datatype property element
    fn process_datatype_property_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;
            let property = DataProperty::new(iri);
            ontology.add_data_property(property)?;

            // Process property characteristics
            for child in &element.children {
                self.process_property_characteristic(ontology, child)?;
            }
        }

        Ok(())
    }

    /// Process named individual element
    fn process_named_individual_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;
            let individual = NamedIndividual::new(iri.clone());
            ontology.add_named_individual(individual.clone())?;

            // Process types and property assertions
            for child in &element.children {
                if child.name == "type" || child.name == "rdf:type" {
                    if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                        let class_iri = IRI::new(resource)?;
                        let class = Class::new(class_iri);
                        let assertion = ClassAssertionAxiom::new(
                            Arc::new(iri.clone()),
                            ClassExpression::Class(class),
                        );
                        ontology.add_class_assertion(assertion)?;
                    }
                }

                // Process property assertions
                self.process_property_assertion(ontology, &individual, child)?;
            }
        }

        Ok(())
    }

    /// Process description element
    fn process_description_element(
        &mut self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        // Handle generic RDF descriptions
        if let Some(about) = element.attributes.get(RDF_ABOUT) {
            let iri = IRI::new(about)?;

            // Check if this is a class axiom by examining child elements
            let has_class_axioms = element.children.iter().any(|child| {
                matches!(
                    child.name.as_str(),
                    "disjointWith"
                        | "owl:disjointWith"
                        | "equivalentClass"
                        | "owl:equivalentClass"
                        | "subClassOf"
                        | "rdfs:subClassOf"
                )
            });

            if has_class_axioms {
                // This is a class description, not an individual
                // Process class axioms
                for child in &element.children {
                    // Process disjoint classes
                    if child.name == "disjointWith" || child.name == "owl:disjointWith" {
                        if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                            let disjoint_class_iri = IRI::new(resource)?;
                            let axiom = DisjointClassesAxiom::new(vec![
                                Arc::new(iri.clone()),
                                Arc::new(disjoint_class_iri.clone()),
                            ]);
                            ontology.add_disjoint_classes_axiom(axiom)?;
                        }
                    }

                    // Process equivalent classes
                    if child.name == "equivalentClass" || child.name == "owl:equivalentClass" {
                        if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                            let equivalent_class_iri = IRI::new(resource)?;
                            let axiom = EquivalentClassesAxiom::new(vec![
                                Arc::new(iri.clone()),
                                Arc::new(equivalent_class_iri.clone()),
                            ]);
                            ontology.add_equivalent_classes_axiom(axiom)?;
                        }
                    }

                    // Process subclass relationships
                    if child.name == "subClassOf" || child.name == "rdfs:subClassOf" {
                        if let Some(resource) = child.attributes.get(RDF_RESOURCE) {
                            let superclass_iri = IRI::new(resource)?;
                            let subclass = Class::new(iri.clone());
                            let superclass = Class::new(superclass_iri);
                            let axiom = SubClassOfAxiom::new(
                                ClassExpression::Class(subclass),
                                ClassExpression::Class(superclass),
                            );
                            ontology.add_axiom(Axiom::SubClassOf(Box::new(axiom)))?;
                        }
                    }
                }
            } else {
                // This is an individual description
                let individual = NamedIndividual::new(iri);
                ontology.add_named_individual(individual.clone())?;

                // Process property assertions
                for child in &element.children {
                    self.process_property_assertion(ontology, &individual, child)?;
                }
            }
        } else if let Some(node_id) = element.attributes.get("rdf:nodeID") {
            // Anonymous individual (blank node)
            let anon_individual = AnonymousIndividual::new(format!("_:{}", node_id));
            ontology.add_anonymous_individual(anon_individual.clone())?;

            // Process property assertions
            for child in &element.children {
                self.process_property_assertion_anon(ontology, &anon_individual, child)?;
            }
        }

        Ok(())
    }

    /// Process property characteristic (functional, symmetric, etc.)
    fn process_property_characteristic(
        &mut self,
        _ontology: &mut Ontology,
        _element: &XmlElement,
    ) -> OwlResult<()> {
        // This is a simplified implementation
        // In practice, you'd handle all OWL property characteristics
        Ok(())
    }

    /// Process property assertion
    fn process_property_assertion(
        &mut self,
        ontology: &mut Ontology,
        individual: &NamedIndividual,
        element: &XmlElement,
    ) -> OwlResult<()> {
        // Handle nested anonymous individuals (like <ex:knows><rdf:Description rdf:nodeID="blank1">...</rdf:Description></ex:knows>)
        if !element.children.is_empty() {
            for child in &element.children {
                if child.name == "rdf:Description" {
                    if let Some(node_id) = child.attributes.get("rdf:nodeID") {
                        // Create anonymous individual from nested description
                        let anon_individual = AnonymousIndividual::new(format!("_:{}", node_id));
                        ontology.add_anonymous_individual(anon_individual.clone())?;

                        // Create property assertion with anonymous individual
                        if let Some(property_name) = element
                            .name
                            .split_once(':')
                            .map(|(_, name)| name)
                            .or(Some(&element.name))
                        {
                            let property_iri = if let Some(expanded) =
                                crate::parser::rdf_xml_common::expand_qname(
                                    &element.name,
                                    &self.namespaces,
                                ) {
                                IRI::new(&expanded)?
                            } else {
                                IRI::new(property_name)?
                            };

                            let assertion = PropertyAssertionAxiom::new_with_anonymous(
                                individual.iri().clone(),
                                Arc::new(property_iri),
                                anon_individual,
                            );
                            ontology.add_property_assertion(assertion)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Process property assertion for anonymous individuals
    fn process_property_assertion_anon(
        &mut self,
        _ontology: &mut Ontology,
        _individual: &AnonymousIndividual,
        _element: &XmlElement,
    ) -> OwlResult<()> {
        // This is a simplified implementation
        // In practice, you'd handle object and data property assertions for anonymous individuals
        Ok(())
    }

    /// Process resource map and resolve references
    fn process_resource_map(&mut self, _ontology: &mut Ontology) -> OwlResult<()> {
        // Process any pending resource mappings
        // This is where you'd resolve forward references
        Ok(())
    }

    /// Validate ontology consistency
    fn validate_ontology(&self, ontology: &Ontology) -> OwlResult<()> {
        // Perform consistency checks
        // This is a simplified implementation
        if ontology.classes().is_empty() && ontology.imports().is_empty() {
            return Err(crate::core::error::OwlError::ValidationError(
                "Ontology must contain at least one declaration or import".to_string(),
            ));
        }

        Ok(())
    }

    /// Get a reference to the arena allocator
    pub fn arena(&self) -> Option<&dyn ParserArenaTrait> {
        self.arena.as_deref()
    }

    /// Allocate a string in the arena if available
    pub fn alloc_string<'a>(&'a self, s: &'a str) -> &'a str {
        if let Some(arena) = self.arena() {
            arena.arena().alloc_str(s)
        } else {
            s
        }
    }
}
