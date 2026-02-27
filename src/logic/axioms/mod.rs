//! OWL2 axiom definitions
//!
//! This module defines the various types of axioms that can appear in an OWL ontology:
//! - Class axioms (subclass, equivalence, disjointness)
//! - Property axioms (domain, range, characteristics)
//! - Assertion axioms (class assertions, property assertions)
//! - Declaration axioms

pub mod class_axioms;
pub mod class_expressions;
pub mod core;
pub mod property_expressions;
pub mod types;

// Re-exports
pub use class_axioms::*;
pub use class_expressions::ClassExpression;
pub use core::*;
pub use core::{CollectionItem, CollectionAxiom, ImportAxiom};
pub use property_expressions::*;
pub use types::*;
use serde::{Deserialize, Serialize};


/// The main Axiom enum that wraps all axiom types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axiom {
    SubClassOf(Box<SubClassOfAxiom>),
    EquivalentClasses(Box<EquivalentClassesAxiom>),
    DisjointClasses(Box<DisjointClassesAxiom>),
    ClassAssertion(Box<ClassAssertionAxiom>),
    PropertyAssertion(Box<core::PropertyAssertionAxiom>),
    DataPropertyAssertion(Box<DataPropertyAssertionAxiom>),
    NegativeObjectPropertyAssertion(Box<core::NegativeObjectPropertyAssertionAxiom>),
    NegativeDataPropertyAssertion(Box<core::NegativeDataPropertyAssertionAxiom>),
    SubObjectProperty(Box<SubObjectPropertyOfAxiom>),
    SubDataProperty(Box<core::SubDataPropertyOfAxiom>),
    EquivalentObjectProperties(Box<EquivalentObjectPropertiesAxiom>),
    EquivalentDataProperties(Box<core::EquivalentDataPropertiesAxiom>),
    DisjointObjectProperties(Box<DisjointObjectPropertiesAxiom>),
    DisjointDataProperties(Box<core::DisjointDataPropertiesAxiom>),
    InverseObjectProperties(Box<core::InverseObjectPropertiesAxiom>),
    FunctionalProperty(Box<FunctionalPropertyAxiom>),
    FunctionalDataProperty(Box<core::FunctionalDataPropertyAxiom>),
    InverseFunctionalProperty(Box<InverseFunctionalPropertyAxiom>),
    ReflexiveProperty(Box<ReflexivePropertyAxiom>),
    IrreflexiveProperty(Box<IrreflexivePropertyAxiom>),
    SymmetricProperty(Box<SymmetricPropertyAxiom>),
    AsymmetricProperty(Box<AsymmetricPropertyAxiom>),
    TransitiveProperty(Box<TransitivePropertyAxiom>),
    ObjectPropertyDomain(Box<core::ObjectPropertyDomainAxiom>),
    ObjectPropertyRange(Box<core::ObjectPropertyRangeAxiom>),
    DataPropertyDomain(Box<core::DataPropertyDomainAxiom>),
    DataPropertyRange(Box<core::DataPropertyRangeAxiom>),
    SubPropertyChainOf(Box<core::SubPropertyChainOfAxiom>),
    HasKey(Box<core::HasKeyAxiom>),
    SameIndividual(Box<core::SameIndividualAxiom>),
    DifferentIndividuals(Box<core::DifferentIndividualsAxiom>),
    AnnotationAssertion(Box<core::AnnotationAssertionAxiom>),
    SubAnnotationPropertyOf(Box<core::SubAnnotationPropertyOfAxiom>),
    AnnotationPropertyDomain(Box<core::AnnotationPropertyDomainAxiom>),
    AnnotationPropertyRange(Box<core::AnnotationPropertyRangeAxiom>),
    // Qualified cardinality (placeholders)
    ObjectMinQualifiedCardinality(Box<()>),
    ObjectMaxQualifiedCardinality(Box<()>),
    ObjectExactQualifiedCardinality(Box<()>),
    DataMinQualifiedCardinality(Box<()>),
    DataMaxQualifiedCardinality(Box<()>),
    DataExactQualifiedCardinality(Box<()>),
    // Other
    Import(Box<core::ImportAxiom>),
    Collection(Box<core::CollectionAxiom>),
    Container(Box<()>),
    Reification(Box<()>),
}

