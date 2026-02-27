//! RDF/XML format parser for OWL2 ontologies
//!
//! Implements parsing of the RDF/XML serialization format with full specification compliance.
//! This module combines streaming and legacy parsing approaches for maximum compatibility.

use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml_legacy::RdfXmlLegacyParser;
use crate::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use crate::parser::{OntologyParser, ParserConfig};
use std::path::Path;

/// RDF/XML format parser with dual-mode operation
pub struct RdfXmlParser {
    pub config: ParserConfig,
}

impl Default for RdfXmlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl RdfXmlParser {
    /// Create a new RDF/XML parser with default configuration
    pub fn new() -> Self {
        Self::with_config(ParserConfig::default())
    }

    /// Create a new RDF/XML parser with custom configuration
    pub fn with_config(config: ParserConfig) -> Self {
        Self { config }
    }
}

impl OntologyParser for RdfXmlParser {
    /// Parse RDF/XML content and build an ontology
    fn parse_str(&self, content: &str) -> OwlResult<Ontology> {
        if self.config.strict_validation && content.trim().is_empty() {
            return Err(crate::core::error::OwlError::ValidationError(
                crate::parser::rdf_xml_common::ERR_EMPTY_ONTOLOGY.to_string(),
            ));
        }

        // Choose parsing strategy based on configuration and feature availability
        #[cfg(feature = "rio-xml")]
        {
            if !self.config.strict_validation {
                // Try streaming parser for non-strict mode
                let mut streaming_parser = RdfXmlStreamingParser::new(self.config.clone());
                match streaming_parser.parse_content(content) {
                    Ok(ontology) => return Ok(ontology),
                    Err(e) => {
                        // If streaming parser fails, try legacy parser as fallback
                        eprintln!(
                            "[FALLBACK] Streaming parser failed: {}. Trying legacy parser...",
                            e
                        );
                        log::warn!("Streaming parser failed: {}. Trying legacy parser...", e);
                        let mut legacy_config = self.config.clone();
                        legacy_config.strict_validation = false; // Disable strict validation for fallback
                        let mut legacy_parser = RdfXmlLegacyParser::new(legacy_config);
                        return legacy_parser.parse_content(content);
                    }
                }
            }
        }

        // Use legacy parser for strict mode or when streaming is not available
        let mut legacy_parser = RdfXmlLegacyParser::new(self.config.clone());
        let mut ontology = legacy_parser.parse_content(content)?;

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

    /// Parse RDF/XML file and build an ontology
    fn parse_file(&self, path: &Path) -> OwlResult<Ontology> {
        use std::fs;

        // Check file size using metadata to avoid loading large files into memory.
        if self.config.max_file_size > 0 {
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.len() as usize > self.config.max_file_size {
                    return Err(crate::core::error::OwlError::ValidationError(
                        "File size exceeds maximum allowed size".to_string(),
                    ));
                }
            }
        }

        #[cfg(feature = "rio-xml")]
        {
            if !self.config.strict_validation {
                let mut streaming_parser = RdfXmlStreamingParser::new(self.config.clone());
                match streaming_parser.parse_file(path) {
                    Ok(ontology) => return Ok(ontology),
                    Err(e) => {
                        eprintln!(
                            "[FALLBACK] Streaming parser failed: {}. Trying legacy parser...",
                            e
                        );
                        log::warn!("Streaming parser failed: {}. Trying legacy parser...", e);
                    }
                }
            }
        }

        let content = fs::read_to_string(path).map_err(crate::core::error::OwlError::IoError)?;

        if self.config.max_file_size > 0 && content.len() > self.config.max_file_size {
            return Err(crate::core::error::OwlError::ValidationError(
                "File size exceeds maximum allowed size".to_string(),
            ));
        }

        // Use parse_str which contains the parsing logic
        self.parse_str(&content)
    }

    /// Get parser format name
    fn format_name(&self) -> &'static str {
        "RDF/XML"
    }
}
