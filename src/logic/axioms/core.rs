//! Core OWL2 axioms that are most commonly used
//!
//! This module contains the most frequently used axiom types
//! extracted from the main axioms module for better organization.

use crate::logic::axioms::types::*;
use crate::core::entities::{AnonymousIndividual, Literal, Annotation};
use crate::core::iri::IRI;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Property assertion axiom: (a, b) ∈ P
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyAssertionAxiom {
    subject: Arc<IRI>,
    property: Arc<IRI>,
    object: PropertyAssertionObject,
}

impl PropertyAssertionAxiom {
    /// Create a new property assertion axiom with named individual
    pub fn new(subject: Arc<IRI>, property: Arc<IRI>, object: Arc<IRI>) -> Self {
        PropertyAssertionAxiom {
            subject,
            property,
            object: PropertyAssertionObject::Named(object),
        }
    }

    /// Create a new property assertion axiom with anonymous individual
    pub fn new_with_anonymous(
        subject: Arc<IRI>,
        property: Arc<IRI>,
        object: AnonymousIndividual,
    ) -> Self {
        PropertyAssertionAxiom {
            subject,
            property,
            object: PropertyAssertionObject::Anonymous(Box::new(object)),
        }
    }

    /// Create a new property assertion axiom with property assertion object
    pub fn new_with_object(
        subject: Arc<IRI>,
        property: Arc<IRI>,
        object: PropertyAssertionObject,
    ) -> Self {
        PropertyAssertionAxiom {
            subject,
            property,
            object,
        }
    }

    /// Get the subject
    pub fn subject(&self) -> &Arc<IRI> {
        &self.subject
    }

    /// Get the property
    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    /// Get the object
    pub fn object(&self) -> &PropertyAssertionObject {
        &self.object
    }

    /// Get object as IRI if it's a named individual
    pub fn object_iri(&self) -> Option<&Arc<IRI>> {
        match &self.object {
            PropertyAssertionObject::Named(iri) => Some(iri),
            PropertyAssertionObject::Anonymous(_) => None,
        }
    }

    /// Check if this axiom involves a specific subject
    pub fn involves_subject(&self, subject_iri: &IRI) -> bool {
        self.subject.as_ref() == subject_iri
    }

    /// Check if this axiom involves a specific property
    pub fn involves_property(&self, property_iri: &IRI) -> bool {
        self.property.as_ref() == property_iri
    }
}

/// Data property assertion axiom: (a, v) ∈ P where v is a literal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPropertyAssertionAxiom {
    subject: Arc<IRI>,
    property: Arc<IRI>,
    value: Literal,
}

impl DataPropertyAssertionAxiom {
    /// Create a new data property assertion axiom
    pub fn new(subject: Arc<IRI>, property: Arc<IRI>, value: Literal) -> Self {
        DataPropertyAssertionAxiom {
            subject,
            property,
            value,
        }
    }

    /// Get the subject
    pub fn subject(&self) -> &Arc<IRI> {
        &self.subject
    }

    /// Get the property
    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    /// Get the literal value
    pub fn value(&self) -> &Literal {
        &self.value
    }

    /// Check if this axiom involves a specific subject
    pub fn involves_subject(&self, subject_iri: &IRI) -> bool {
        self.subject.as_ref() == subject_iri
    }

    /// Check if this axiom involves a specific property
    pub fn involves_property(&self, property_iri: &IRI) -> bool {
        self.property.as_ref() == property_iri
    }
}

/// Annotation assertion axiom: ⊤ ⊑ ∃r.{@a}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationAssertionAxiom {
    subject: Arc<IRI>,
    property: Arc<IRI>,
    value: Annotation,
}

impl AnnotationAssertionAxiom {
    /// Create a new annotation assertion axiom
    pub fn new(subject: Arc<IRI>, property: Arc<IRI>, value: Annotation) -> Self {
        AnnotationAssertionAxiom {
            subject,
            property,
            value,
        }
    }

    /// Get the subject
    pub fn subject(&self) -> &Arc<IRI> {
        &self.subject
    }

    /// Get the annotation property
    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    /// Get the annotation value
    pub fn value(&self) -> &Annotation {
        &self.value
    }

    /// Check if this axiom involves a specific subject
    pub fn involves_subject(&self, subject_iri: &IRI) -> bool {
        self.subject.as_ref() == subject_iri
    }
}

// Property axioms

/// SubObjectPropertyOf axiom: P ⊑ Q
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubObjectPropertyOfAxiom {
    sub_property: Arc<IRI>,
    super_property: Arc<IRI>,
}

