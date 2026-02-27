//! Common utilities and constants for RDF/XML parsing

use crate::core::iri::IRI;
use std::collections::HashMap;

/// Static string constants to avoid allocations
pub static PREFIX_RDF: &str = "rdf";
pub static NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub static PREFIX_RDFS: &str = "rdfs";
pub static NS_RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub static PREFIX_OWL: &str = "owl";
pub static NS_OWL: &str = "http://www.w3.org/2002/07/owl#";
pub static PREFIX_XSD: &str = "xsd";
pub static NS_XSD: &str = "http://www.w3.org/2001/XMLSchema#";
#[allow(dead_code)]
pub static NS_OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
#[allow(dead_code)]
pub static NS_XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// Static strings for XML/RDF parsing
pub static XMLNS: &str = "xmlns";
pub static XMLNS_XML: &str = "xml:base";
#[allow(dead_code)]
pub static XML_LANG: &str = "xml:lang";
pub static RDF_ABOUT: &str = "rdf:about";
pub static RDF_RESOURCE: &str = "rdf:resource";
#[allow(dead_code)]
pub static RDF_ID: &str = "rdf:ID";
#[allow(dead_code)]
pub static RDF_NODE_ID: &str = "rdf:nodeID";
pub static RDF_TYPE: &str = "rdf:type";
pub static RDF_DESCRIPTION: &str = "rdf:Description";
pub static RDF_RDF: &str = "rdf:RDF";
pub static RDFS_SUBCLASSOF: &str = "rdfs:subClassOf";
pub static RDFS_DOMAIN: &str = "rdfs:domain";
pub static RDFS_RANGE: &str = "rdfs:range";

/// Static strings for OWL elements
pub static OWL_ONTOLOGY: &str = "owl:Ontology";
pub static OWL_CLASS: &str = "owl:Class";
pub static OWL_OBJECT_PROPERTY: &str = "owl:ObjectProperty";
pub static OWL_DATATYPE_PROPERTY: &str = "owl:DatatypeProperty";
pub static OWL_NAMED_INDIVIDUAL: &str = "owl:NamedIndividual";
pub static OWL_RESTRICTION: &str = "owl:Restriction";
pub static OWL_EQUIVALENT_CLASS: &str = "owl:equivalentClass";
pub static OWL_DISJOINT_WITH: &str = "owl:disjointWith";
#[allow(dead_code)]
pub static OWL_INVERSE_OF: &str = "owl:inverseOf";
#[allow(dead_code)]
pub static OWL_PROPERTY_DISJOINT_WITH: &str = "owl:propertyDisjointWith";
pub static OWL_IMPORTS: &str = "owl:imports";

/// Static strings for error messages
pub static ERR_EMPTY_ONTOLOGY: &str = "Ontology contains no entities or imports";
pub static ERR_FILE_TOO_LARGE: &str = "File size exceeds maximum allowed size";
pub static ERR_RIO_XML_PARSE: &str = "rio-xml parse error";
pub static ERR_UNKNOWN_PROPERTY_CHAR: &str = "Unknown property characteristic";

/// Resource information for RDF/XML parsing
#[derive(Debug, Clone, Default)]
pub struct ResourceInfo {
    pub iri: Option<IRI>,
    pub node_id: Option<String>,
    pub resource_type: Option<String>,
    pub properties: Vec<(String, String)>,
}

/// XML document representation
#[derive(Debug, Default)]
pub struct XmlDocument {
    pub root: Option<XmlElement>,
    pub xml_version: Option<String>,
    pub encoding: Option<String>,
    pub standalone: Option<bool>,
}

/// XML element representation
#[derive(Debug, Default)]
pub struct XmlElement {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<XmlElement>,
    pub content: String,
}

/// Initialize standard OWL2 namespaces
pub fn initialize_namespaces(config_prefixes: &HashMap<String, String>) -> HashMap<String, String> {
    let mut namespaces = HashMap::new();
    namespaces.insert(PREFIX_RDF.to_string(), NS_RDF.to_string());
    namespaces.insert(PREFIX_RDFS.to_string(), NS_RDFS.to_string());
    namespaces.insert(PREFIX_OWL.to_string(), NS_OWL.to_string());
    namespaces.insert(PREFIX_XSD.to_string(), NS_XSD.to_string());

    for (prefix, namespace) in config_prefixes {
        namespaces.insert(prefix.clone(), namespace.clone());
    }

    namespaces
}

/// Enhanced IRI validation according to RFC 3987
pub fn validate_iri(iri_str: &str) -> Result<(), String> {
    // Basic IRI validation
    if iri_str.is_empty() {
        return Err("IRI cannot be empty".to_string());
    }

    // Check for invalid characters
    if iri_str.contains(' ') || iri_str.contains('\t') || iri_str.contains('\n') {
        return Err("IRI cannot contain whitespace".to_string());
    }

    // Check for valid IRI scheme
    if let Some(scheme_end) = iri_str.find(':') {
        let scheme = &iri_str[..scheme_end];
        if scheme.is_empty()
            || !scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return Err("IRI must start with a valid scheme".to_string());
        }

        for c in scheme.chars() {
            if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
                return Err("IRI scheme contains invalid characters".to_string());
            }
        }
    } else {
        return Err("IRI must contain a scheme".to_string());
    }

    // Additional validation could be added here for stricter RFC 3987 compliance
    Ok(())
}

/// Parse XML qualified name into prefix and local name
pub fn parse_qname(qname: &str) -> Option<(String, String)> {
    qname
        .split_once(':')
        .map(|(prefix, local)| (prefix.to_string(), local.to_string()))
}

/// Expand XML qualified name using namespace map
pub fn expand_qname(qname: &str, namespaces: &HashMap<String, String>) -> Option<String> {
    if let Some((prefix, local)) = parse_qname(qname) {
        namespaces.get(&prefix).map(|ns| format!("{}{}", ns, local))
    } else {
        // No prefix, return as-is
        Some(qname.to_string())
    }
}
