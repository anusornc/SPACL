//! JSON-LD Value Processing
//!
//! Implements comprehensive value processing for JSON-LD 1.1 including:
//! - Typed literals with @type coercion
//! - Language-tagged strings with @language and @direction
//! - Compact IRI expansion
//! - Value object expansion

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::parser::json_ld::context::{Context, TermDefinition};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Represents a processed JSON-LD value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProcessedValue {
    /// Simple IRI reference
    Iri(IRI),
    /// Typed literal
    TypedLiteral { value: String, datatype: IRI },
    /// Language-tagged literal
    LanguageLiteral { value: String, language: String },
    /// Directional literal
    DirectionalLiteral {
        value: String,
        language: Option<String>,
        direction: String,
    },
    /// Blank node
    BlankNode(String),
    /// Collection (set or list)
    Collection(Vec<ProcessedValue>),
    /// Multiple values
    Multiple(Vec<ProcessedValue>),
}

/// JSON-LD Value Processor
#[derive(Debug, Clone)]
pub struct ValueProcessor {
    /// Default datatypes for common patterns
    #[allow(dead_code)]
    default_datatypes: HashMap<String, String>,
}

impl ValueProcessor {
    /// Create a new value processor
    pub fn new() -> Self {
        let mut default_datatypes = HashMap::new();

        // Common datatype mappings
        default_datatypes.insert(
            "string".to_string(),
            "http://www.w3.org/2001/XMLSchema#string".to_string(),
        );
        default_datatypes.insert(
            "integer".to_string(),
            "http://www.w3.org/2001/XMLSchema#integer".to_string(),
        );
        default_datatypes.insert(
            "decimal".to_string(),
            "http://www.w3.org/2001/XMLSchema#decimal".to_string(),
        );
        default_datatypes.insert(
            "double".to_string(),
            "http://www.w3.org/2001/XMLSchema#double".to_string(),
        );
        default_datatypes.insert(
            "float".to_string(),
            "http://www.w3.org/2001/XMLSchema#float".to_string(),
        );
        default_datatypes.insert(
            "boolean".to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean".to_string(),
        );
        default_datatypes.insert(
            "date".to_string(),
            "http://www.w3.org/2001/XMLSchema#date".to_string(),
        );
        default_datatypes.insert(
            "dateTime".to_string(),
            "http://www.w3.org/2001/XMLSchema#dateTime".to_string(),
        );
        default_datatypes.insert(
            "time".to_string(),
            "http://www.w3.org/2001/XMLSchema#time".to_string(),
        );
        default_datatypes.insert(
            "anyURI".to_string(),
            "http://www.w3.org/2001/XMLSchema#anyURI".to_string(),
        );

        Self { default_datatypes }
    }

    /// Process a JSON-LD value according to a term definition
    pub fn process_value(
        &self,
        value: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        match value {
            // String values
            Value::String(s) => self.process_string_value(s, term_def, context),

            // Numeric values
            Value::Number(n) => self.process_number_value(n, term_def, context),

            // Boolean values
            Value::Bool(b) => self.process_boolean_value(*b, term_def, context),

            // Null values
            Value::Null => Ok(vec![]), // Ignore null values

            // Array values (multiple values)
            Value::Array(arr) => {
                let mut results = Vec::new();
                for item in arr {
                    let processed = self.process_value(item, term_def, context)?;
                    results.extend(processed);
                }
                Ok(results)
            }

            // Object values (value objects or nested structures)
            Value::Object(obj) => {
                if self.is_value_object(obj) {
                    self.process_value_object(obj, term_def, context)
                } else {
                    // Nested structure - recursively process
                    self.process_nested_object(obj, term_def, context)
                }
            }
        }
    }

    /// Process a string value
    fn process_string_value(
        &self,
        value: &str,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        // Check if this is a blank node
        if value.starts_with("_:") {
            return Ok(vec![ProcessedValue::BlankNode(value.to_string())]);
        }

        // Check if this is an IRI (starts with http://, https://, etc.)
        if self.is_iri(value) {
            let iri = IRI::new(value)
                .map_err(|e| OwlError::ParseError(format!("Invalid IRI '{}': {}", value, e)))?;
            return Ok(vec![ProcessedValue::Iri(iri)]);
        }

        // Apply type coercion or language tagging from term definition
        if let Some(ref type_) = term_def.type_ {
            // Type coercion
            let datatype_iri = IRI::new(type_).map_err(|e| {
                OwlError::ParseError(format!("Invalid datatype IRI '{}': {}", type_, e))
            })?;
            Ok(vec![ProcessedValue::TypedLiteral {
                value: value.to_string(),
                datatype: datatype_iri,
            }])
        } else if let Some(ref language) = term_def.language {
            // Language tagging from term definition
            Ok(vec![ProcessedValue::LanguageLiteral {
                value: value.to_string(),
                language: language.clone(),
            }])
        } else if let Some(ref direction) = term_def.direction {
            // Directional literal
            Ok(vec![ProcessedValue::DirectionalLiteral {
                value: value.to_string(),
                language: context.language.clone(),
                direction: direction.clone(),
            }])
        } else if let Some(ref language) = context.language {
            // Language tagging from context
            Ok(vec![ProcessedValue::LanguageLiteral {
                value: value.to_string(),
                language: language.clone(),
            }])
        } else {
            // Default to string literal
            let datatype_iri =
                IRI::new("http://www.w3.org/2001/XMLSchema#string").map_err(|e| {
                    crate::core::error::OwlError::ParseError(format!("Invalid XSD string IRI: {}", e))
                })?;
            Ok(vec![ProcessedValue::TypedLiteral {
                value: value.to_string(),
                datatype: datatype_iri,
            }])
        }
    }