impl SubObjectPropertyOfAxiom {
    pub fn new(sub_property: Arc<IRI>, super_property: Arc<IRI>) -> Self {
        Self { sub_property, super_property }
    }
    pub fn sub_property(&self) -> &Arc<IRI> { &self.sub_property }
    pub fn super_property(&self) -> &Arc<IRI> { &self.super_property }
}

/// EquivalentObjectProperties axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalentObjectPropertiesAxiom {
    properties: Vec<Arc<IRI>>,
}

impl EquivalentObjectPropertiesAxiom {
    pub fn new(properties: Vec<Arc<IRI>>) -> Self {
        Self { properties }
    }
    pub fn properties(&self) -> &Vec<Arc<IRI>> { &self.properties }
}

/// DisjointObjectProperties axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisjointObjectPropertiesAxiom {
    properties: Vec<Arc<IRI>>,
}

impl DisjointObjectPropertiesAxiom {
    pub fn new(properties: Vec<Arc<IRI>>) -> Self {
        Self { properties }
    }
    pub fn properties(&self) -> &Vec<Arc<IRI>> { &self.properties }
}

/// FunctionalProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalPropertyAxiom {
    property: Arc<IRI>,
}

impl FunctionalPropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// InverseFunctionalProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InverseFunctionalPropertyAxiom {
    property: Arc<IRI>,
}

impl InverseFunctionalPropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// ReflexiveProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflexivePropertyAxiom {
    property: Arc<IRI>,
}

impl ReflexivePropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// IrreflexiveProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrreflexivePropertyAxiom {
    property: Arc<IRI>,
}

impl IrreflexivePropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// SymmetricProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetricPropertyAxiom {
    property: Arc<IRI>,
}

impl SymmetricPropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// AsymmetricProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsymmetricPropertyAxiom {
    property: Arc<IRI>,
}

impl AsymmetricPropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// TransitiveProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitivePropertyAxiom {
    property: Arc<IRI>,
}

impl TransitivePropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
    pub fn property(&self) -> &Arc<IRI> { &self.property }
}

/// SubDataPropertyOf axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubDataPropertyOfAxiom {
    sub_property: Arc<IRI>,
    super_property: Arc<IRI>,
}

impl SubDataPropertyOfAxiom {
    pub fn new(sub_property: Arc<IRI>, super_property: Arc<IRI>) -> Self {
        Self { sub_property, super_property }
    }
}

/// EquivalentDataProperties axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalentDataPropertiesAxiom {
    properties: Vec<Arc<IRI>>,
}

impl EquivalentDataPropertiesAxiom {
    pub fn new(properties: Vec<Arc<IRI>>) -> Self {
        Self { properties }
    }
}

/// DisjointDataProperties axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisjointDataPropertiesAxiom {
    properties: Vec<Arc<IRI>>,
}

impl DisjointDataPropertiesAxiom {
    pub fn new(properties: Vec<Arc<IRI>>) -> Self {
        Self { properties }
    }
}

/// FunctionalDataProperty axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalDataPropertyAxiom {
    property: Arc<IRI>,
}

impl FunctionalDataPropertyAxiom {
    pub fn new(property: Arc<IRI>) -> Self {
        Self { property }
    }
}

/// SameIndividual axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SameIndividualAxiom {
    individuals: Vec<Arc<IRI>>,
}

impl SameIndividualAxiom {
    pub fn new(individuals: Vec<Arc<IRI>>) -> Self {
        Self { individuals }
    }
}

/// DifferentIndividuals axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DifferentIndividualsAxiom {
    individuals: Vec<Arc<IRI>>,
}

impl DifferentIndividualsAxiom {
    pub fn new(individuals: Vec<Arc<IRI>>) -> Self {
        Self { individuals }
    }
}

/// HasKey axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HasKeyAxiom {
    class: Arc<IRI>,
    properties: Vec<Arc<IRI>>,
}

impl HasKeyAxiom {
    pub fn new(class: Arc<IRI>, properties: Vec<Arc<IRI>>) -> Self {
        Self { class, properties }
    }
}

/// SubAnnotationPropertyOf axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubAnnotationPropertyOfAxiom {
    sub_property: Arc<IRI>,
    super_property: Arc<IRI>,
}

impl SubAnnotationPropertyOfAxiom {
    pub fn new(sub_property: Arc<IRI>, super_property: Arc<IRI>) -> Self {
        Self { sub_property, super_property }
    }
}

/// AnnotationPropertyDomain axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationPropertyDomainAxiom {
    property: Arc<IRI>,
    domain: Arc<IRI>,
}

impl AnnotationPropertyDomainAxiom {
    pub fn new(property: Arc<IRI>, domain: Arc<IRI>) -> Self {
        Self { property, domain }
    }
}

/// AnnotationPropertyRange axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationPropertyRangeAxiom {
    property: Arc<IRI>,
    range: Arc<IRI>,
}

