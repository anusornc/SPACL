//! Error types for the OWL2 reasoner

use thiserror::Error;

/// OWL2 Reasoner error type
#[derive(Error, Debug)]
pub enum OwlError {
    /// IRI-related errors
    #[error("Invalid IRI: {0}")]
    InvalidIRI(String),

    /// IRI parsing errors with context
    #[error("IRI parse error: {iri}, context: {context}")]
    IriParseError { iri: String, context: String },

    /// IRI creation errors with format context
    #[error("IRI creation failed: {iri_str}")]
    IriCreationError { iri_str: String },

    /// Unknown namespace prefix
    #[error("Unknown prefix: {0}")]
    UnknownPrefix(String),

    /// Parse errors
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Parse errors with line and column information
    #[error("Parse error at line {line}, column {column}: {message}")]
    ParseErrorWithLocation {
        line: usize,
        column: usize,
        message: String,
    },

    /// Serialization errors
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Reasoning errors
    #[error("Reasoning error: {0}")]
    ReasoningError(String),

    /// Tableaux reasoning errors
    #[error("Tableaux reasoning error: {node_id}: {message}")]
    TableauxError { node_id: usize, message: String },

    /// Graph operation errors
    #[error("Graph operation error: {operation} failed: {message}")]
    GraphError { operation: String, message: String },

    /// Query errors
    #[error("Query error: {0}")]
    QueryError(String),

    /// Storage errors
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Cache operation errors
    #[error("Cache error: {operation} failed: {message}")]
    CacheError { operation: String, message: String },

    /// Lock contention errors
    #[error("Lock contention error: {lock_type} lock failed after {timeout_ms}ms: {message}")]
    LockError {
        lock_type: String,
        timeout_ms: u64,
        message: String,
    },

    /// Validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Entity validation errors
    #[error("Entity validation error: {entity_type} '{name}': {message}")]
    EntityValidationError {
        entity_type: String,
        name: String,
        message: String,
    },

    /// Axiom validation errors
    #[error("Axiom validation error: {axiom_type}: {message}")]
    AxiomValidationError { axiom_type: String, message: String },

    /// OWL2 specification violations
    #[error("OWL2 specification violation: {0}")]
    OwlViolation(String),

    /// Profile validation errors
    #[error("Profile validation error: {profile}: {message}")]
    ProfileViolation { profile: String, message: String },

    /// Inconsistent ontology
    #[error("Inconsistent ontology: {0}")]
    InconsistentOntology(String),

    /// Resource limit exceeded errors
    #[error("Resource limit exceeded: {resource_type} limit {limit} reached: {message}")]
    ResourceLimitExceeded {
        resource_type: String,
        limit: usize,
        message: String,
    },

    /// Timeout errors
    #[error("Timeout error: {operation} timed out after {timeout_ms}ms")]
    TimeoutError { operation: String, timeout_ms: u64 },

    /// Configuration errors
    #[error("Configuration error: {parameter}: {message}")]
    ConfigError { parameter: String, message: String },

    /// Import resolution errors
    #[error("Import resolution error for {iri}: {message}")]
    ImportResolutionError {
        iri: crate::core::iri::IRI,
        message: String,
    },

    /// I/O errors
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// UTF-8 conversion errors
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    /// JSON serialization errors
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Unexpected structure errors (replacing panic! calls)
    #[error("Unexpected structure: {0}")]
    UnexpectedStructure(String),

    /// Expected entity type errors
    #[error("Expected named object property")]
    ExpectedNamedObjectProperty,

    #[error("Expected literal value")]
    ExpectedLiteralValue,

    /// Expected specific axiom types
    #[error("Expected FunctionalProperty axiom")]
    ExpectedFunctionalPropertyAxiom,

    #[error("Expected ReflexiveProperty axiom")]
    ExpectedReflexivePropertyAxiom,

    #[error("Expected TransitiveProperty axiom")]
    ExpectedTransitivePropertyAxiom,

    #[error("Expected SubDataProperty axiom")]
    ExpectedSubDataPropertyAxiom,

    #[error("Expected FunctionalDataProperty axiom")]
    ExpectedFunctionalDataPropertyAxiom,

    #[error("Expected EquivalentDataProperties axiom")]
    ExpectedEquivalentDataPropertiesAxiom,

    #[error("Expected DisjointDataProperties axiom")]
    ExpectedDisjointDataPropertiesAxiom,

    #[error("Expected SameIndividual axiom")]
    ExpectedSameIndividualAxiom,

    #[error("Expected DifferentIndividuals axiom")]
    ExpectedDifferentIndividualsAxiom,

    #[error("Expected SubPropertyChainOf axiom")]
    ExpectedSubPropertyChainOfAxiom,

    #[error("Expected InverseObjectProperties axiom")]
    ExpectedInverseObjectPropertiesAxiom,

    /// Other errors
    #[error("Other error: {0}")]
    Other(String),
}

/// Result type for OWL2 operations
pub type OwlResult<T> = Result<T, OwlError>;

/// Error context builder for better error messages
#[derive(Debug, Clone)]
pub struct ErrorContext {
    operation: String,
    details: Vec<(String, String)>,
}

impl ErrorContext {
    /// Create a new error context
    #[must_use]
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            details: Vec::new(),
        }
    }

    /// Add a detail to the error context
    #[must_use]
    pub fn add_detail(mut self, key: &str, value: &str) -> Self {
        self.details.push((key.to_string(), value.to_string()));
        self
    }

    /// Build an error with context
    #[must_use]
    pub fn build(self, error: OwlError) -> OwlError {
        let context_str = if self.details.is_empty() {
            self.operation
        } else {
            format!(
                "{}: {}",
                self.operation,
                self.details
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        match error {
            OwlError::ReasoningError(msg) => {
                OwlError::ReasoningError(format!("{context_str}:{msg}"))
            }
            OwlError::StorageError(msg) => {
                OwlError::StorageError(format!("{context_str}:{msg}"))
            }
            OwlError::ParseError(msg) => OwlError::ParseError(format!("{context_str}:{msg}")),
            _ => error,
        }
    }
}
