//! JSON-LD Container Processing
//!
//! Implements comprehensive container processing for JSON-LD 1.1 including:
//! - Language containers (@container: @language)
//! - Index containers (@container: @index)
//! - Set containers (@container: @set)
//! - List containers (@container: @list)
//! - Graph containers (@container: @graph)
//! - Type and ID containers

use crate::core::error::OwlResult;
use crate::parser::json_ld::context::{Container, Context, TermDefinition};
use crate::parser::json_ld::value::{ProcessedValue, ValueProcessor};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Represents a processed container with its values
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedContainer {
    /// Container type
    pub container_type: Container,
    /// Container values
    pub values: Vec<ProcessedValue>,
    /// Container index or language key (if applicable)
    pub key: Option<String>,
    /// Whether the container preserves order (list vs set)
    pub ordered: bool,
}

/// JSON-LD Container Processor
#[derive(Debug, Clone)]
pub struct ContainerProcessor {
    value_processor: ValueProcessor,
}

impl ContainerProcessor {
    /// Create a new container processor
    pub fn new() -> Self {
        Self {
            value_processor: ValueProcessor::new(),
        }
    }

    /// Process a value according to its container specification
    pub fn process_container_value(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        let container_type = self.get_container_type(term_def, value);

        match container_type {
            Container::Language => self.process_language_container(value, term_def, context),
            Container::Index(index_key) => {
                self.process_index_container(value, &index_key, term_def, context)
            }
            Container::Set => self.process_set_container(value, term_def, context),
            Container::List => self.process_list_container(value, term_def, context),
            Container::Graph => self.process_graph_container(value, term_def, context),
            Container::Type => self.process_type_container(value, term_def, context),
            Container::Id => self.process_id_container(value, term_def, context),
        }
    }

    /// Determine the container type for a value
    fn get_container_type(&self, term_def: &TermDefinition, value: &Value) -> Container {
        // Check term definition container
        if let Some(ref container) = term_def.container {
            return container.clone();
        }

        // Check value for implicit containers
        match value {
            Value::Object(obj) => {
                if obj.contains_key("@list") {
                    Container::List
                } else if obj.contains_key("@set") {
                    Container::Set
                } else if obj.contains_key("@graph") {
                    Container::Graph
                } else if self.is_language_map(obj) {
                    Container::Language
                } else if self.is_index_map(obj) {
                    Container::Index(String::new()) // Empty key, will be filled later
                } else {
                    // No container
                    Container::Set // Default to set for arrays
                }
            }
            Value::Array(_) => Container::Set, // Arrays default to sets
            _ => Container::Set,               // Single values default to set
        }
    }

