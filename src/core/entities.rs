//! OWL2 Entities - Classes, Properties, and Individuals
//!
//! This module defines the core entities of OWL2 ontologies including classes,
//! object properties, data properties, annotations, and individuals.

use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::util::cache_manager;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::sync::Arc;

/// Well-known IRI constants
pub static XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
pub static RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// Get a shared Arc<IRI> from the global cache manager
fn get_shared_iri<S: Into<String>>(iri: S) -> OwlResult<Arc<IRI>> {
    cache_manager::get_or_create_iri(iri.into())
}

/// Get global entity cache statistics
pub fn global_entity_cache_stats() -> cache_manager::CacheStatsSnapshot {
    cache_manager::global_cache_stats()
}

/// Clear the global entity cache
pub fn clear_global_entity_cache() -> OwlResult<()> {
    cache_manager::clear_global_iri_cache()
}

/// Common trait for all OWL2 entities
pub trait Entity {
    /// Create a new entity with the given IRI (fallback constructor)
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self;

    /// Create a new entity with shared IRI (preferred for memory efficiency)
    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self>
    where
        Self: Sized;

    /// Get the IRI of this entity
    fn iri(&self) -> &Arc<IRI>;

    /// Get the annotations associated with this entity
    fn annotations(&self) -> &[Annotation];

    /// Get mutable access to annotations
    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]>;

    /// Add an annotation to this entity
    fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations_mut().push(annotation);
    }

    /// Create entity from shared IRI (internal helper)
    fn from_shared_iri(iri: Arc<IRI>) -> Self;
}

/// Shared entity creation logic to reduce code duplication
pub fn create_entity_with_fallback<I: Into<IRI> + Clone, E: Entity>(iri: I) -> E {
    // For backward compatibility, fall back to direct creation if sharing fails
    let iri_clone = iri.clone();
    let shared_iri =
        get_shared_iri(iri.into().as_str()).unwrap_or_else(|_| Arc::new(iri_clone.into()));
    E::from_shared_iri(shared_iri)
}

/// Shared entity creation with error handling
pub fn create_entity_shared<S: Into<String>, E: Entity>(iri: S) -> OwlResult<E> {
    let shared_iri = get_shared_iri(iri)?;
    Ok(E::from_shared_iri(shared_iri))
}

/// Force eviction of N entries from global entity cache
pub fn force_global_entity_cache_eviction(count: usize) -> OwlResult<usize> {
    let current_size = cache_manager::global_cache_manager().get_iri_cache_size()?;
    let target_size = current_size.saturating_sub(count);
    let evicted = current_size - target_size;
    Ok(evicted)
}

/// A named class in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Class {
    /// The IRI of the class
    iri: Arc<IRI>,
    /// Annotations associated with this class
    annotations: SmallVec<[Annotation; 4]>,
}

impl Entity for Class {
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        create_entity_with_fallback(iri)
    }

    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        create_entity_shared(iri)
    }

    fn iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]> {
        &mut self.annotations
    }

    fn from_shared_iri(iri: Arc<IRI>) -> Self {
        Class {
            iri,
            annotations: SmallVec::new(),
        }
    }
}

impl Class {
    /// Built-in owl:Thing class
    #[inline]
    pub fn owl_thing() -> Self {
        Class::new("http://www.w3.org/2002/07/owl#Thing")
    }

    /// Built-in owl:Nothing class
    #[inline]
    pub fn owl_nothing() -> Self {
        Class::new("http://www.w3.org/2002/07/owl#Nothing")
    }
    /// Create a new class with the given IRI (backward compatibility)
    pub fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        <Self as Entity>::new(iri)
    }

    /// Create a new class with shared IRI (backward compatibility)
    pub fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        <Self as Entity>::new_shared(iri)
    }

    /// Get the IRI of this class (backward compatibility)
    pub fn iri(&self) -> &Arc<IRI> {
        <Self as Entity>::iri(self)
    }

    /// Get the annotations for this class (backward compatibility)
    pub fn annotations(&self) -> &[Annotation] {
        <Self as Entity>::annotations(self)
    }

    /// Add an annotation to this class (backward compatibility)
    pub fn add_annotation(&mut self, annotation: Annotation) {
        <Self as Entity>::add_annotation(self, annotation);
    }

    /// Check if this is a built-in OWL class
    #[inline]
    pub fn is_builtin(&self) -> bool {
        self.iri.is_owl() && matches!(self.iri.local_name(), "Thing" | "Nothing" | "Class")
    }

    /// Check if this is owl:Thing (the top class)
    #[inline(always)]
    pub fn is_thing(&self) -> bool {
        self.iri.as_str() == "http://www.w3.org/2002/07/owl#Thing"
    }

    /// Check if this is owl:Nothing (the bottom class)
    #[inline(always)]
    pub fn is_nothing(&self) -> bool {
        self.iri.as_str() == "http://www.w3.org/2002/07/owl#Nothing"
    }
}

