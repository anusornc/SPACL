//! Streaming/incremental ontology parser
//!
//! This module provides parsers that process ontology files incrementally
//! without loading the entire file into memory. This is essential for
//! handling very large ontologies (>100K classes).
//!
//! # Example
//! ```rust
//! use owl2_reasoner::parser::streaming::StreamingOwlXmlParser;
//! use std::io::BufReader;
//! use std::fs::File;
//!
//! let file = File::open("large.owl").unwrap();
//! let mut parser = StreamingOwlXmlParser::new(BufReader::new(file)).unwrap();
//!
//! while let Some(axiom) = parser.next_axiom().unwrap() {
//!     reasoner.add_axiom(axiom)?;
//! }
//! ```

pub mod owl_xml_streaming;

pub use owl_xml_streaming::{StreamingOwlXmlParser, parse_owl_xml_streaming};

use crate::core::error::OwlResult;
use crate::logic::axioms::Axiom;
use std::io::{BufRead, Read};

/// Parse an ontology with progress reporting
pub fn parse_with_progress<R: Read>(
    reader: R,
    mut callback: impl FnMut(usize, usize), // (axioms_parsed, bytes_read)
) -> OwlResult<Vec<Axiom>> {
    use std::io::BufReader;
    
    let buf_reader = BufReader::new(reader);
    let mut parser = StreamingOwlXmlParser::new(buf_reader)?;
    let mut axioms = Vec::new();
    let mut last_reported = 0;
    
    while let Some(axiom) = parser.next_axiom()? {
        axioms.push(axiom);
        
        // Report every 100 axioms
        if axioms.len() - last_reported >= 100 {
            callback(axioms.len(), parser.bytes_read());
            last_reported = axioms.len();
        }
    }
    
    callback(axioms.len(), parser.bytes_read());
    Ok(axioms)
}
