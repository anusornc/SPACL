//! RDF/XML format parser for OWL2 ontologies
//!
//! Implements parsing of the RDF/XML serialization format with full specification compliance.
//! This module combines streaming and legacy parsing approaches for maximum compatibility.

use crate::core::error::{OwlError, OwlResult};
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml_legacy::RdfXmlLegacyParser;
use crate::parser::rdf_xml_streaming::RdfXmlStreamingParser;
use crate::parser::{OntologyParser, ParserConfig};
use std::io::{BufReader, Read};
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

    /// Remove an internal DTD subset and expand its simple general entities.
    ///
    /// `rio-xml` intentionally rejects DTD declarations. OWL 2 RDF/XML test
    /// cases use small internal entity tables for namespace aliases, so they
    /// can be handled safely without enabling external entity resolution.
    fn expand_internal_entities(content: &str) -> OwlResult<String> {
        let Some(doctype_start) = content.find("<!DOCTYPE") else {
            return Ok(content.to_string());
        };
        let subset_start = content[doctype_start..]
            .find('[')
            .map(|offset| doctype_start + offset)
            .ok_or_else(|| {
                OwlError::ParseError("DOCTYPE without an internal subset".to_string())
            })?;
        let subset_end = content[subset_start + 1..]
            .find("]>")
            .map(|offset| subset_start + 1 + offset)
            .ok_or_else(|| OwlError::ParseError("unterminated internal DTD subset".to_string()))?;

        let subset = &content[subset_start + 1..subset_end];
        let mut entities = Vec::new();
        let mut cursor = 0;
        while let Some(relative_start) = subset[cursor..].find("<!ENTITY") {
            let declaration_start = cursor + relative_start;
            let relative_end = subset[declaration_start..]
                .find('>')
                .ok_or_else(|| OwlError::ParseError("unterminated DTD entity".to_string()))?;
            let declaration_end = declaration_start + relative_end;
            let declaration = subset[declaration_start + "<!ENTITY".len()..declaration_end].trim();
            if declaration.starts_with('%') {
                return Err(OwlError::ParseError(
                    "parameter DTD entities are not supported".to_string(),
                ));
            }

            let mut parts = declaration.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default().trim();
            if name.is_empty() || value.len() < 2 {
                return Err(OwlError::ParseError(
                    "invalid internal DTD entity declaration".to_string(),
                ));
            }
            let quote = value.as_bytes()[0] as char;
            if !matches!(quote, '"' | '\'') || !value.ends_with(quote) {
                return Err(OwlError::ParseError(
                    "only quoted internal DTD entities are supported".to_string(),
                ));
            }
            entities.push((name.to_string(), value[1..value.len() - 1].to_string()));
            cursor = declaration_end + 1;
        }

        let mut expanded = String::with_capacity(content.len());
        expanded.push_str(&content[..doctype_start]);
        expanded.push_str(&content[subset_end + 2..]);
        for _ in 0..=entities.len() {
            let mut changed = false;
            for (name, value) in &entities {
                let reference = format!("&{name};");
                if expanded.contains(&reference) {
                    expanded = expanded.replace(&reference, value);
                    if expanded.len() > 64 * 1024 * 1024 {
                        return Err(OwlError::ParseError(
                            "expanded XML exceeds the internal entity size limit".to_string(),
                        ));
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(expanded)
    }

    fn file_contains_doctype(path: &Path) -> bool {
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };
        let mut sample = String::new();
        BufReader::new(file)
            .take(128 * 1024)
            .read_to_string(&mut sample)
            .is_ok()
            && sample.contains("<!DOCTYPE")
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

        let expanded_content = Self::expand_internal_entities(content)?;
        let content = expanded_content.as_str();

        // Choose parsing strategy based on configuration and feature availability
        #[cfg(feature = "rio-xml")]
        {
            if !self.config.strict_validation {
                // Try streaming parser for non-strict mode
                let mut streaming_parser = RdfXmlStreamingParser::new(self.config.clone());
                match streaming_parser.parse_content(content) {
                    Ok(ontology) => return Ok(ontology),
                    Err(e) if super::rdf_xml_common::prevents_lossy_fallback(&e) => return Err(e),
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

        let has_internal_dtd = Self::file_contains_doctype(path);

        #[cfg(feature = "rio-xml")]
        {
            if !self.config.strict_validation && !has_internal_dtd {
                let mut streaming_parser = RdfXmlStreamingParser::new(self.config.clone());
                match streaming_parser.parse_file(path) {
                    Ok(ontology) => return Ok(ontology),
                    Err(e) if super::rdf_xml_common::prevents_lossy_fallback(&e) => return Err(e),
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

#[cfg(test)]
mod tests {
    use super::RdfXmlParser;

    #[test]
    fn expands_simple_internal_xml_entities_without_external_resolution() {
        let content = r#"<!DOCTYPE rdf:RDF [
          <!ENTITY ex "http://example.org/">
        ]>
        <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                 xmlns:owl="http://www.w3.org/2002/07/owl#"
                 xmlns:ex="&ex;">
          <owl:Ontology rdf:about="&ex;ontology"/>
        </rdf:RDF>"#;

        let expanded = RdfXmlParser::expand_internal_entities(content).unwrap();
        assert!(!expanded.contains("<!DOCTYPE"));
        assert!(expanded.contains("xmlns:ex=\"http://example.org/\""));
        assert!(expanded.contains("rdf:about=\"http://example.org/ontology\""));
    }

    #[test]
    fn rejects_external_internal_dtd_entities() {
        let content = r#"<!DOCTYPE rdf:RDF [
          <!ENTITY ex SYSTEM "https://example.org/external">
        ]>
        <rdf:RDF/>"#;

        let error = RdfXmlParser::expand_internal_entities(content).unwrap_err();
        assert!(error
            .to_string()
            .contains("only quoted internal DTD entities"));
    }
}