/// An object property in OWL2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectProperty {
    /// The IRI of the property
    iri: Arc<IRI>,
    /// Annotations associated with this property
    annotations: SmallVec<[Annotation; 4]>,
    /// Property characteristics
    characteristics: HashSet<ObjectPropertyCharacteristic>,
}

impl Entity for ObjectProperty {
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        create_entity_with_fallback(iri)
    }

    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        create_entity_shared(iri)
    }

    fn iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]> {
        &mut self.annotations
    }

    fn from_shared_iri(iri: Arc<IRI>) -> Self {
        ObjectProperty {
            iri,
            annotations: SmallVec::new(),
            characteristics: HashSet::new(),
        }
    }
}

impl ObjectProperty {
    /// Create a new object property with the given IRI (backward compatibility)
    pub fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        <Self as Entity>::new(iri)
    }

    /// Create a new object property with shared IRI (backward compatibility)
    pub fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        <Self as Entity>::new_shared(iri)
    }

    /// Get the IRI of this property (backward compatibility)
    pub fn iri(&self) -> &Arc<IRI> {
        <Self as Entity>::iri(self)
    }

    /// Get the annotations for this property (backward compatibility)
    pub fn annotations(&self) -> &[Annotation] {
        <Self as Entity>::annotations(self)
    }

    /// Get the characteristics of this property
    pub fn characteristics(&self) -> &HashSet<ObjectPropertyCharacteristic> {
        &self.characteristics
    }

    /// Add an annotation to this property (backward compatibility)
    pub fn add_annotation(&mut self, annotation: Annotation) {
        <Self as Entity>::add_annotation(self, annotation);
    }

    /// Add a characteristic to this property
    pub fn add_characteristic(&mut self, characteristic: ObjectPropertyCharacteristic) {
        self.characteristics.insert(characteristic);
    }

    /// Check if this property has a specific characteristic
    pub fn has_characteristic(&self, characteristic: ObjectPropertyCharacteristic) -> bool {
        self.characteristics.contains(&characteristic)
    }

    /// Check if this property is functional
    #[inline]
    pub fn is_functional(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Functional)
    }

    /// Check if this property is inverse functional
    #[inline]
    pub fn is_inverse_functional(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::InverseFunctional)
    }

    /// Check if this property is transitive
    #[inline]
    pub fn is_transitive(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Transitive)
    }

    /// Check if this property is symmetric
    pub fn is_symmetric(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Symmetric)
    }

    /// Check if this property is asymmetric
    pub fn is_asymmetric(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Asymmetric)
    }

    /// Check if this property is reflexive
    pub fn is_reflexive(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Reflexive)
    }

    /// Check if this property is irreflexive
    pub fn is_irreflexive(&self) -> bool {
        self.has_characteristic(ObjectPropertyCharacteristic::Irreflexive)
    }
}

/// Characteristics of object properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectPropertyCharacteristic {
    /// Functional property (each subject has at most one object)
    Functional,
    /// Inverse functional property (each object has at most one subject)
    InverseFunctional,
    /// Transitive property (if R(a,b) and R(b,c) then R(a,c))
    Transitive,
    /// Symmetric property (if R(a,b) then R(b,a))
    Symmetric,
    /// Asymmetric property (if R(a,b) then not R(b,a))
    Asymmetric,
    /// Reflexive property (R(a,a) for all a)
    Reflexive,
    /// Irreflexive property (not R(a,a) for all a)
    Irreflexive,
}

/// A data property in OWL2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProperty {
    /// The IRI of the property
    iri: Arc<IRI>,
    /// Annotations associated with this property
    annotations: SmallVec<[Annotation; 4]>,
    /// Property characteristics
    characteristics: HashSet<DataPropertyCharacteristic>,
}

impl PartialEq for ObjectProperty {
    fn eq(&self, other: &Self) -> bool {
        self.iri == other.iri
    }
}

impl Eq for ObjectProperty {}

impl std::hash::Hash for ObjectProperty {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.iri.hash(state);
    }
}

impl Entity for DataProperty {
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        create_entity_with_fallback(iri)
    }

    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        create_entity_shared(iri)
    }

    fn iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]> {
        &mut self.annotations
    }

    fn from_shared_iri(iri: Arc<IRI>) -> Self {
        DataProperty {
            iri,
            annotations: SmallVec::new(),
            characteristics: HashSet::new(),
        }
    }
}

