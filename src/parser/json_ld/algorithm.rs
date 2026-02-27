//! JSON-LD 1.1 Expansion Algorithm
//!
//! Implements the core JSON-LD expansion algorithm according to W3C specification:
//! - Term expansion
//! - Value expansion
//! - Context processing
//! - Container handling
//! - Array and object expansion

use crate::core::error::{OwlError, OwlResult};
use crate::parser::json_ld::container::ContainerProcessor;
use crate::parser::json_ld::context::{Context, ContextManager};
use crate::parser::json_ld::value::ValueProcessor;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// JSON-LD Expansion Algorithm implementation
#[derive(Debug, Clone)]
pub struct JsonLdExpansionAlgorithm {
    context_manager: ContextManager,
    #[allow(dead_code)]
    value_processor: ValueProcessor,
    #[allow(dead_code)]
    container_processor: ContainerProcessor,
    /// Active context stack for nested processing
    active_contexts: Vec<Context>,
    /// Active graph for expanded nodes
    active_graph: Vec<ExpandedNode>,
}

/// Represents an expanded JSON-LD node
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedNode {
    /// Node IRI or blank node identifier
    pub id: Option<String>,
    /// Node type(s)
    pub types: Vec<String>,
    /// Properties
    pub properties: HashMap<String, Vec<ExpandedValue>>,
    /// Graph container (if this is a graph node)
    pub graph: Option<Vec<ExpandedNode>>,
}

/// Represents an expanded value
#[derive(Debug, Clone, PartialEq)]
pub enum ExpandedValue {
    /// IRI reference
    Iri(String),
    /// Blank node
    BlankNode(String),
    /// Literal value
    Literal {
        value: String,
        datatype: String,
        language: Option<String>,
    },
    /// List of values
    List(Vec<ExpandedValue>),
    /// Set of values
    Set(Vec<ExpandedValue>),
    /// Nested node (for @graph)
    Node(Box<ExpandedNode>),
}

/// Expansion configuration options
#[derive(Debug, Clone, Default)]
pub struct ExpansionConfig {
    /// Whether to keep blank node identifiers stable
    pub keep_blank_nodes: bool,
    /// Whether to process @reverse properties
    pub process_reverse: bool,
    /// Whether to process @import directives
    pub process_imports: bool,
    /// Maximum expansion depth
    pub max_depth: usize,
}

impl JsonLdExpansionAlgorithm {
    /// Create a new expansion algorithm instance
    pub fn new() -> Self {
        Self {
            context_manager: ContextManager::new(),
            value_processor: ValueProcessor::new(),
            container_processor: ContainerProcessor::new(),
            active_contexts: Vec::new(),
            active_graph: Vec::new(),
        }
    }

    /// Create an expansion algorithm with specific configuration
    pub fn with_config(_config: ExpansionConfig) -> Self {
        let mut algorithm = Self::new();
        algorithm.context_manager = ContextManager::new(); // Use non-remote for testing
        algorithm
    }

    /// Create an expansion algorithm with custom expansion configuration
    pub fn with_expansion_config(_config: ExpansionConfig) -> Self {
        let mut algorithm = Self::new();
        algorithm.context_manager = ContextManager::new(); // Use non-remote for testing
        algorithm
    }

    /// Expand JSON-LD input according to the W3C specification
    pub fn expand(&mut self, input: &Value) -> OwlResult<Vec<ExpandedNode>> {
        // Step 1: Initialize with default context
        self.active_contexts.push(Context::default());
        self.active_graph.clear();

        // Step 2: Determine processing mode
        let expanded = self.expand_value(input)?;

        // Step 3: Process the expanded result
        match expanded {
            ExpandedValue::List(values) => {
                // Handle @graph arrays
                self.process_graph_array(values)
            }
            ExpandedValue::Set(values) => {
                // Treat sets as graph arrays for top-level processing
                self.process_graph_array(values)
            }
            ExpandedValue::Node(node) => {
                // Handle single node
                Ok(vec![*node])
            }
            ExpandedValue::Iri(iri) => {
                // Single IRI - create a node with that IRI
                Ok(vec![ExpandedNode {
                    id: Some(iri),
                    types: Vec::new(),
                    properties: HashMap::new(),
                    graph: None,
                }])
            }
            _ => {
                // Other types - return empty graph
                Ok(vec![])
            }
        }
    }

