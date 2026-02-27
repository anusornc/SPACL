//! Constants for the OWL2 reasoner
//!
//! This module centralizes all magic numbers, timeouts, and commonly used IRIs
//! to improve maintainability and reduce hardcoded values.

/// RDF vocabulary IRIs
pub mod rdf {
    use crate::core::iri::IRI;

    /// rdf:type property
    pub fn type_property() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").expect("Valid RDF type IRI")
    }

    /// rdf:first property
    pub fn first() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#first").expect("Valid RDF first IRI")
    }

    /// rdf:rest property
    pub fn rest() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest").expect("Valid RDF rest IRI")
    }

    /// rdf:nil resource
    pub fn nil() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil").expect("Valid RDF nil IRI")
    }

    /// rdf:subject property
    pub fn subject() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#subject")
            .expect("Valid RDF subject IRI")
    }

    /// rdf:predicate property
    pub fn predicate() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate")
            .expect("Valid RDF predicate IRI")
    }

    /// rdf:object property
    pub fn object() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#object").expect("Valid RDF object IRI")
    }

    /// rdf:Statement class
    pub fn statement() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement")
            .expect("Valid RDF Statement IRI")
    }

    /// rdf:Seq class
    pub fn seq() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#Seq").expect("Valid RDF Seq IRI")
    }

    /// rdf:Bag class
    pub fn bag() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#Bag").expect("Valid RDF Bag IRI")
    }

    /// rdf:Alt class
    pub fn alt() -> IRI {
        IRI::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#Alt").expect("Valid RDF Alt IRI")
    }
}

/// OWL vocabulary IRIs
pub mod owl {
    use crate::core::iri::IRI;

    /// owl:Thing class
    pub fn thing() -> IRI {
        IRI::new("http://www.w3.org/2002/07/owl#Thing").expect("Valid OWL Thing IRI")
    }

    /// owl:Nothing class
    pub fn nothing() -> IRI {
        IRI::new("http://www.w3.org/2002/07/owl#Nothing").expect("Valid OWL Nothing IRI")
    }
}

/// XSD vocabulary IRIs
pub mod xsd {
    use crate::core::iri::IRI;

    /// xsd:string datatype
    pub fn string() -> IRI {
        IRI::new("http://www.w3.org/2001/XMLSchema#string").expect("Valid XSD string IRI")
    }

    /// xsd:integer datatype
    pub fn integer() -> IRI {
        IRI::new("http://www.w3.org/2001/XMLSchema#integer").expect("Valid XSD integer IRI")
    }

    /// xsd:boolean datatype
    pub fn boolean() -> IRI {
        IRI::new("http://www.w3.org/2001/XMLSchema#boolean").expect("Valid XSD boolean IRI")
    }

    /// xsd:dateTime datatype
    pub fn datetime() -> IRI {
        IRI::new("http://www.w3.org/2001/XMLSchema#dateTime").expect("Valid XSD dateTime IRI")
    }

    /// xsd:langString datatype
    pub fn lang_string() -> IRI {
        IRI::new("http://www.w3.org/2001/XMLSchema#langString").expect("Valid XSD langString IRI")
    }
}

/// Test namespace IRIs
pub mod test {
    use crate::core::iri::IRI;

    /// Base test namespace
    pub fn base() -> IRI {
        IRI::new("http://example.org/").expect("Valid test base IRI")
    }

    /// Test Person class
    pub fn person() -> IRI {
        IRI::new("http://example.org/Person").expect("Valid test Person IRI")
    }

    /// Test Animal class
    pub fn animal() -> IRI {
        IRI::new("http://example.org/Animal").expect("Valid test Animal IRI")
    }

    /// Test Human class
    pub fn human() -> IRI {
        IRI::new("http://example.org/Human").expect("Valid test Human IRI")
    }

    /// Test Parent class
    pub fn parent() -> IRI {
        IRI::new("http://example.org/Parent").expect("Valid test Parent IRI")
    }

    /// Test individual
    pub fn individual(name: &str) -> IRI {
        IRI::new(format!("http://example.org/{}", name)).expect("Valid test individual IRI")
    }

    /// Test property
    pub fn property(name: &str) -> IRI {
        IRI::new(format!("http://example.org/{}", name)).expect("Valid test property IRI")
    }
}

/// Performance and configuration constants
pub mod config {
    /// Default cache size for LRU caches
    pub const DEFAULT_CACHE_SIZE: usize = 1000;

    /// Default timeout for operations in milliseconds
    pub const DEFAULT_TIMEOUT_MS: u64 = 5000;

    /// Maximum file size for parsing (10MB)
    pub const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

    /// Maximum number of concurrent operations
    pub const MAX_CONCURRENT_OPERATIONS: usize = 100;

    /// Default buffer size for I/O operations
    pub const DEFAULT_BUFFER_SIZE: usize = 8192;

    /// Maximum recursion depth for reasoning
    pub const MAX_REASONING_DEPTH: usize = 1000;

    /// Cache expiration time in seconds
    pub const CACHE_EXPIRATION_SECONDS: u64 = 3600;

    /// Memory limit in bytes (1GB)
    pub const MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;
}

/// Error messages
pub mod messages {
    /// General error message for invalid IRI
    pub const INVALID_IRI: &str = "Invalid IRI format";

    /// Error message for parse failures
    pub const PARSE_ERROR: &str = "Failed to parse input";

    /// Error message for timeout
    pub const TIMEOUT_ERROR: &str = "Operation timed out";

    /// Error message for resource limits
    pub const RESOURCE_LIMIT_EXCEEDED: &str = "Resource limit exceeded";

    /// Error message for inconsistent ontology
    pub const INCONSISTENT_ONTOLOGY: &str = "Ontology is inconsistent";
}

/// Blank node prefix
pub const BLANK_NODE_PREFIX: &str = "_:";

/// RDF element prefix for container properties
pub const RDF_ELEMENT_PREFIX: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#_";
