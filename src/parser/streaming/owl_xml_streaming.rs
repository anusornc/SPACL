//! Streaming OWL/XML parser
//!
//! Parses OWL/XML files incrementally using quick-xml's streaming API.
//! This avoids loading the entire file into memory.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{BufRead, BufReader, Read};

use crate::core::entities::Class;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::logic::axioms::{
    Axiom, ClassAssertionAxiom, SubClassOfAxiom,
    class_expressions::ClassExpression,
};

/// Streaming OWL/XML parser
pub struct StreamingOwlXmlParser<R: BufRead> {
    reader: Reader<R>,
    buf: Vec<u8>,
    line_count: usize,
    bytes_read: usize,
    pending_axioms: Vec<Axiom>,
}

impl<R: BufRead> StreamingOwlXmlParser<R> {
    /// Create a new streaming parser
    pub fn new(reader: R) -> OwlResult<Self> {
        let mut xml_reader = Reader::from_reader(reader);
        xml_reader.trim_text(true);
        
        Ok(Self {
            reader: xml_reader,
            buf: Vec::with_capacity(4096),
            line_count: 0,
            bytes_read: 0,
            pending_axioms: Vec::new(),
        })
    }
    
    /// Parse the next axiom
    pub fn next_axiom(&mut self) -> OwlResult<Option<Axiom>> {
        // Return pending axioms first
        if !self.pending_axioms.is_empty() {
            return Ok(self.pending_axioms.pop());
        }
        
        // Read events until we find an axiom
        loop {
            self.buf.clear();
            match self.reader.read_event_into(&mut self.buf) {
                Ok(Event::Start(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    
                    // Track approximate line count
                    self.bytes_read += self.buf.len();
                    
                    // Check for subclass axiom
                    if name.ends_with("SubClassOf") {
                        if let Some(axiom) = self.parse_subclass_axiom()? {
                            return Ok(Some(axiom));
                        }
                    }
                    // Check for class assertion
                    else if name.ends_with("ClassAssertion") {
                        if let Some(axiom) = self.parse_class_assertion()? {
                            return Ok(Some(axiom));
                        }
                    }
                }
                Ok(Event::Text(e)) => {
                    self.bytes_read += e.len();
                }
                Ok(Event::Eof) => {
                    return Ok(None);
                }
                Err(e) => {
                    return Err(OwlError::ParseError(format!(
                        "XML parsing error: {:?}",
                        e
                    )));
                }
                _ => {}
            }
        }
    }
    
    /// Get parsing progress (approximate based on bytes read)
    pub fn progress(&self) -> f64 {
        // Progress tracking requires knowing total size
        // This is approximate - return 0.0 for now
        0.0
    }
    
    /// Get approximate byte position
    pub fn bytes_read(&self) -> usize {
        self.bytes_read
    }
    
    // === Private parsing methods ===
    
    fn parse_subclass_axiom(&mut self) -> OwlResult<Option<Axiom>> {
        let mut sub_class = None;
        let mut super_class = None;
        let mut depth = 1;
        
        loop {
            self.buf.clear();
            match self.reader.read_event_into(&mut self.buf) {
                Ok(Event::Start(e)) => {
                    depth += 1;
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    
                    if name.ends_with("Class") && depth == 2 {
                        // First class is subclass
                        if sub_class.is_none() {
                            sub_class = Some(self.parse_class_reference(&e)?);
                        } else if super_class.is_none() {
                            super_class = Some(self.parse_class_reference(&e)?);
                        }
                    }
                }
                Ok(Event::Empty(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    
                    if name.ends_with("Class") {
                        if sub_class.is_none() {
                            sub_class = Some(self.parse_class_reference(&e)?);
                        } else if super_class.is_none() {
                            super_class = Some(self.parse_class_reference(&e)?);
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    if name.ends_with("SubClassOf") {
                        break;
                    }
                    depth -= 1;
                }
                Ok(Event::Eof) => break,
                _ => {}
            }
        }
        
        if let (Some(sub), Some(sup)) = (sub_class, super_class) {
            Ok(Some(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(sub, sup)))))
        } else {
            Ok(None)
        }
    }
    
    fn parse_class_assertion(&mut self) -> OwlResult<Option<Axiom>> {
        let mut individual: Option<IRI> = None;
        let mut class_expr: Option<ClassExpression> = None;
        
        loop {
            self.buf.clear();
            match self.reader.read_event_into(&mut self.buf) {
                Ok(Event::Start(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    
                    if name.ends_with("NamedIndividual") {
                        individual = Some(self.parse_individual_reference(&e)?);
                    } else if name.ends_with("Class") {
                        class_expr = Some(self.parse_class_reference(&e)?);
                    }
                }
                Ok(Event::Empty(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    
                    if name.ends_with("NamedIndividual") {
                        individual = Some(self.parse_individual_reference(&e)?);
                    } else if name.ends_with("Class") {
                        class_expr = Some(self.parse_class_reference(&e)?);
                    }
                }
                Ok(Event::End(e)) => {
                    let name = match std::str::from_utf8(e.name().as_ref()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    if name.ends_with("ClassAssertion") {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                _ => {}
            }
        }
        
        if let (Some(ind), Some(cls)) = (individual, class_expr) {
            let iri = std::sync::Arc::new(ind);
            Ok(Some(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(iri, cls)))))
        } else {
            Ok(None)
        }
    }
    
    fn parse_class_reference(
        &self,
        event: &quick_xml::events::BytesStart,
    ) -> OwlResult<ClassExpression> {
        // Extract IRI from about or IRI attribute
        for attr in event.attributes() {
            let attr = match attr {
                Ok(a) => a,
                Err(_) => continue,
            };
            let key = match std::str::from_utf8(&attr.key.as_ref()) {
                Ok(k) => k,
                Err(_) => continue,
            };
            
            if key == "about" || key.ends_with("about") || key == "IRI" || key.ends_with("IRI") {
                let value = match attr.unescape_value() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let iri = IRI::new(value.to_string())?;
                return Ok(ClassExpression::Class(Class::new(iri)));
            }
        }
        Err(OwlError::ParseError("Class without IRI reference".to_string()))
    }
    
    fn parse_individual_reference(
        &self,
        event: &quick_xml::events::BytesStart,
    ) -> OwlResult<IRI> {
        for attr in event.attributes() {
            let attr = match attr {
                Ok(a) => a,
                Err(_) => continue,
            };
            let key = match std::str::from_utf8(&attr.key.as_ref()) {
                Ok(k) => k,
                Err(_) => continue,
            };
            
            if key == "about" || key.ends_with("about") || key == "IRI" || key.ends_with("IRI") {
                let value = match attr.unescape_value() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                return IRI::new(value.to_string());
            }
        }
        Err(OwlError::ParseError("Individual without IRI reference".to_string()))
    }
}

/// Parse an ontology file using streaming parser
pub fn parse_owl_xml_streaming<R: Read>(reader: R) -> OwlResult<Vec<Axiom>> {
    let buf_reader = BufReader::new(reader);
    let mut parser = StreamingOwlXmlParser::new(buf_reader)?;
    let mut axioms = Vec::new();
    
    while let Some(axiom) = parser.next_axiom()? {
        axioms.push(axiom);
    }
    
    Ok(axioms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    
    #[test]
    fn test_streaming_parse_simple() {
        let xml = r#"<?xml version="1.0"?>
            <Ontology xmlns="http://www.w3.org/2002/07/owl#">
                <SubClassOf>
                    <Class about="http://example.org/A"/>
                    <Class about="http://example.org/B"/>
                </SubClassOf>
            </Ontology>"#;
        
        let cursor = Cursor::new(xml.as_bytes());
        let axioms = parse_owl_xml_streaming(cursor).unwrap();
        
        assert_eq!(axioms.len(), 1);
    }
    
    #[test]
    fn test_streaming_parse_multiple() {
        let xml = r#"<?xml version="1.0"?>
            <Ontology xmlns="http://www.w3.org/2002/07/owl#">
                <SubClassOf>
                    <Class about="http://example.org/A"/>
                    <Class about="http://example.org/B"/>
                </SubClassOf>
                <SubClassOf>
                    <Class about="http://example.org/B"/>
                    <Class about="http://example.org/C"/>
                </SubClassOf>
            </Ontology>"#;
        
        let cursor = Cursor::new(xml.as_bytes());
        let axioms = parse_owl_xml_streaming(cursor).unwrap();
        
        assert_eq!(axioms.len(), 2);
    }
}