    /// Process a numeric value
    fn process_number_value(
        &self,
        value: &serde_json::Number,
        term_def: &TermDefinition,
        _context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        let value_str = value.to_string();

        // Determine appropriate datatype
        let datatype = if value.is_i64() {
            "http://www.w3.org/2001/XMLSchema#integer".to_string()
        } else if value.is_f64() {
            "http://www.w3.org/2001/XMLSchema#double".to_string()
        } else {
            // Check if it has decimal places
            if value_str.contains('.') {
                "http://www.w3.org/2001/XMLSchema#decimal".to_string()
            } else {
                "http://www.w3.org/2001/XMLSchema#integer".to_string()
            }
        };

        // Apply type coercion if specified
        let final_datatype = if let Some(ref type_) = term_def.type_ {
            type_.clone()
        } else {
            datatype
        };

        let datatype_iri = IRI::new(&final_datatype).map_err(|e| {
            OwlError::ParseError(format!("Invalid datatype IRI '{}': {}", final_datatype, e))
        })?;

        Ok(vec![ProcessedValue::TypedLiteral {
            value: value_str,
            datatype: datatype_iri,
        }])
    }

    /// Process a boolean value
    fn process_boolean_value(
        &self,
        value: bool,
        term_def: &TermDefinition,
        _context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        let datatype = if let Some(ref type_) = term_def.type_ {
            type_.clone()
        } else {
            "http://www.w3.org/2001/XMLSchema#boolean".to_string()
        };

        let datatype_iri = IRI::new(&datatype).map_err(|e| {
            OwlError::ParseError(format!("Invalid datatype IRI '{}': {}", datatype, e))
        })?;

        Ok(vec![ProcessedValue::TypedLiteral {
            value: value.to_string(),
            datatype: datatype_iri,
        }])
    }

    /// Check if a JSON object is a value object
    fn is_value_object(&self, obj: &Map<String, Value>) -> bool {
        // Value objects must have @value
        obj.contains_key("@value")
    }

    /// Process a value object (@value, @type, @language, @direction)
    fn process_value_object(
        &self,
        obj: &Map<String, Value>,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        let value = obj
            .get("@value")
            .ok_or_else(|| OwlError::ParseError("Value object must have @value".to_string()))?;

        let type_ = obj.get("@type").and_then(|v| v.as_str());
        let language_override = obj.get("@language").and_then(|v| v.as_str());
        let direction = obj.get("@direction").and_then(|v| v.as_str());

        // Process the base value
        let mut processed_values = self.process_value(value, term_def, context)?;

        // Apply overrides from the value object
        for processed in &mut processed_values {
            match processed {
                ProcessedValue::TypedLiteral {
                    ref mut datatype, ..
                } => {
                    if let Some(type_str) = type_ {
                        *datatype = IRI::new(type_str).map_err(|e| {
                            OwlError::ParseError(format!(
                                "Invalid datatype IRI '{}': {}",
                                type_str, e
                            ))
                        })?;
                    }
                }
                ProcessedValue::LanguageLiteral {
                    language: ref mut lang_value,
                    ..
                } => {
                    if let Some(lang_str) = language_override {
                        *lang_value = lang_str.to_string();
                    }
                }
                _ => {} // Other types not affected
            }
        }

        // Handle direction
        if let Some(dir_str) = direction {
            for processed in &mut processed_values {
                if let ProcessedValue::LanguageLiteral { value, language } = processed {
                    *processed = ProcessedValue::DirectionalLiteral {
                        value: value.clone(),
                        language: Some(language.clone()),
                        direction: dir_str.to_string(),
                    };
                }
            }
        }

        Ok(processed_values)
    }

