//! OWL/XML format parser for OWL2 ontologies
//!
//! Implements parsing of the OWL/XML serialization format using simple XML parsing.

use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::SubClassOfAxiom;
use crate::core::entities::*;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml::RdfXmlParser;
use crate::parser::{OntologyParser, ParserConfig};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;

/// OWL/XML format parser
pub struct OwlXmlParser {
    config: ParserConfig,
    namespaces: HashMap<String, String>,
}

impl OwlXmlParser {
    fn env_truthy(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn might_be_rdf_xml(path: &Path) -> bool {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut reader = BufReader::new(file);
        let mut buf = vec![0u8; 16 * 1024];
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if n == 0 {
            return false;
        }
        let sample = String::from_utf8_lossy(&buf[..n]);
        let sample = sample.trim_start();
        sample.contains("<rdf:RDF") || sample.contains("<rdf:Description")
    }

    /// Create a new OWL/XML parser with default configuration
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new OWL/XML parser with custom configuration
    pub fn with_config(config: ParserConfig) -> Self {
        let mut namespaces = HashMap::new();
        for (prefix, namespace) in &config.prefixes {
            namespaces.insert(prefix.clone(), namespace.clone());
        }
        OwlXmlParser { config, namespaces }
    }

    /// Parse OWL/XML content and build an ontology
    fn parse_content(&mut self, content: &str) -> OwlResult<Ontology> {
        let mut ontology = Ontology::new();

        // Simple XML parsing for OWL/XML constructs
        if let Ok(document) = self.parse_xml_document(content) {
            // Debug: print what we found
            // println!("Root element: {:?}", document.root.as_ref().map(|r| &r.name));
            // println!("Standalone elements: {}", document.elements.len());
            // if let Some(root) = &document.root {
            //     println!("Root children: {}", root.children.len());
            // }

            self.process_owl_xml_document(&mut ontology, &document)?;

            if self.config.strict_validation {
                self.validate_ontology(&ontology)?;
            }
        }

        // Resolve imports if configured to do so
        if self.config.resolve_imports {
            if let Err(e) = ontology.resolve_imports() {
                if self.config.ignore_import_errors {
                    log::warn!("Import resolution failed: {}", e);
                } else {
                    return Err(e);
                }
            }
        }

        Ok(ontology)
    }

    /// Parse XML document into a simple structure
    fn parse_xml_document(&mut self, content: &str) -> OwlResult<XmlDocument> {
        let mut document = XmlDocument {
            root: None,
            elements: Vec::new(),
        };

        // More sophisticated XML parsing for OWL/XML structure
        let mut element_stack: Vec<(XmlElement, usize)> = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse tags in the line
            let mut pos = 0;
            while pos < line.len() {
                if let Some(tag_start) = line[pos..].find('<') {
                    let tag_start_pos = pos + tag_start;
                    if let Some(tag_end) = line[tag_start_pos..].find('>') {
                        let tag_end_pos = tag_start_pos + tag_end;
                        let mut tag_content = &line[tag_start_pos + 1..tag_end_pos];

                        // Handle self-closing tags by removing trailing slash
                        let is_self_closing = tag_content.ends_with('/');
                        if is_self_closing {
                            tag_content = tag_content[..tag_content.len() - 1].trim_end();
                        }

                        if !tag_content.starts_with("!--") && !tag_content.starts_with("?") {
                            if tag_content.starts_with("/") {
                                // Closing tag
                                if let Some((opening_element, _)) = element_stack.pop() {
                                    let element_name = opening_element.name.clone();
                                    // Push the completed element to its parent or document
                                    if let Some((parent_element, _)) = element_stack.last_mut() {
                                        parent_element.children.push(opening_element);
                                    } else if element_name == "Ontology" {
                                        document.root = Some(Box::new(opening_element));
                                    } else {
                                        // Only add as standalone if it's not already processed as a child
                                        // This prevents duplicate processing
                                        document.elements.push(opening_element);
                                    }
                                }
                            } else {
                                // Opening tag
                                let tag_name =
                                    tag_content.split_whitespace().next().unwrap_or(tag_content);

                                // Extract attributes
                                let mut element = XmlElement {
                                    name: tag_name.to_string(),
                                    attributes: HashMap::new(),
                                    content: String::new(),
                                    children: Vec::new(),
                                };

                                // Parse attributes
                                let attr_content = &tag_content[tag_name.len()..];
                                self.parse_attributes(attr_content, &mut element);

                                // For self-closing tags, add to parent immediately
                                if is_self_closing {
                                    if let Some((parent_element, _)) = element_stack.last_mut() {
                                        parent_element.children.push(element);
                                    } else if element.name == "Ontology" {
                                        document.root = Some(Box::new(element));
                                    } else {
                                        document.elements.push(element);
                                    }
                                } else {
                                    // Push to stack for later closing
                                    element_stack.push((element, line_num));
                                }
                            }
                        }

                        pos = tag_end_pos + 1;
                    } else {
                        pos += tag_start + 1;
                    }
                } else {
                    pos = line.len();
                }
            }
        }

        Ok(document)
    }