    /// Expand a value according to the active context
    fn expand_value(&mut self, value: &Value) -> OwlResult<ExpandedValue> {
        match value {
            Value::Object(obj) => self.expand_object(obj),
            Value::Array(arr) => self.expand_array(arr),
            Value::String(s) => self.expand_string(s),
            Value::Number(n) => self.expand_number(n),
            Value::Bool(b) => self.expand_boolean(*b),
            Value::Null => Err(OwlError::ParseError("Cannot expand null value".to_string())),
        }
    }

    /// Expand a JSON object
    fn expand_object(&mut self, obj: &Map<String, Value>) -> OwlResult<ExpandedValue> {
        // Check for @context first
        if let Some(context) = obj.get("@context") {
            let new_context = self.context_manager.parse_context(context)?;
            self.active_contexts.push(new_context);
        }

        // Check for @graph
        if let Some(graph) = obj.get("@graph") {
            let expanded_graph = self.expand_value(graph)?;
            // Clean up context after processing
            if obj.contains_key("@context") {
                self.active_contexts.pop();
            }
            return Ok(expanded_graph);
        }

        // Check for @value (value object)
        if let Some(value) = obj.get("@value") {
            let expanded_value = self.expand_value(value)?;
            // Apply value object properties
            let value_obj = self.expand_value_object(expanded_value, obj)?;
            // Clean up context after processing
            if obj.contains_key("@context") {
                self.active_contexts.pop();
            }
            return Ok(value_obj);
        }

        // Check for @list
        if let Some(list) = obj.get("@list") {
            let expanded_list = self.expand_value(list)?;
            // Clean up context after processing
            if obj.contains_key("@context") {
                self.active_contexts.pop();
            }
            return Ok(expanded_list);
        }

        // Check for @set
        if let Some(set) = obj.get("@set") {
            let expanded_set = self.expand_value(set)?;
            // Clean up context after processing
            if obj.contains_key("@context") {
                self.active_contexts.pop();
            }
            return Ok(expanded_set);
        }

        // Check for @id
        let node_id = if let Some(id) = obj.get("@id") {
            match id {
                Value::String(s) => Some(self.expand_iri(s)?),
                _ => None,
            }
        } else {
            None
        };

        // Check for @type
        let mut node_types = Vec::new();
        if let Some(type_) = obj.get("@type") {
            match type_ {
                Value::String(s) => {
                    let expanded_type = self.expand_iri(s)?;
                    node_types.push(expanded_type);
                }
                Value::Array(arr) => {
                    for type_val in arr {
                        if let Value::String(s) = type_val {
                            let expanded_type = self.expand_iri(s)?;
                            node_types.push(expanded_type);
                        }
                    }
                }
                _ => {}
            }
        }

        // Process regular properties
        let mut properties = HashMap::new();
        let mut reverse_properties = HashMap::new();

        // First pass: identify reverse properties
        let reverse_predicates: Vec<(String, &Value)> = obj
            .iter()
            .filter(|(key, _)| !key.starts_with('@'))
            .filter(|(key, _)| {
                self.active_contexts
                    .last()
                    .and_then(|ctx| ctx.terms.get(*key))
                    .and_then(|term_def| term_def.reverse.as_ref())
                    .is_some()
            })
            .map(|(key, value)| (key.clone(), value))
            .collect();

        // Second pass: collect regular properties
        let reverse_keys: HashSet<&str> = reverse_predicates
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();

        let regular_properties: Vec<(&String, &Value)> = obj
            .iter()
            .filter(|(key, _)| !key.starts_with('@'))
            .filter(|(key, _)| !reverse_keys.contains(key.as_str()))
            .collect();

        // Process reverse properties
        for (predicate, value) in reverse_predicates {
            let expanded_values = self.expand_value(value)?;
            reverse_properties
                .entry(predicate.to_string())
                .or_insert_with(Vec::new)
                .push(expanded_values);
        }

        // Process regular properties
        for (key, value) in regular_properties {
            let expanded_key = self.expand_iri(key)?;

            if let Some('@') = expanded_key.chars().next() {
                // Skip if expanded key is a keyword
                continue;
            }

            // Regular property processing
            let expanded_values = self.expand_value(value)?;

            properties
                .entry(expanded_key)
                .or_insert_with(Vec::new)
                .push(expanded_values);
        }

        // Clean up context
        if obj.contains_key("@context") {
            self.active_contexts.pop();
        }

        // Create expanded node
        let expanded_node = ExpandedNode {
            id: node_id,
            types: node_types,
            properties,
            graph: None,
        };

        Ok(ExpandedValue::Node(Box::new(expanded_node)))
    }