    /// Process a nested object (not a value object)
    fn process_nested_object(
        &self,
        obj: &Map<String, Value>,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        // Check for @list
        if let Some(list) = obj.get("@list") {
            return self.process_list_value(list, term_def, context);
        }

        // Check for @set
        if let Some(set) = obj.get("@set") {
            return self.process_set_value(set, term_def, context);
        }

        // Check for @language map
        if obj.len() == 1
            && obj
                .keys()
                .next()
                .is_some_and(|k| !k.starts_with('@') && self.is_language_code(k))
        {
            return self.process_language_map(obj, term_def, context);
        }

        // Check for @index map
        if obj.values().any(|v| matches!(v, Value::Object(_))) {
            return self.process_index_map(obj, term_def, context);
        }

        // Default: treat as nested structure
        let mut results = Vec::new();
        for (key, val) in obj {
            if !key.starts_with('@') {
                let processed = self.process_value(val, term_def, context)?;
                results.extend(processed);
            }
        }
        Ok(results)
    }

    /// Process a list value
    fn process_list_value(
        &self,
        list: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        match list {
            Value::Array(arr) => {
                let mut items = Vec::new();
                for item in arr {
                    let processed = self.process_value(item, term_def, context)?;
                    items.extend(processed);
                }
                Ok(vec![ProcessedValue::Collection(items)])
            }
            _ => Err(OwlError::ParseError("@list must be an array".to_string())),
        }
    }

    /// Process a set value
    fn process_set_value(
        &self,
        set: &Value,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        // Sets are processed like arrays but maintain uniqueness
        match set {
            Value::Array(arr) => {
                let mut items = Vec::new();
                let mut seen = std::collections::HashSet::new();

                for item in arr {
                    let mut processed = self.process_value(item, term_def, context)?;
                    // Deduplicate items
                    for val in &mut processed {
                        if seen.insert(val.clone()) {
                            items.push(val.clone());
                        }
                    }
                }
                Ok(vec![ProcessedValue::Collection(items)])
            }
            _ => Err(OwlError::ParseError("@set must be an array".to_string())),
        }
    }

    /// Process a language map
    fn process_language_map(
        &self,
        map: &Map<String, Value>,
        term_def: &TermDefinition,
        _context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        let mut results = Vec::new();

        for (lang, value) in map {
            // Override language in term definition
            let mut lang_term_def = term_def.clone();
            lang_term_def.language = Some(lang.clone());

            let processed = self.process_value(value, &lang_term_def, _context)?;
            results.extend(processed);
        }

        Ok(results)
    }

    /// Process an index map
    fn process_index_map(
        &self,
        map: &Map<String, Value>,
        term_def: &TermDefinition,
        context: &Context,
    ) -> OwlResult<Vec<ProcessedValue>> {
        let mut results = Vec::new();

        for (_index, value) in map {
            // Index maps are treated as regular values, but the index could be used
            // for more sophisticated processing in the future
            let processed = self.process_value(value, term_def, context)?;
            results.extend(processed);
        }

        Ok(results)
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

    /// Check if a string is a language code
    fn is_language_code(&self, s: &str) -> bool {
        // Simple check for language codes (en, en-US, fr, etc.)
        s.len() >= 2 && s.len() <= 8 && s.chars().all(|c| c.is_alphabetic() || c == '-')
    }

    /// Expand a compact IRI using the context
    pub fn expand_compact_iri(&self, compact_iri: &str, context: &Context) -> OwlResult<String> {
        if let Some(colon_pos) = compact_iri.find(':') {
            let prefix = &compact_iri[..colon_pos];
            let suffix = &compact_iri[colon_pos + 1..];

            // Check for blank node
            if prefix == "_" {
                return Ok(format!("_:{}", suffix));
            }

            // Check term definitions for prefix
            if let Some(term_def) = context.terms.get(prefix) {
                if term_def.prefix {
                    if let Some(ref id) = term_def.id {
                        return Ok(format!("{}{}", id, suffix));
                    }
                }
            }
        }

        // If not a compact IRI, return as-is (could be a full IRI)
        Ok(compact_iri.to_string())
    }

    /// Get default datatype for a value
    pub fn get_default_datatype(&self, value: &Value) -> Option<&str> {
        match value {
            Value::String(_) => Some("http://www.w3.org/2001/XMLSchema#string"),
            Value::Number(n) => {
                if n.is_i64() {
                    Some("http://www.w3.org/2001/XMLSchema#integer")
                } else {
                    Some("http://www.w3.org/2001/XMLSchema#double")
                }
            }
            Value::Bool(_) => Some("http://www.w3.org/2001/XMLSchema#boolean"),
            _ => None,
        }
    }
}

impl Default for ValueProcessor {
    fn default() -> Self {
        Self::new()
    }
}
