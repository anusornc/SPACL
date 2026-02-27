//! Parser for OWL 2 Restrictions including datatype restrictions
//!
//! This module handles parsing of complex restriction structures from RDF/XML,
//! including owl:Restriction, owl:someValuesFrom, owl:withRestrictions, and facet restrictions.

use crate::logic::axioms::class_expressions::{ClassExpression, DataRange, FacetRestriction};
use crate::logic::axioms::property_expressions::DataPropertyExpression;
use crate::core::entities::{DataProperty, Literal};
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use std::sync::Arc;
use xmltree::Element;

/// Namespace constants
const NS_OWL: &str = "http://www.w3.org/2002/07/owl#";
const NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const NS_RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const NS_XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Parser for OWL 2 Restriction structures
pub struct RestrictionParser;

impl RestrictionParser {
    /// Check if an element is an owl:Restriction
    pub fn is_restriction(element: &Element) -> bool {
        Self::has_qualified_name(element, NS_OWL, "Restriction")
    }

    /// Parse an owl:Restriction element into a ClassExpression
    pub fn parse_restriction(element: &Element) -> OwlResult<ClassExpression> {
        // Look for owl:onProperty
        let on_property =
            Self::find_child_with_name(element, NS_OWL, "onProperty").ok_or_else(|| {
                OwlError::ParseError("owl:Restriction missing owl:onProperty".to_string())
            })?;

        let property_iri = Self::get_resource_iri(on_property)?;

        // Look for owl:someValuesFrom
        if let Some(some_values_from) =
            Self::find_child_with_name(element, NS_OWL, "someValuesFrom")
        {
            return Self::parse_some_values_from(property_iri, some_values_from);
        }

        // Look for owl:allValuesFrom
        if let Some(all_values_from) = Self::find_child_with_name(element, NS_OWL, "allValuesFrom")
        {
            return Self::parse_all_values_from(property_iri, all_values_from);
        }

        Err(OwlError::ParseError(
            "owl:Restriction must have someValuesFrom or allValuesFrom".to_string(),
        ))
    }

    /// Parse owl:someValuesFrom element
    fn parse_some_values_from(
        property_iri: Arc<IRI>,
        element: &Element,
    ) -> OwlResult<ClassExpression> {
        // Check if it's a datatype restriction
        if let Some(datatype_elem) = element.children.iter().find(|child| {
            if let Some(elem) = child.as_element() {
                Self::has_qualified_name(elem, NS_RDFS, "Datatype")
            } else {
                false
            }
        }) {
            let datatype_elem = datatype_elem.as_element().ok_or_else(|| {
                OwlError::ParseError("Expected datatype element but found non-element".to_string())
            })?;
            let data_range = Self::parse_datatype_restriction(datatype_elem)?;
            let data_property = DataProperty::new((*property_iri).clone());
            let property_expr = DataPropertyExpression::DataProperty(data_property);
            return Ok(ClassExpression::DataSomeValuesFrom(
                Box::new(property_expr),
                Box::new(data_range),
            ));
        }

        // Otherwise, it's a class expression (object property restriction)
        // For now, return an error as we're focusing on datatype restrictions
        Err(OwlError::ParseError(
            "Object property restrictions not yet implemented in this parser".to_string(),
        ))
    }

    /// Parse owl:allValuesFrom element (placeholder)
    fn parse_all_values_from(
        _property_iri: Arc<IRI>,
        _element: &Element,
    ) -> OwlResult<ClassExpression> {
        Err(OwlError::ParseError(
            "allValuesFrom restrictions not yet implemented".to_string(),
        ))
    }

    /// Parse rdfs:Datatype element with owl:withRestrictions
    fn parse_datatype_restriction(element: &Element) -> OwlResult<DataRange> {
        // Get owl:onDatatype
        let on_datatype =
            Self::find_child_with_name(element, NS_OWL, "onDatatype").ok_or_else(|| {
                OwlError::ParseError("rdfs:Datatype missing owl:onDatatype".to_string())
            })?;

        let datatype_iri = Self::get_resource_iri(on_datatype)?;

        // Get owl:withRestrictions
        let with_restrictions = Self::find_child_with_name(element, NS_OWL, "withRestrictions")
            .ok_or_else(|| {
                OwlError::ParseError("rdfs:Datatype missing owl:withRestrictions".to_string())
            })?;

        // Parse the collection of facet restrictions
        let facet_restrictions = Self::parse_facet_restrictions(with_restrictions)?;

        Ok(DataRange::DatatypeRestriction(
            (*datatype_iri).clone(),
            facet_restrictions,
        ))
    }