impl DataProperty {
    /// Create a new data property with the given IRI (backward compatibility)
    pub fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        <Self as Entity>::new(iri)
    }

    /// Create a new data property with shared IRI (backward compatibility)
    pub fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        <Self as Entity>::new_shared(iri)
    }

    /// Get the IRI of this property (backward compatibility)
    pub fn iri(&self) -> &Arc<IRI> {
        <Self as Entity>::iri(self)
    }

    /// Get the annotations for this property (backward compatibility)
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Get the characteristics of this property
    pub fn characteristics(&self) -> &HashSet<DataPropertyCharacteristic> {
        &self.characteristics
    }

    /// Add an annotation to this property
    pub fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }

    /// Add a characteristic to this property
    pub fn add_characteristic(&mut self, characteristic: DataPropertyCharacteristic) {
        self.characteristics.insert(characteristic);
    }

    /// Check if this property has a specific characteristic
    pub fn has_characteristic(&self, characteristic: DataPropertyCharacteristic) -> bool {
        self.characteristics.contains(&characteristic)
    }

    /// Check if this property is functional
    pub fn is_functional(&self) -> bool {
        self.has_characteristic(DataPropertyCharacteristic::Functional)
    }
}

impl PartialEq for DataProperty {
    fn eq(&self, other: &Self) -> bool {
        self.iri == other.iri
    }
}

impl Eq for DataProperty {}

impl std::hash::Hash for DataProperty {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.iri.hash(state);
    }
}

/// A annotation property in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnnotationProperty {
    /// The IRI of the property
    iri: Arc<IRI>,
    /// Annotations associated with this property
    annotations: SmallVec<[Annotation; 4]>,
}

impl Entity for AnnotationProperty {
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        create_entity_with_fallback(iri)
    }

    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        create_entity_shared(iri)
    }

    fn iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]> {
        &mut self.annotations
    }

    fn add_annotation(&mut self, annotation: Annotation) {
        if self.annotations.len() < 4 {
            self.annotations.push(annotation);
        }
    }

    fn from_shared_iri(iri: Arc<IRI>) -> Self {
        Self {
            iri,
            annotations: SmallVec::new(),
        }
    }
}

/// Characteristics of data properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataPropertyCharacteristic {
    /// Functional property (each subject has at most one value)
    Functional,
}

/// A named individual in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedIndividual {
    /// The IRI of the individual
    iri: Arc<IRI>,
    /// Annotations associated with this individual
    annotations: SmallVec<[Annotation; 4]>,
}

impl Entity for NamedIndividual {
    fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        create_entity_with_fallback(iri)
    }

    fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        create_entity_shared(iri)
    }

    fn iri(&self) -> &Arc<IRI> {
        &self.iri
    }

    fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    fn annotations_mut(&mut self) -> &mut SmallVec<[Annotation; 4]> {
        &mut self.annotations
    }

    fn from_shared_iri(iri: Arc<IRI>) -> Self {
        NamedIndividual {
            iri,
            annotations: SmallVec::new(),
        }
    }
}

impl NamedIndividual {
    /// Create a new named individual with the given IRI (backward compatibility)
    pub fn new<I: Into<IRI> + Clone>(iri: I) -> Self {
        <Self as Entity>::new(iri)
    }

    /// Create a new named individual with shared IRI (backward compatibility)
    pub fn new_shared<S: Into<String>>(iri: S) -> OwlResult<Self> {
        <Self as Entity>::new_shared(iri)
    }

    /// Get the IRI of this individual (backward compatibility)
    pub fn iri(&self) -> &Arc<IRI> {
        <Self as Entity>::iri(self)
    }

    /// Get the annotations for this individual (backward compatibility)
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Add an annotation to this individual
    pub fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }
}

/// An annotation in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Annotation {
    /// The annotation property
    property: Arc<IRI>,
    /// The annotation value
    value: AnnotationValue,
}

impl Annotation {
    /// Create a new annotation
    pub fn new<P: Into<IRI>, V: Into<AnnotationValue>>(property: P, value: V) -> Self {
        Annotation {
            property: IRI::new_optimized(property.into().as_str())
                .expect("Failed to create annotation property IRI"),
            value: value.into(),
        }
    }

    /// Get the annotation property
    pub fn property(&self) -> &Arc<IRI> {
        &self.property
    }

    /// Get the annotation value
    pub fn value(&self) -> &AnnotationValue {
        &self.value
    }
}

/// Annotation values in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnnotationValue {
    /// IRI reference
    IRI(Arc<IRI>),
    /// Literal value
    Literal(Literal),
    /// Anonymous individual
    AnonymousIndividual(String),
}

impl From<IRI> for AnnotationValue {
    fn from(iri: IRI) -> Self {
        AnnotationValue::IRI(iri.into())
    }
}

impl From<Arc<IRI>> for AnnotationValue {
    fn from(iri: Arc<IRI>) -> Self {
        AnnotationValue::IRI(iri)
    }
}