impl Axiom {
    /// Get the axiom type for this axiom
    pub fn axiom_type(&self) -> AxiomType {
        match self {
            Axiom::SubClassOf(_) => AxiomType::SubClassOf,
            Axiom::EquivalentClasses(_) => AxiomType::EquivalentClasses,
            Axiom::DisjointClasses(_) => AxiomType::DisjointClasses,
            Axiom::ClassAssertion(_) => AxiomType::ClassAssertion,
            Axiom::PropertyAssertion(_) => AxiomType::PropertyAssertion,
            Axiom::DataPropertyAssertion(_) => AxiomType::DataPropertyAssertion,
            Axiom::NegativeObjectPropertyAssertion(_) => AxiomType::NegativeObjectPropertyAssertion,
            Axiom::NegativeDataPropertyAssertion(_) => AxiomType::NegativeDataPropertyAssertion,
            Axiom::SubObjectProperty(_) => AxiomType::SubObjectProperty,
            Axiom::SubDataProperty(_) => AxiomType::SubDataProperty,
            Axiom::EquivalentObjectProperties(_) => AxiomType::EquivalentObjectProperties,
            Axiom::EquivalentDataProperties(_) => AxiomType::EquivalentDataProperties,
            Axiom::DisjointObjectProperties(_) => AxiomType::DisjointObjectProperties,
            Axiom::DisjointDataProperties(_) => AxiomType::DisjointDataProperties,
            Axiom::InverseObjectProperties(_) => AxiomType::InverseObjectProperties,
            Axiom::FunctionalProperty(_) => AxiomType::FunctionalProperty,
            Axiom::FunctionalDataProperty(_) => AxiomType::FunctionalDataProperty,
            Axiom::InverseFunctionalProperty(_) => AxiomType::InverseFunctionalProperty,
            Axiom::ReflexiveProperty(_) => AxiomType::ReflexiveProperty,
            Axiom::IrreflexiveProperty(_) => AxiomType::IrreflexiveProperty,
            Axiom::SymmetricProperty(_) => AxiomType::SymmetricProperty,
            Axiom::AsymmetricProperty(_) => AxiomType::AsymmetricProperty,
            Axiom::TransitiveProperty(_) => AxiomType::TransitiveProperty,
            Axiom::ObjectPropertyDomain(_) => AxiomType::ObjectPropertyDomain,
            Axiom::ObjectPropertyRange(_) => AxiomType::ObjectPropertyRange,
            Axiom::DataPropertyDomain(_) => AxiomType::DataPropertyDomain,
            Axiom::DataPropertyRange(_) => AxiomType::DataPropertyRange,
            Axiom::SubPropertyChainOf(_) => AxiomType::SubPropertyChainOf,
            Axiom::HasKey(_) => AxiomType::HasKey,
            Axiom::SameIndividual(_) => AxiomType::SameIndividual,
            Axiom::DifferentIndividuals(_) => AxiomType::DifferentIndividuals,
            Axiom::AnnotationAssertion(_) => AxiomType::AnnotationAssertion,
            Axiom::SubAnnotationPropertyOf(_) => AxiomType::SubAnnotationPropertyOf,
            Axiom::AnnotationPropertyDomain(_) => AxiomType::AnnotationPropertyDomain,
            Axiom::AnnotationPropertyRange(_) => AxiomType::AnnotationPropertyRange,
            Axiom::ObjectMinQualifiedCardinality(_) => AxiomType::ObjectMinQualifiedCardinality,
            Axiom::ObjectMaxQualifiedCardinality(_) => AxiomType::ObjectMaxQualifiedCardinality,
            Axiom::ObjectExactQualifiedCardinality(_) => AxiomType::ObjectExactQualifiedCardinality,
            Axiom::DataMinQualifiedCardinality(_) => AxiomType::DataMinQualifiedCardinality,
            Axiom::DataMaxQualifiedCardinality(_) => AxiomType::DataMaxQualifiedCardinality,
            Axiom::DataExactQualifiedCardinality(_) => AxiomType::DataExactQualifiedCardinality,
            Axiom::Import(_) => AxiomType::Import,
            Axiom::Collection(_) => AxiomType::Collection,
            Axiom::Container(_) => AxiomType::Container,
            Axiom::Reification(_) => AxiomType::Reification,
        }
    }
}

impl From<SubClassOfAxiom> for Axiom {
    fn from(axiom: SubClassOfAxiom) -> Self {
        Axiom::SubClassOf(Box::new(axiom))
    }
}

impl From<ClassAssertionAxiom> for Axiom {
    fn from(axiom: ClassAssertionAxiom) -> Self {
        Axiom::ClassAssertion(Box::new(axiom))
    }
}