    /// Process language container (@container: @language)
    fn process_language_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        match value {
            Value::Object(obj) => {
                let mut containers = Vec::new();

                for (lang, val) in obj {
                    // Override language in term definition
                    let mut lang_term_def = term_def.clone();
                    lang_term_def.language = Some(lang.clone());

                    let processed_values =
                        self.value_processor
                            .process_value(val, &lang_term_def, context)?;

                    if !processed_values.is_empty() {
                        containers.push(ProcessedContainer {
                            container_type: Container::Language,
                            values: processed_values,
                            key: Some(lang.clone()),
                            ordered: false, // Language maps are unordered
                        });
                    }
                }

                Ok(containers)
            }
            Value::Array(_arr) => {
                // Array values in language container
                let processed_values = self
                    .value_processor
                    .process_value(value, term_def, context)?;
                if !processed_values.is_empty() {
                    Ok(vec![ProcessedContainer {
                        container_type: Container::Language,
                        values: processed_values,
                        key: context.language.clone(),
                        ordered: false,
                    }])
                } else {
                    Ok(vec![])
                }
            }
            _ => {
                // Single value
                let processed_values = self
                    .value_processor
                    .process_value(value, term_def, context)?;
                if !processed_values.is_empty() {
                    Ok(vec![ProcessedContainer {
                        container_type: Container::Language,
                        values: processed_values,
                        key: context.language.clone(),
                        ordered: false,
                    }])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    /// Process index container (@container: @index)
    fn process_index_container(
        &self,
        value: &Value,
        index_key: &str,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        match value {
            Value::Object(obj) => {
                let mut containers = Vec::new();

                for (index, val) in obj {
                    let processed_values =
                        self.value_processor.process_value(val, term_def, context)?;

                    if !processed_values.is_empty() {
                        containers.push(ProcessedContainer {
                            container_type: Container::Index(index.to_string()),
                            values: processed_values,
                            key: Some(index.clone()),
                            ordered: false, // Index maps are unordered
                        });
                    }
                }

                Ok(containers)
            }
            _ => {
                // Single value with index
                let processed_values = self
                    .value_processor
                    .process_value(value, term_def, context)?;
                if !processed_values.is_empty() {
                    Ok(vec![ProcessedContainer {
                        container_type: Container::Index(index_key.to_string()),
                        values: processed_values,
                        key: Some(index_key.to_string()),
                        ordered: false,
                    }])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    /// Process set container (@container: @set)
    fn process_set_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        let processed_values = self
            .value_processor
            .process_value(value, term_def, context)?;

        if !processed_values.is_empty() {
            Ok(vec![ProcessedContainer {
                container_type: Container::Set,
                values: processed_values,
                key: None,
                ordered: false, // Sets are unordered
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Process list container (@container: @list)
    fn process_list_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        let processed_values = self
            .value_processor
            .process_value(value, term_def, context)?;

        if !processed_values.is_empty() {
            Ok(vec![ProcessedContainer {
                container_type: Container::List,
                values: processed_values,
                key: None,
                ordered: true, // Lists preserve order
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Process graph container (@container: @graph)
    fn process_graph_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        let processed_values = self
            .value_processor
            .process_value(value, term_def, context)?;

        if !processed_values.is_empty() {
            Ok(vec![ProcessedContainer {
                container_type: Container::Graph,
                values: processed_values,
                key: None,
                ordered: false, // Graphs are unordered
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Process type container (@container: @type)
    fn process_type_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        match value {
            Value::Object(obj) => {
                let mut containers = Vec::new();

                for (type_, val) in obj {
                    // Override type in term definition
                    let mut type_term_def = term_def.clone();
                    type_term_def.type_ = Some(type_.clone());

                    let processed_values =
                        self.value_processor
                            .process_value(val, &type_term_def, context)?;

                    if !processed_values.is_empty() {
                        containers.push(ProcessedContainer {
                            container_type: Container::Type,
                            values: processed_values,
                            key: Some(type_.clone()),
                            ordered: false,
                        });
                    }
                }

                Ok(containers)
            }
            _ => {
                // Single value
                let processed_values = self
                    .value_processor
                    .process_value(value, term_def, context)?;
                if !processed_values.is_empty() {
                    Ok(vec![ProcessedContainer {
                        container_type: Container::Type,
                        values: processed_values,
                        key: term_def.type_.clone(),
                        ordered: false,
                    }])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    /// Process ID container (@container: @id)
    fn process_id_container(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedContainer>> {
        let processed_values = self
            .value_processor
            .process_value(value, term_def, context)?;

        if !processed_values.is_empty() {
            Ok(vec![ProcessedContainer {
                container_type: Container::Id,
                values: processed_values,
                key: None,
                ordered: false,
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Check if an object is a language map
    fn is_language_map(&self, obj: &Map<String, Value>) -> bool {
        // Language maps have language codes as keys
        obj.keys().all(|k| self.is_language_code(k))
    }

    /// Check if an object is an index map
    fn is_index_map(&self, obj: &Map<String, Value>) -> bool {
        // Index maps have string keys and objects as values
        obj.iter().all(|(_, v)| matches!(v, Value::Object(_)))
    }

    /// Check if a string is a language code
    fn is_language_code(&self, s: &str) -> bool {
        s.len() >= 2 && s.len() <= 8 && s.chars().all(|c| c.is_alphabetic() || c == '-')
    }

    /// Flatten containers into a single list of values
    pub fn flatten_containers(&self, containers: Vec<ProcessedContainer>) -> Vec<ProcessedValue> {
        let mut values = Vec::new();

        for container in containers {
            values.extend(container.values);
        }

        values
    }

    /// Group values by container type
    pub fn group_by_container_type(
        &self,
        containers: Vec<ProcessedContainer>,
    ) -> HashMap<Container, Vec<ProcessedContainer>> {
        let mut groups = HashMap::new();

        for container in containers {
            groups
                .entry(container.container_type.clone())
                .or_insert_with(Vec::new)
                .push(container);
        }

        groups
    }

    /// Extract unique values from containers (for set semantics)
    pub fn unique_values(&self, values: Vec<ProcessedValue>) -> Vec<ProcessedValue> {
        let mut unique = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for value in values {
            if seen.insert(value.clone()) {
                unique.push(value);
            }
        }

        unique
    }

    /// Convert containers to RDF triples (simplified for OWL2 use case)
    pub fn containers_to_rdf_triples(
        &self,
        subject: &str,
        predicate: &str,
        containers: Vec<ProcessedContainer>,
    ) -> Vec<RdfTriple> {
        let mut triples = Vec::new();

        for container in containers {
            for value in &container.values {
                let object = self.processed_value_to_rdf_object(value);
                let triple = RdfTriple {
                    subject: subject.to_string(),
                    predicate: predicate.to_string(),
                    object,
                };
                triples.push(triple);
            }
        }

        triples
    }

    /// Convert a processed value to an RDF object representation
    fn processed_value_to_rdf_object(&self, value: &ProcessedValue) -> RdfObject {
        match value {
            ProcessedValue::Iri(iri) => RdfObject::Iri(iri.as_str().to_string()),
            ProcessedValue::TypedLiteral { value, datatype } => RdfObject::Literal {
                value: value.clone(),
                datatype: datatype.as_str().to_string(),
                language: None,
            },
            ProcessedValue::LanguageLiteral { value, language } => RdfObject::Literal {
                value: value.clone(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                language: Some(language.clone()),
            },
            ProcessedValue::DirectionalLiteral {
                value,
                language,
                direction,
            } => RdfObject::Literal {
                value: format!("{} ({})", value, direction),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                language: language.clone(),
            },
            ProcessedValue::BlankNode(id) => RdfObject::BlankNode(id.clone()),
            ProcessedValue::Collection(_) => {
                // Collections are complex - for now, treat as blank node
                RdfObject::BlankNode("_:collection".to_string())
            }
            ProcessedValue::Multiple(_) => {
                // Multiple values - for now, treat as blank node
                RdfObject::BlankNode("_:multiple".to_string())
            }
        }
    }
}

/// Simple RDF triple representation
#[derive(Debug, Clone, PartialEq)]
pub struct RdfTriple {
    pub subject: String,
    pub predicate: String,
    pub object: RdfObject,
}

/// RDF object representation
#[derive(Debug, Clone, PartialEq)]
pub enum RdfObject {
    Iri(String),
    Literal {
        value: String,
        datatype: String,
        language: Option<String>,
    },
    BlankNode(String),
}

impl Default for ContainerProcessor {
    fn default() -> Self {
        Self::new()
    }
}