    /// Parse owl:withRestrictions collection
    fn parse_facet_restrictions(element: &Element) -> OwlResult<Vec<FacetRestriction>> {
        let mut restrictions = Vec::new();

        // owl:withRestrictions should have rdf:parseType="Collection"
        // which means its children are the collection items
        for child in &element.children {
            if let Some(child_elem) = child.as_element() {
                // Each child should be an rdf:Description with facet properties
                if Self::has_qualified_name(child_elem, NS_RDF, "Description")
                    || child_elem.name == "Description"
                {
                    restrictions.extend(Self::parse_facet_description(child_elem)?);
                }
            }
        }

        Ok(restrictions)
    }

    /// Parse an rdf:Description element containing facet restrictions
    fn parse_facet_description(element: &Element) -> OwlResult<Vec<FacetRestriction>> {
        let mut restrictions = Vec::new();

        for child in &element.children {
            if let Some(child_elem) = child.as_element() {
                // Check for XSD facets
                if let Some(facet_restriction) = Self::try_parse_facet(child_elem)? {
                    restrictions.push(facet_restriction);
                }
            }
        }

        Ok(restrictions)
    }

    /// Try to parse an element as a facet restriction
    fn try_parse_facet(element: &Element) -> OwlResult<Option<FacetRestriction>> {
        // Check if this is an XSD facet
        let facet_name = &element.name;
        let facet_iri_str = if element.namespace.as_deref() == Some(NS_XSD) {
            format!("{}#{}", NS_XSD, facet_name)
        } else if let Some(stripped) = facet_name.strip_prefix("xsd:") {
            format!("{}#{}", NS_XSD, stripped)
        } else {
            return Ok(None);
        };

        let facet_iri = IRI::new(facet_iri_str)?;

        // Get the datatype from rdf:datatype attribute
        let datatype_iri = element
            .attributes
            .get("datatype")
            .or_else(|| {
                element
                    .attributes
                    .iter()
                    .find(|(k, _)| k.ends_with(":datatype"))
                    .map(|(_, v)| v)
            })
            .map(|dt| IRI::new(dt.clone()))
            .transpose()?;

        // Get the text content as the value
        let value_str = element.get_text().unwrap_or_default().to_string();

        // Create a literal with the appropriate datatype
        let literal = if let Some(dt_iri) = datatype_iri {
            Literal::typed(value_str, dt_iri)
        } else {
            Literal::simple(value_str)
        };

        Ok(Some(FacetRestriction::new(facet_iri, literal)))
    }

    // Helper methods

    /// Check if an element has a specific qualified name
    fn has_qualified_name(element: &Element, namespace: &str, local_name: &str) -> bool {
        element.name == local_name && element.namespace.as_deref() == Some(namespace)
            || element.name == format!("owl:{}", local_name) && namespace == NS_OWL
            || element.name == format!("rdf:{}", local_name) && namespace == NS_RDF
            || element.name == format!("rdfs:{}", local_name) && namespace == NS_RDFS
    }

    /// Find a child element with a specific qualified name
    fn find_child_with_name<'a>(
        element: &'a Element,
        namespace: &str,
        local_name: &str,
    ) -> Option<&'a Element> {
        element.children.iter().find_map(|child| {
            child.as_element().and_then(|elem| {
                if Self::has_qualified_name(elem, namespace, local_name) {
                    Some(elem)
                } else {
                    None
                }
            })
        })
    }

    /// Get the IRI from rdf:resource attribute
    fn get_resource_iri(element: &Element) -> OwlResult<Arc<IRI>> {
        element
            .attributes
            .get("resource")
            .or_else(|| {
                element
                    .attributes
                    .iter()
                    .find(|(k, _)| k.ends_with(":resource"))
                    .map(|(_, v)| v)
            })
            .ok_or_else(|| OwlError::ParseError("Missing rdf:resource attribute".to_string()))
            .and_then(|iri_str| IRI::new(iri_str.clone()).map(Arc::new))
    }
}
