//! JSON-LD Context Management
//!
//! This module provides functionality for managing JSON-LD contexts,
//! including term definitions, vocabulary mappings, and context resolution.

use crate::core::error::{OwlError, OwlResult};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Represents a JSON-LD context
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    /// Base IRI for resolving relative IRIs
    pub base: Option<String>,
    /// Vocabulary mapping for default term
    pub vocab: Option<String>,
    /// Default language for string literals
    pub language: Option<String>,
    /// Default text direction
    pub direction: Option<String>,
    /// Whether this context is protected
    pub is_protected: bool,
    /// Term definitions
    pub terms: HashMap<String, TermDefinition>,
}

impl Context {
    /// Expand a term using this context
    pub fn expand_term(&self, iri: &str) -> Option<String> {
        // Check if it's a keyword
        if iri.starts_with('@') {
            return Some(iri.to_string());
        }

        // Check if it's a term in the context
        if let Some(term_def) = self.terms.get(iri) {
            if let Some(id) = &term_def.id {
                return Some(id.clone());
            }
        }

        // Check for vocabulary mapping
        if let Some(vocab) = &self.vocab {
            if !iri.contains(':') {
                return Some(format!("{}{}", vocab, iri));
            }
        }

        // Return as-is if no mapping found
        Some(iri.to_string())
    }
}

/// Represents a term definition in a context
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TermDefinition {
    /// IRI mapping for the term
    pub id: Option<String>,
    /// Type mapping for the term
    pub type_: Option<String>,
    /// Container mapping
    pub container: Option<Container>,
    /// Language mapping
    pub language: Option<String>,
    /// Direction mapping
    pub direction: Option<String>,
    /// Reverse property flag
    pub reverse: Option<Box<Context>>,
    /// Index mapping
    pub index: Option<String>,
    /// Context for nested terms
    pub context: Option<Box<Context>>,
    /// Protected flag
    pub protected: bool,
    /// Prefix flag
    pub prefix: bool,
}

/// Represents container types in JSON-LD
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Container {
    Language,
    Set,
    List,
    Index(String),
    Graph,
    Id,
    Type,
}

/// Context manager for handling multiple contexts
#[derive(Debug, Clone)]
pub struct ContextManager {
    /// Active contexts stack
    active_contexts: Vec<Context>,
    /// Remote context cache
    #[allow(dead_code)]
    context_cache: HashMap<String, Context>,
}

impl ContextManager {
    /// Create a new context manager
    pub fn new() -> Self {
        Self {
            active_contexts: vec![Context::default()],
            context_cache: HashMap::new(),
        }
    }

    /// Parse a JSON-LD context
    pub fn parse_context(&self, context: &Value) -> OwlResult<Context> {
        match context {
            Value::String(base) => {
                // Remote context - simplified for now
                Ok(Context {
                    base: Some(base.clone()),
                    ..Default::default()
                })
            }
            Value::Object(obj) => self.parse_object_context(obj),
            Value::Null => Ok(Context::default()),
            _ => Err(OwlError::ParseError("Invalid context format".to_string())),
        }
    }

    /// Parse an object context
    fn parse_object_context(&self, obj: &Map<String, Value>) -> OwlResult<Context> {
        let mut context = Context::default();

        // Parse @base
        if let Some(Value::String(base_str)) = obj.get("@base") {
            context.base = Some(base_str.clone());
        }

        // Parse @vocab
        if let Some(Value::String(vocab_str)) = obj.get("@vocab") {
            context.vocab = Some(vocab_str.clone());
        }

        // Parse @language
        if let Some(Value::String(lang_str)) = obj.get("@language") {
            context.language = Some(lang_str.clone());
        }

        // Parse @direction
        if let Some(Value::String(dir_str)) = obj.get("@direction") {
            context.direction = Some(dir_str.clone());
        }

        // Parse @protected
        if let Some(Value::Bool(protected_bool)) = obj.get("@protected") {
            context.is_protected = *protected_bool;
        }

        // Parse term definitions
        for (key, value) in obj {
            if key.starts_with('@') {
                continue;
            }

            let term_def = self.parse_term_definition(value)?;
            context.terms.insert(key.clone(), term_def);
        }

        Ok(context)
    }