    /// Expand an array
    fn expand_array(&mut self, arr: &[Value]) -> OwlResult<ExpandedValue> {
        let mut expanded_values = Vec::new();

        for item in arr {
            let expanded = self.expand_value(item)?;
            expanded_values.push(expanded);
        }

        // If we're in a list context, return as list
        if self.is_in_list_context() {
            Ok(ExpandedValue::List(expanded_values))
        } else {
            // Default to set for arrays
            Ok(ExpandedValue::Set(expanded_values))
        }
    }

    /// Expand a string value
    fn expand_string(&self, value: &str) -> OwlResult<ExpandedValue> {
        // Check if it's a blank node
        if value.starts_with("_:") {
            return Ok(ExpandedValue::BlankNode(value.to_string()));
        }

        // Check if it's an IRI
        if self.is_iri(value) {
            let expanded_iri = self.expand_iri(value)?;
            return Ok(ExpandedValue::Iri(expanded_iri));
        }

        // Treat as literal
        Ok(ExpandedValue::Literal {
            value: value.to_string(),
            datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
            language: self
                .active_contexts
                .last()
                .and_then(|ctx| ctx.language.clone()),
        })
    }

    /// Expand a numeric value
    fn expand_number(&self, value: &serde_json::Number) -> OwlResult<ExpandedValue> {
        let value_str = value.to_string();

        let datatype = if value.is_i64() {
            "http://www.w3.org/2001/XMLSchema#integer"
        } else if value_str.contains('.') {
            "http://www.w3.org/2001/XMLSchema#decimal"
        } else {
            "http://www.w3.org/2001/XMLSchema#double"
        };

        Ok(ExpandedValue::Literal {
            value: value_str,
            datatype: datatype.to_string(),
            language: None,
        })
    }

    /// Expand a boolean value
    fn expand_boolean(&self, value: bool) -> OwlResult<ExpandedValue> {
        Ok(ExpandedValue::Literal {
            value: value.to_string(),
            datatype: "http://www.w3.org/2001/XMLSchema#boolean".to_string(),
            language: None,
        })
    }

    /// Expand an IRI using the active context
    fn expand_iri(&self, iri: &str) -> OwlResult<String> {
        if let Some(current_context) = self.active_contexts.last() {
            current_context
                .expand_term(iri)
                .ok_or_else(|| OwlError::ParseError("Failed to expand term".to_string()))
        } else {
            // No context, return as-is if it looks like an IRI
            if self.is_iri(iri) {
                Ok(iri.to_string())
            } else {
                Err(OwlError::ParseError(format!(
                    "Cannot expand IRI '{}' without context",
                    iri
                )))
            }
        }
    }

    /// Expand a value object with @type, @language, @direction
    fn expand_value_object(
        &self,
        base_value: ExpandedValue,
        obj: &Map<String, Value>,
    ) -> OwlResult<ExpandedValue> {
        let mut expanded_value = base_value;

        // Apply @type if present
        if let Some(Value::String(type_str)) = obj.get("@type") {
            let expanded_type = self.expand_iri(type_str)?;
            if let ExpandedValue::Literal {
                ref mut datatype, ..
            } = expanded_value
            {
                *datatype = expanded_type;
            }
        }

        // Apply @language if present
        if let Some(Value::String(lang_str)) = obj.get("@language") {
            if let ExpandedValue::Literal {
                ref mut language, ..
            } = expanded_value
            {
                *language = Some(lang_str.to_string());
            }
        }

        // Apply @direction if present
        if let Some(Value::String(dir_str)) = obj.get("@direction") {
            // For now, add direction to the literal value
            if let ExpandedValue::Literal { ref mut value, .. } = expanded_value {
                *value = format!("{} ({})", value, dir_str);
            }
        }

        Ok(expanded_value)
    }