    /// Parse XML attributes
    fn parse_attributes(&mut self, attr_content: &str, element: &mut XmlElement) {
        let attr_parts: Vec<&str> = attr_content.split_whitespace().collect();
        for part in attr_parts {
            if let Some(eq_pos) = part.find('=') {
                let key = &part[..eq_pos];
                let value = &part[eq_pos + 1..];
                if value.len() >= 2 && (value.starts_with('"') || value.starts_with('\'')) {
                    let clean_value = &value[1..value.len() - 1];
                    element
                        .attributes
                        .insert(key.to_string(), clean_value.to_string());

                    // Track namespace declarations
                    if let Some(prefix) = key.strip_prefix("xmlns:") {
                        self.namespaces
                            .insert(prefix.to_string(), clean_value.to_string());
                    } else if key == "xmlns" {
                        self.namespaces
                            .insert("".to_string(), clean_value.to_string());
                    }
                } else {
                    // Handle unquoted values
                    element
                        .attributes
                        .insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    /// Process OWL/XML document and populate ontology
    fn process_owl_xml_document(
        &self,
        ontology: &mut Ontology,
        document: &XmlDocument,
    ) -> OwlResult<()> {
        let mut processed_ids = std::collections::HashSet::new();

        // Process root element first
        if let Some(root) = &document.root {
            self.process_owl_xml_element_with_tracking(ontology, root, &mut processed_ids)?;
            // Process all children of root
            for child in &root.children {
                self.process_owl_xml_element_with_tracking(ontology, child, &mut processed_ids)?;
                // Process grandchildren recursively
                self.process_element_recursive_with_tracking(ontology, child, &mut processed_ids)?;
            }
        }

        // Process standalone elements (only if not already processed)
        for element in &document.elements {
            self.process_owl_xml_element_with_tracking(ontology, element, &mut processed_ids)?;
            self.process_element_recursive_with_tracking(ontology, element, &mut processed_ids)?;
        }
        Ok(())
    }

    /// Process element with duplicate tracking
    fn process_owl_xml_element_with_tracking(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
        processed_ids: &mut std::collections::HashSet<String>,
    ) -> OwlResult<()> {
        // Create a unique ID for this element based on its name and key attributes
        let element_id = if let Some(iri) = element.attributes.get("IRI") {
            format!("{}:{}", element.name, iri)
        } else {
            element.name.clone()
        };

        // Only process if we haven't seen this element before
        if processed_ids.insert(element_id.clone()) {
            self.process_owl_xml_element_internal(ontology, element)?;
        }
        Ok(())
    }

    /// Process element and all its children recursively with tracking
    fn process_element_recursive_with_tracking(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
        processed_ids: &mut std::collections::HashSet<String>,
    ) -> OwlResult<()> {
        for child in &element.children {
            self.process_owl_xml_element_with_tracking(ontology, child, processed_ids)?;
            self.process_element_recursive_with_tracking(ontology, child, processed_ids)?;
        }
        Ok(())
    }

    /// Process element and all its children recursively (legacy method - now uses tracking)
    #[allow(dead_code)]
    fn process_element_recursive(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        let mut processed_ids = std::collections::HashSet::new();
        for child in &element.children {
            self.process_owl_xml_element_with_tracking(ontology, child, &mut processed_ids)?;
            self.process_element_recursive_with_tracking(ontology, child, &mut processed_ids)?;
        }
        Ok(())
    }

    /// Process individual OWL/XML elements (internal implementation)
    fn process_owl_xml_element_internal(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        match element.name.as_str() {
            "Ontology" => {
                if let Some(iri) = element.attributes.get("IRI") {
                    ontology.set_iri(IRI::new(iri)?);
                }
            }
            "Declaration" => {
                self.process_declaration(ontology, element)?;
            }
            "SubClassOf" => {
                self.process_subclass_of(ontology, element)?;
            }
            "EquivalentClasses" => {
                self.process_equivalent_classes(ontology, element)?;
            }
            "DisjointClasses" => {
                self.process_disjoint_classes(ontology, element)?;
            }
            "ObjectProperty" => {
                self.process_object_property(ontology, element)?;
            }
            "DataProperty" => {
                self.process_data_property(ontology, element)?;
            }
            "NamedIndividual" => {
                self.process_named_individual(ontology, element)?;
            }
            // Skip standalone Class elements in OWL/XML - they should be in Declarations
            "Class" => {
                // Standalone Class elements are skipped in OWL/XML to avoid duplicates
                // They should be processed within Declaration elements
            }
            _ => {
                // Unknown element types are silently ignored
            }
        }
        Ok(())
    }

    /// Process class declaration
    fn process_declaration(&self, ontology: &mut Ontology, element: &XmlElement) -> OwlResult<()> {
        for child in &element.children {
            if let Some(iri) = child.attributes.get("IRI") {
                // Resolve IRI against base URI if needed
                let resolved_iri = if iri.starts_with("http") {
                    iri.clone()
                } else {
                    // This is a relative IRI, resolve against base
                    if let Some(base_iri) = ontology.iri() {
                        // Ensure base IRI ends with / for proper resolution
                        let base_str = base_iri.to_string();
                        let base = if base_str.ends_with('/') {
                            base_str
                        } else {
                            format!("{}/", base_str)
                        };
                        format!("{base}{iri}")
                    } else {
                        // Try to extract xml:base from the root element
                        // For now, use example.org as fallback
                        format!("http://example.org/{iri}")
                    }
                };

                match child.name.as_str() {
                    "Class" => {
                        let class = Class::new(IRI::new(&resolved_iri)?);
                        ontology.add_class(class)?;
                    }
                    "ObjectProperty" => {
                        let prop = ObjectProperty::new(IRI::new(&resolved_iri)?);
                        ontology.add_object_property(prop)?;
                    }
                    "DataProperty" => {
                        let prop = DataProperty::new(IRI::new(&resolved_iri)?);
                        ontology.add_data_property(prop)?;
                    }
                    "NamedIndividual" => {
                        let individual = NamedIndividual::new(IRI::new(&resolved_iri)?);
                        ontology.add_named_individual(individual)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Process subclass relationship
    fn process_subclass_of(&self, ontology: &mut Ontology, element: &XmlElement) -> OwlResult<()> {
        let mut sub_class = None;
        let mut super_class = None;

        for child in &element.children {
            if child.name == "Class" {
                if let Some(iri) = child.attributes.get("IRI") {
                    if sub_class.is_none() {
                        sub_class = Some(Class::new(IRI::new(iri)?));
                    } else {
                        super_class = Some(Class::new(IRI::new(iri)?));
                    }
                }
            }
        }

        if let (Some(sub), Some(sup)) = (sub_class, super_class) {
            let subclass_axiom = SubClassOfAxiom::new(
                crate::logic::axioms::class_expressions::ClassExpression::Class(sub),
                crate::logic::axioms::class_expressions::ClassExpression::Class(sup),
            );
            ontology.add_subclass_axiom(subclass_axiom)?;
        }

        Ok(())
    }

    /// Process object property
    fn process_object_property(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(iri) = element.attributes.get("IRI") {
            let prop = ObjectProperty::new(IRI::new(iri)?);
            ontology.add_object_property(prop)?;
        }
        Ok(())
    }

    /// Process data property
    fn process_data_property(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(iri) = element.attributes.get("IRI") {
            let prop = DataProperty::new(IRI::new(iri)?);
            ontology.add_data_property(prop)?;
        }
        Ok(())
    }

    /// Process named individual
    fn process_named_individual(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        if let Some(iri) = element.attributes.get("IRI") {
            let individual = NamedIndividual::new(IRI::new(iri)?);
            ontology.add_named_individual(individual)?;
        }
        Ok(())
    }

    /// Process equivalent classes
    fn process_equivalent_classes(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        use crate::logic::axioms::class_expressions::ClassExpression;
        use crate::logic::axioms::EquivalentClassesAxiom;

        let mut class_descriptions = Vec::new();

        // Process child elements to find class descriptions
        for child in &element.children {
            match child.name.as_str() {
                "Class" | "owl:Class" => {
                    if let Some(iri) = child.attributes.get("IRI") {
                        let iri_obj = IRI::new(iri)?;
                        let class = Class::new(iri_obj);
                        class_descriptions.push(ClassExpression::Class(class));
                    } else if let Some(abbreviated_iri) = child.attributes.get("abbreviatedIRI") {
                        // Handle abbreviated IRI like "owl:Thing"
                        if let Some(colon_pos) = abbreviated_iri.find(':') {
                            let prefix = &abbreviated_iri[..colon_pos];
                            let local = &abbreviated_iri[colon_pos + 1..];

                            if let Some(namespace) = self.namespaces.get(prefix) {
                                let full_iri = format!("{}{}", namespace, local);
                                let iri_obj = IRI::new(&full_iri)?;
                                let class = Class::new(iri_obj);
                                class_descriptions.push(ClassExpression::Class(class));
                            }
                        }
                    }
                }
                "ObjectIntersectionOf" | "owl:ObjectIntersectionOf" => {
                    // Handle intersection of classes
                    if let Some(class_expr) = self.parse_object_intersection_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                "ObjectUnionOf" | "owl:ObjectUnionOf" => {
                    // Handle union of classes
                    if let Some(class_expr) = self.parse_object_union_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                "ObjectComplementOf" | "owl:ObjectComplementOf" => {
                    // Handle complement of classes
                    if let Some(class_expr) = self.parse_object_complement_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                _ => {
                    // Skip unknown elements
                    if self.config.strict_validation {
                        return Err(crate::core::error::OwlError::ParseError(format!(
                            "Unknown class description: {}",
                            child.name
                        )));
                    }
                }
            }
        }

        if class_descriptions.len() >= 2 {
            // Extract IRIs from ClassExpressions
            let class_iris: Vec<IRI> = class_descriptions
                .into_iter()
                .filter_map(|ce| match ce {
                    ClassExpression::Class(cls) => Some((**cls.iri()).clone()),
                    _ => None,
                })
                .collect();

            if class_iris.len() >= 2 {
                let class_arc_iris: Vec<Arc<IRI>> = class_iris.into_iter().map(Arc::new).collect();
                let axiom = EquivalentClassesAxiom::new(class_arc_iris);
                ontology.add_equivalent_classes_axiom(axiom)?;
            } else if self.config.strict_validation {
                return Err(crate::core::error::OwlError::ParseError(
                    "EquivalentClasses requires at least 2 named classes".to_string(),
                ));
            }
        } else if self.config.strict_validation {
            return Err(crate::core::error::OwlError::ParseError(
                "EquivalentClasses requires at least 2 class descriptions".to_string(),
            ));
        }

        Ok(())
    }

    /// Process disjoint classes
    fn process_disjoint_classes(
        &self,
        ontology: &mut Ontology,
        element: &XmlElement,
    ) -> OwlResult<()> {
        use crate::logic::axioms::class_expressions::ClassExpression;
        use crate::logic::axioms::DisjointClassesAxiom;

        let mut class_descriptions = Vec::new();

        // Process child elements to find class descriptions
        for child in &element.children {
            match child.name.as_str() {
                "Class" | "owl:Class" => {
                    if let Some(iri) = child.attributes.get("IRI") {
                        let iri_obj = IRI::new(iri)?;
                        let class = Class::new(iri_obj);
                        class_descriptions.push(ClassExpression::Class(class));
                    } else if let Some(abbreviated_iri) = child.attributes.get("abbreviatedIRI") {
                        // Handle abbreviated IRI like "owl:Thing"
                        if let Some(colon_pos) = abbreviated_iri.find(':') {
                            let prefix = &abbreviated_iri[..colon_pos];
                            let local = &abbreviated_iri[colon_pos + 1..];

                            if let Some(namespace) = self.namespaces.get(prefix) {
                                let full_iri = format!("{}{}", namespace, local);
                                let iri_obj = IRI::new(&full_iri)?;
                                let class = Class::new(iri_obj);
                                class_descriptions.push(ClassExpression::Class(class));
                            }
                        }
                    }
                }
                "ObjectIntersectionOf" | "owl:ObjectIntersectionOf" => {
                    // Handle intersection of classes
                    if let Some(class_expr) = self.parse_object_intersection_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                "ObjectUnionOf" | "owl:ObjectUnionOf" => {
                    // Handle union of classes
                    if let Some(class_expr) = self.parse_object_union_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                "ObjectComplementOf" | "owl:ObjectComplementOf" => {
                    // Handle complement of classes
                    if let Some(class_expr) = self.parse_object_complement_of(child)? {
                        class_descriptions.push(class_expr);
                    }
                }
                _ => {
                    // Skip unknown elements
                    if self.config.strict_validation {
                        return Err(crate::core::error::OwlError::ParseError(format!(
                            "Unknown class description: {}",
                            child.name
                        )));
                    }
                }
            }
        }

        if class_descriptions.len() >= 2 {
            // Extract IRIs from ClassExpressions
            let class_iris: Vec<IRI> = class_descriptions
                .into_iter()
                .filter_map(|ce| match ce {
                    ClassExpression::Class(cls) => Some((**cls.iri()).clone()),
                    _ => None,
                })
                .collect();

            if class_iris.len() >= 2 {
                let class_arc_iris: Vec<Arc<IRI>> = class_iris.into_iter().map(Arc::new).collect();
                let axiom = DisjointClassesAxiom::new(class_arc_iris);
                ontology.add_disjoint_classes_axiom(axiom)?;
            } else if self.config.strict_validation {
                return Err(crate::core::error::OwlError::ParseError(
                    "DisjointClasses requires at least 2 named classes".to_string(),
                ));
            }
        } else if self.config.strict_validation {
            return Err(crate::core::error::OwlError::ParseError(
                "DisjointClasses requires at least 2 class descriptions".to_string(),
            ));
        }

        Ok(())
    }

    /// Parse ObjectIntersectionOf class expression
    fn parse_object_intersection_of(
        &self,
        element: &XmlElement,
    ) -> OwlResult<Option<ClassExpression>> {
        use crate::logic::axioms::class_expressions::ClassExpression;
        use smallvec::SmallVec;

        let mut operands = Vec::new();

        for child in &element.children {
            if let Some(class_expr) = self.parse_class_expression(child)? {
                operands.push(Box::new(class_expr));
            }
        }

        if operands.len() >= 2 {
            let smallvec: SmallVec<[Box<ClassExpression>; 4]> = operands.into();
            Ok(Some(ClassExpression::ObjectIntersectionOf(smallvec)))
        } else {
            Ok(None)
        }
    }

    /// Parse ObjectUnionOf class expression
    fn parse_object_union_of(&self, element: &XmlElement) -> OwlResult<Option<ClassExpression>> {
        use crate::logic::axioms::class_expressions::ClassExpression;
        use smallvec::SmallVec;

        let mut operands = Vec::new();

        for child in &element.children {
            if let Some(class_expr) = self.parse_class_expression(child)? {
                operands.push(Box::new(class_expr));
            }
        }

        if operands.len() >= 2 {
            let smallvec: SmallVec<[Box<ClassExpression>; 4]> = operands.into();
            Ok(Some(ClassExpression::ObjectUnionOf(smallvec)))
        } else {
            Ok(None)
        }
    }

    /// Parse ObjectComplementOf class expression
    fn parse_object_complement_of(
        &self,
        element: &XmlElement,
    ) -> OwlResult<Option<ClassExpression>> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        if let Some(child) = element.children.first() {
            if let Some(class_expr) = self.parse_class_expression(child)? {
                return Ok(Some(ClassExpression::ObjectComplementOf(Box::new(
                    class_expr,
                ))));
            }
        }

        Ok(None)
    }

    /// Parse class expression (helper for complex class descriptions)
    fn parse_class_expression(&self, element: &XmlElement) -> OwlResult<Option<ClassExpression>> {
        use crate::logic::axioms::class_expressions::ClassExpression;

        match element.name.as_str() {
            "Class" | "owl:Class" => {
                if let Some(iri) = element.attributes.get("IRI") {
                    let iri_obj = IRI::new(iri)?;
                    let class = Class::new(iri_obj);
                    Ok(Some(ClassExpression::Class(class)))
                } else if let Some(abbreviated_iri) = element.attributes.get("abbreviatedIRI") {
                    if let Some(colon_pos) = abbreviated_iri.find(':') {
                        let prefix = &abbreviated_iri[..colon_pos];
                        let local = &abbreviated_iri[colon_pos + 1..];

                        if let Some(namespace) = self.namespaces.get(prefix) {
                            let full_iri = format!("{}{}", namespace, local);
                            let iri_obj = IRI::new(&full_iri)?;
                            let class = Class::new(iri_obj);
                            Ok(Some(ClassExpression::Class(class)))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Validate the parsed ontology
    fn validate_ontology(&self, ontology: &Ontology) -> OwlResult<()> {
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

impl OntologyParser for OwlXmlParser {
    fn parse_str(&self, content: &str) -> OwlResult<Ontology> {
        // Create a mutable copy for parsing
        let mut parser_copy = OwlXmlParser::with_config(self.config.clone());
        parser_copy.parse_content(content)
    }

    fn parse_file(&self, path: &Path) -> OwlResult<Ontology> {
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

        // Many ".owl"/".xml" files are RDF/XML, so delegate to the RDF/XML parser to use streaming.
        if Self::env_truthy("OWL2_REASONER_LARGE_PARSE") && Self::might_be_rdf_xml(path) {
            let rdf_parser = RdfXmlParser::with_config(self.config.clone());
            return rdf_parser.parse_file(path);
        }

        let mut file = fs::File::open(path)?;
        let capacity = fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let mut content = String::with_capacity(capacity.saturating_add(1));
        file.read_to_string(&mut content)?;
        let mut parser_copy = OwlXmlParser::with_config(self.config.clone());
        parser_copy.namespaces = self.namespaces.clone();
        parser_copy.parse_content(&content)
    }

    fn format_name(&self) -> &'static str {
        "OWL/XML"
    }
}

impl Default for OwlXmlParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple XML document structure for OWL/XML parsing
#[derive(Debug, Clone)]
struct XmlDocument {
    root: Option<Box<XmlElement>>,
    elements: Vec<XmlElement>,
}

/// Simple XML element structure
#[derive(Debug, Clone)]
struct XmlElement {
    name: String,
    attributes: HashMap<String, String>,
    #[allow(dead_code)]
    content: String,
    children: Vec<XmlElement>,
}