impl From<Literal> for AnnotationValue {
    fn from(literal: Literal) -> Self {
        AnnotationValue::Literal(literal)
    }
}

impl From<String> for AnnotationValue {
    fn from(s: String) -> Self {
        AnnotationValue::Literal(Literal::simple(s))
    }
}

impl From<&str> for AnnotationValue {
    fn from(s: &str) -> Self {
        AnnotationValue::Literal(Literal::simple(s.to_string()))
    }
}

/// A literal value in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Literal {
    /// The lexical value
    lexical_form: String,
    /// The datatype IRI
    datatype: Arc<IRI>,
    /// Optional language tag
    language_tag: Option<String>,
}

impl Literal {
    /// Create a simple string literal
    pub fn simple<S: Into<String>>(value: S) -> Self {
        Literal {
            lexical_form: value.into(),
            datatype: IRI::new_optimized(XSD_STRING)
                .expect("XSD string IRI should always be valid"),
            language_tag: None,
        }
    }

    /// Create a typed literal
    pub fn typed<S: Into<String>, D: Into<IRI>>(value: S, datatype: D) -> Self {
        Literal {
            lexical_form: value.into(),
            datatype: IRI::new_optimized(datatype.into().as_str())
                .expect("Failed to create datatype IRI"),
            language_tag: None,
        }
    }

    /// Create a language-tagged literal
    pub fn lang_tagged<S: Into<String>, L: Into<String>>(value: S, language: L) -> Self {
        Literal {
            lexical_form: value.into(),
            datatype: IRI::new_optimized(RDF_LANG_STRING)
                .expect("RDF langString IRI should always be valid"),
            language_tag: Some(language.into()),
        }
    }

    /// Get the lexical form of the literal
    pub fn lexical_form(&self) -> &str {
        &self.lexical_form
    }

    /// Get the datatype of the literal
    pub fn datatype(&self) -> &Arc<IRI> {
        &self.datatype
    }

    /// Get the language tag of the literal
    pub fn language_tag(&self) -> Option<&str> {
        self.language_tag.as_deref()
    }

    /// Check if this is a plain literal (no datatype or language tag)
    pub fn is_plain(&self) -> bool {
        self.datatype.as_str() == "http://www.w3.org/2001/XMLSchema#string"
            && self.language_tag.is_none()
    }

    /// Check if this is a language-tagged literal
    pub fn is_lang_tagged(&self) -> bool {
        self.language_tag.is_some()
    }

    /// Check if this is a typed literal
    pub fn is_typed(&self) -> bool {
        !self.is_plain() && !self.is_lang_tagged()
    }
}

/// Anonymous individual (blank node)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnonymousIndividual {
    /// The node ID
    node_id: String,
    /// Annotations associated with this individual
    annotations: SmallVec<[Annotation; 4]>,
}

impl AnonymousIndividual {
    /// Create a new anonymous individual with the given node ID
    pub fn new<S: Into<String>>(node_id: S) -> Self {
        AnonymousIndividual {
            node_id: node_id.into(),
            annotations: SmallVec::new(),
        }
    }

    /// Get the node ID of this individual
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get the annotations for this individual
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Add an annotation to this individual
    pub fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }
}

/// Any individual (named or anonymous)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Individual {
    /// Named individual
    Named(NamedIndividual),
    /// Anonymous individual
    Anonymous(AnonymousIndividual),
}

impl From<NamedIndividual> for Individual {
    fn from(individual: NamedIndividual) -> Self {
        Individual::Named(individual)
    }
}

impl From<AnonymousIndividual> for Individual {
    fn from(individual: AnonymousIndividual) -> Self {
        Individual::Anonymous(individual)
    }
}

impl Individual {
    /// Get the IRI of this individual if it's named
    pub fn iri(&self) -> Option<&Arc<IRI>> {
        match self {
            Individual::Named(named) => Some(named.iri()),
            Individual::Anonymous(_) => None,
        }
    }

    /// Get the node ID of this individual if it's anonymous
    pub fn node_id(&self) -> Option<&str> {
        match self {
            Individual::Named(_) => None,
            Individual::Anonymous(anonymous) => Some(anonymous.node_id()),
        }
    }

    /// Get the annotations for this individual
    pub fn annotations(&self) -> &[Annotation] {
        match self {
            Individual::Named(named) => named.annotations(),
            Individual::Anonymous(anonymous) => anonymous.annotations(),
        }
    }

    /// Add an annotation to this individual
    pub fn add_annotation(&mut self, annotation: Annotation) {
        match self {
            Individual::Named(named) => named.add_annotation(annotation),
            Individual::Anonymous(anonymous) => anonymous.add_annotation(annotation),
        }
    }
}