impl AnnotationPropertyRangeAxiom {
    pub fn new(property: Arc<IRI>, range: Arc<IRI>) -> Self {
        Self { property, range }
    }
}

// Type aliases for backward compatibility
pub type SubObjectPropertyAxiom = SubObjectPropertyOfAxiom;
pub type SubDataPropertyAxiom = SubDataPropertyOfAxiom;

// Additional axiom types

/// SubPropertyChainOf axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubPropertyChainOfAxiom {
    properties: Vec<Arc<IRI>>,
    super_property: Arc<IRI>,
}

impl SubPropertyChainOfAxiom {
    pub fn new(properties: Vec<Arc<IRI>>, super_property: Arc<IRI>) -> Self {
        Self { properties, super_property }
    }
}

/// InverseObjectProperties axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InverseObjectPropertiesAxiom {
    property1: Arc<IRI>,
    property2: Arc<IRI>,
}

impl InverseObjectPropertiesAxiom {
    pub fn new(property1: Arc<IRI>, property2: Arc<IRI>) -> Self {
        Self { property1, property2 }
    }
}

/// ObjectPropertyDomain axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectPropertyDomainAxiom {
    property: Arc<IRI>,
    domain: Arc<IRI>,
}

impl ObjectPropertyDomainAxiom {
    pub fn new(property: Arc<IRI>, domain: Arc<IRI>) -> Self {
        Self { property, domain }
    }

    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    pub fn domain(&self) -> &Arc<IRI> {
        &self.domain
    }
}

/// ObjectPropertyRange axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectPropertyRangeAxiom {
    property: Arc<IRI>,
    range: Arc<IRI>,
}

impl ObjectPropertyRangeAxiom {
    pub fn new(property: Arc<IRI>, range: Arc<IRI>) -> Self {
        Self { property, range }
    }

    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    pub fn range(&self) -> &Arc<IRI> {
        &self.range
    }
}

/// DataPropertyDomain axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPropertyDomainAxiom {
    property: Arc<IRI>,
    domain: Arc<IRI>,
}

impl DataPropertyDomainAxiom {
    pub fn new(property: Arc<IRI>, domain: Arc<IRI>) -> Self {
        Self { property, domain }
    }
}

/// DataPropertyRange axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPropertyRangeAxiom {
    property: Arc<IRI>,
    range: Arc<IRI>,
}

impl DataPropertyRangeAxiom {
    pub fn new(property: Arc<IRI>, range: Arc<IRI>) -> Self {
        Self { property, range }
    }
}

// Qualified cardinality axioms (placeholders)
pub type ObjectMinQualifiedCardinalityAxiom = ();  // Placeholder
pub type ObjectMaxQualifiedCardinalityAxiom = ();  // Placeholder
pub type ObjectExactQualifiedCardinalityAxiom = ();  // Placeholder
pub type DataMinQualifiedCardinalityAxiom = ();  // Placeholder
pub type DataMaxQualifiedCardinalityAxiom = ();  // Placeholder
pub type DataExactQualifiedCardinalityAxiom = ();  // Placeholder

/// NegativeObjectPropertyAssertion axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegativeObjectPropertyAssertionAxiom {
    subject: Arc<IRI>,
    property: Arc<IRI>,
    object: PropertyAssertionObject,
}

impl NegativeObjectPropertyAssertionAxiom {
    pub fn new(subject: Arc<IRI>, property: Arc<IRI>, object: Arc<IRI>) -> Self {
        Self { subject, property, object: PropertyAssertionObject::Named(object) }
    }
}

/// NegativeDataPropertyAssertion axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegativeDataPropertyAssertionAxiom {
    subject: Arc<IRI>,
    property: Arc<IRI>,
    value: Literal,
}

impl NegativeDataPropertyAssertionAxiom {
    pub fn new(subject: Arc<IRI>, property: Arc<IRI>, value: Literal) -> Self {
        Self { subject, property, value }
    }
}

/// Collection item for RDF collections
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionItem {
    Named(Arc<IRI>),
    Anonymous(Box<AnonymousIndividual>),
    Literal(crate::core::entities::Literal),
}

/// Collection axiom for RDF collections
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionAxiom {
    items: Vec<CollectionItem>,
}

impl CollectionAxiom {
    pub fn new(items: Vec<CollectionItem>) -> Self {
        Self { items }
    }
}

/// Import axiom
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportAxiom {
    imported_ontology: Arc<IRI>,
}

impl ImportAxiom {
    pub fn new(imported_ontology: Arc<IRI>) -> Self {
        Self { imported_ontology }
    }
    pub fn imported_ontology(&self) -> &Arc<IRI> {
        &self.imported_ontology
    }
}