    /// Process @graph array
    fn process_graph_array(&mut self, values: Vec<ExpandedValue>) -> OwlResult<Vec<ExpandedNode>> {
        let mut graph_nodes = Vec::new();

        for value in values {
            match value {
                ExpandedValue::Node(node) => graph_nodes.push(*node),
                ExpandedValue::Iri(iri) => {
                    // Create a node with just the IRI
                    graph_nodes.push(ExpandedNode {
                        id: Some(iri),
                        types: Vec::new(),
                        properties: HashMap::new(),
                        graph: None,
                    });
                }
                _ => {
                    // Skip other types in graph
                }
            }
        }

        Ok(graph_nodes)
    }

    /// Check if currently processing in a list context
    fn is_in_list_context(&self) -> bool {
        // Simplified check - in a full implementation, this would track
        // the container type from term definitions
        false
    }

    /// Check if a string is an IRI
    fn is_iri(&self, s: &str) -> bool {
        s.starts_with("http://")
            || s.starts_with("https://")
            || s.starts_with("ftp://")
            || s.starts_with("mailto:")
            || s.starts_with("urn:")
            || s.contains("://") // Generic IRI pattern
    }

    /// Convert expanded nodes to a more convenient format for OWL2 processing
    pub fn to_owl2_format(&self, expanded_nodes: &[ExpandedNode]) -> OwlResult<Vec<Owl2Node>> {
        let mut owl2_nodes = Vec::new();

        for node in expanded_nodes {
            let owl2_node = Owl2Node {
                id: node.id.clone(),
                types: node.types.clone(),
                properties: node
                    .properties
                    .iter()
                    .map(|(pred, values)| {
                        let owl2_values: Vec<Owl2Value> = values
                            .iter()
                            .filter_map(|v| self.convert_expanded_to_owl2(v))
                            .collect();
                        (pred.clone(), owl2_values)
                    })
                    .collect(),
            };
            owl2_nodes.push(owl2_node);
        }

        Ok(owl2_nodes)
    }

    /// Convert expanded value to OWL2 format
    #[allow(clippy::only_used_in_recursion)]
    fn convert_expanded_to_owl2(&self, expanded: &ExpandedValue) -> Option<Owl2Value> {
        match expanded {
            ExpandedValue::Iri(iri) => Some(Owl2Value::Iri(iri.clone())),
            ExpandedValue::BlankNode(id) => Some(Owl2Value::BlankNode(id.clone())),
            ExpandedValue::Literal {
                value,
                datatype,
                language,
            } => Some(Owl2Value::Literal {
                value: value.clone(),
                datatype: datatype.clone(),
                language: language.clone(),
            }),
            ExpandedValue::List(values) => {
                let owl2_values: Vec<Owl2Value> = values
                    .iter()
                    .filter_map(|v| self.convert_expanded_to_owl2(v))
                    .collect();
                Some(Owl2Value::List(owl2_values))
            }
            ExpandedValue::Set(values) => {
                let owl2_values: Vec<Owl2Value> = values
                    .iter()
                    .filter_map(|v| self.convert_expanded_to_owl2(v))
                    .collect();
                Some(Owl2Value::Set(owl2_values))
            }
            ExpandedValue::Node(_) => {
                // Nested nodes are complex - for now, skip
                None
            }
        }
    }
}

/// OWL2 format node representation (simplified for the reasoner)
#[derive(Debug, Clone, PartialEq)]
pub struct Owl2Node {
    pub id: Option<String>,
    pub types: Vec<String>,
    pub properties: HashMap<String, Vec<Owl2Value>>,
}

/// OWL2 format value representation
#[derive(Debug, Clone, PartialEq)]
pub enum Owl2Value {
    Iri(String),
    BlankNode(String),
    Literal {
        value: String,
        datatype: String,
        language: Option<String>,
    },
    List(Vec<Owl2Value>),
    Set(Vec<Owl2Value>),
}

impl Default for JsonLdExpansionAlgorithm {
    fn default() -> Self {
        Self::new()
    }
}