    /// Parse a term definition
    fn parse_term_definition(&self, value: &Value) -> OwlResult<TermDefinition> {
        match value {
            Value::String(id) => Ok(TermDefinition {
                id: Some(id.clone()),
                ..Default::default()
            }),
            Value::Object(obj) => {
                let mut term_def = TermDefinition::default();

                // Parse @id
                if let Some(Value::String(id_str)) = obj.get("@id") {
                    term_def.id = Some(id_str.clone());
                }

                // Parse @type
                if let Some(Value::String(type_str)) = obj.get("@type") {
                    term_def.type_ = Some(type_str.clone());
                }

                // Parse @container
                if let Some(container) = obj.get("@container") {
                    term_def.container = self.parse_container(container)?;
                }

                // Parse @language
                if let Some(Value::String(lang_str)) = obj.get("@language") {
                    term_def.language = Some(lang_str.clone());
                }

                // Parse @direction
                if let Some(Value::String(dir_str)) = obj.get("@direction") {
                    term_def.direction = Some(dir_str.clone());
                }

                // Parse @reverse
                if let Some(Value::String(_reverse_str)) = obj.get("@reverse") {
                    term_def.reverse = Some(Box::new(Context::default())); // Simplified
                }

                // Parse @index
                if let Some(Value::String(index_str)) = obj.get("@index") {
                    term_def.index = Some(index_str.clone());
                }

                // Parse @context
                if let Some(ctx) = obj.get("@context") {
                    let context = self.parse_context(ctx)?;
                    term_def.context = Some(Box::new(context));
                }

                // Parse @protected
                if let Some(Value::Bool(protected_bool)) = obj.get("@protected") {
                    term_def.protected = *protected_bool;
                }

                // Parse @prefix
                if let Some(Value::Bool(prefix_bool)) = obj.get("@prefix") {
                    term_def.prefix = *prefix_bool;
                }

                Ok(term_def)
            }
            _ => Err(OwlError::ParseError(
                "Invalid term definition format".to_string(),
            )),
        }
    }

    /// Parse container specification
    #[allow(clippy::only_used_in_recursion)]
    fn parse_container(&self, container: &Value) -> OwlResult<Option<Container>> {
        match container {
            Value::String(container_str) => match container_str.as_str() {
                "@language" => Ok(Some(Container::Language)),
                "@set" => Ok(Some(Container::Set)),
                "@list" => Ok(Some(Container::List)),
                "@graph" => Ok(Some(Container::Graph)),
                "@id" => Ok(Some(Container::Id)),
                "@type" => Ok(Some(Container::Type)),
                _ => Ok(None),
            },
            Value::Array(arr) => {
                // Array of containers - simplified to take first
                if let Some(first_container) = arr.first() {
                    self.parse_container(first_container)
                } else {
                    Ok(None)
                }
            }
            Value::Object(obj) => {
                // Handle @index with specific index value
                if let Some(Value::String(index_str)) = obj.get("@index") {
                    return Ok(Some(Container::Index(index_str.clone())));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Expand IRI using current context
    pub fn expand_iri(&self, iri: &str) -> OwlResult<String> {
        let context = self.active_contexts.last().ok_or_else(|| {
            crate::core::error::OwlError::ParseError(
                "No active context available for IRI expansion".to_string(),
            )
        })?;

        // Check if it's a keyword
        if iri.starts_with('@') {
            return Ok(iri.to_string());
        }

        // Check if it's a term in the context
        if let Some(term_def) = context.terms.get(iri) {
            if let Some(id) = &term_def.id {
                return Ok(id.clone());
            }
        }

        // Check for vocabulary mapping
        if let Some(vocab) = &context.vocab {
            if !iri.contains(':') {
                return Ok(format!("{}{}", vocab, iri));
            }
        }

        // Return as-is if no mapping found
        Ok(iri.to_string())
    }

    /// Compact IRI using current context
    pub fn compact_iri(&self, iri: &str) -> OwlResult<String> {
        let context = self.active_contexts.last().ok_or_else(|| {
            crate::core::error::OwlError::ParseError(
                "No active context available for IRI expansion".to_string(),
            )
        })?;

        // Check for vocabulary mapping
        if let Some(vocab) = &context.vocab {
            if iri.starts_with(vocab) && iri.len() > vocab.len() {
                let suffix = &iri[vocab.len()..];
                if !context.terms.contains_key(suffix) {
                    return Ok(suffix.to_string());
                }
            }
        }

        // Check for term mappings
        for (term, term_def) in &context.terms {
            if let Some(id) = &term_def.id {
                if id == iri {
                    return Ok(term.clone());
                }
            }
        }

        Ok(iri.to_string())
    }

    /// Push a new context onto the stack
    pub fn push_context(&mut self, context: Context) {
        self.active_contexts.push(context);
    }

    /// Pop the current context from the stack
    pub fn pop_context(&mut self) -> Option<Context> {
        if self.active_contexts.len() > 1 {
            self.active_contexts.pop()
        } else {
            None
        }
    }

    /// Get the current active context
    pub fn current_context(&self) -> OwlResult<&Context> {
        self.active_contexts.last().ok_or_else(|| {
            crate::core::error::OwlError::ParseError("No active context available".to_string())
        })
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}
