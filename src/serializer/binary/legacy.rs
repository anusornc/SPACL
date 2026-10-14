use std::io::{Error, ErrorKind, Result as IoResult};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::core::entities::{
    Annotation, AnnotationProperty, AnonymousIndividual, Class, DataProperty, Individual, Literal,
    NamedIndividual, ObjectProperty,
};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::class_axioms::{
    ClassAssertionAxiom, DisjointClassesAxiom, EquivalentClassesAxiom, SubClassOfAxiom,
};
use crate::logic::axioms::class_expressions::{ClassExpression, DataRange};
use crate::logic::axioms::core;
use crate::logic::axioms::property_expressions::{
    DataPropertyExpression, ObjectPropertyExpression,
};
use crate::logic::axioms::{Axiom, DisjointObjectPropertiesAxiom, EquivalentObjectPropertiesAxiom};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LegacyOntologyPayload {
    iri: Option<IRI>,
    version_iri: Option<IRI>,
    imports: Vec<IRI>,
    classes: Vec<Class>,
    object_properties: Vec<ObjectProperty>,
    data_properties: Vec<DataProperty>,
    named_individuals: Vec<NamedIndividual>,
    anonymous_individuals: Vec<AnonymousIndividual>,
    annotation_properties: Vec<AnnotationProperty>,
    axioms: Vec<LegacyAxiom>,
    annotations: Vec<Annotation>,
}

impl LegacyOntologyPayload {
    pub(super) fn class_count(&self) -> usize {
        self.classes.len()
    }

    pub(super) fn axiom_count(&self) -> usize {
        self.axioms.len()
    }

    pub(super) fn try_from_ontology(ontology: &Ontology) -> IoResult<Self> {
        let mut axioms = Vec::with_capacity(ontology.axioms().len());
        for axiom in ontology.axioms() {
            axioms.push(LegacyAxiom::try_from_current(axiom.as_ref())?);
        }

        Ok(Self {
            iri: ontology.iri().cloned(),
            version_iri: ontology.version_iri().cloned(),
            imports: ontology
                .imports()
                .iter()
                .map(|iri| (**iri).clone())
                .collect(),
            classes: ontology
                .classes()
                .iter()
                .map(|class| (**class).clone())
                .collect(),
            object_properties: ontology
                .object_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            data_properties: ontology
                .data_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            named_individuals: ontology
                .named_individuals()
                .iter()
                .map(|ind| (**ind).clone())
                .collect(),
            anonymous_individuals: ontology
                .anonymous_individuals()
                .iter()
                .map(|ind| (**ind).clone())
                .collect(),
            annotation_properties: ontology
                .annotation_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            axioms,
            annotations: ontology.annotations().to_vec(),
        })
    }

    pub(super) fn into_current(self) -> super::OntologyPayload {
        super::OntologyPayload {
            iri: self.iri,
            version_iri: self.version_iri,
            imports: self.imports,
            classes: self.classes,
            object_properties: self.object_properties,
            data_properties: self.data_properties,
            named_individuals: self.named_individuals,
            anonymous_individuals: self.anonymous_individuals,
            annotation_properties: self.annotation_properties,
            axioms: self
                .axioms
                .into_iter()
                .map(LegacyAxiom::into_current)
                .collect(),
            annotations: self.annotations,
        }
    }
}

pub(super) fn validate_legacy_payload_representable(ontology: &Ontology) -> IoResult<()> {
    for axiom in ontology.axioms() {
        LegacyAxiom::try_from_current(axiom.as_ref())?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum LegacyAxiom {
    SubClassOf(Box<LegacySubClassOfAxiom>),
    EquivalentClasses(Box<EquivalentClassesAxiom>),
    DisjointClasses(Box<DisjointClassesAxiom>),
    ClassAssertion(Box<LegacyClassAssertionAxiom>),
    PropertyAssertion(Box<core::PropertyAssertionAxiom>),
    DataPropertyAssertion(Box<core::DataPropertyAssertionAxiom>),
    NegativeObjectPropertyAssertion(Box<core::NegativeObjectPropertyAssertionAxiom>),
    NegativeDataPropertyAssertion(Box<core::NegativeDataPropertyAssertionAxiom>),
    SubObjectProperty(Box<core::SubObjectPropertyOfAxiom>),
    SubDataProperty(Box<core::SubDataPropertyOfAxiom>),
    EquivalentObjectProperties(Box<EquivalentObjectPropertiesAxiom>),
    EquivalentDataProperties(Box<core::EquivalentDataPropertiesAxiom>),
    DisjointObjectProperties(Box<DisjointObjectPropertiesAxiom>),
    DisjointDataProperties(Box<core::DisjointDataPropertiesAxiom>),
    InverseObjectProperties(Box<core::InverseObjectPropertiesAxiom>),
    FunctionalProperty(Box<core::FunctionalPropertyAxiom>),
    FunctionalDataProperty(Box<core::FunctionalDataPropertyAxiom>),
    InverseFunctionalProperty(Box<core::InverseFunctionalPropertyAxiom>),
    ReflexiveProperty(Box<core::ReflexivePropertyAxiom>),
    IrreflexiveProperty(Box<core::IrreflexivePropertyAxiom>),
    SymmetricProperty(Box<core::SymmetricPropertyAxiom>),
    AsymmetricProperty(Box<core::AsymmetricPropertyAxiom>),
    TransitiveProperty(Box<core::TransitivePropertyAxiom>),
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
    ObjectMinQualifiedCardinality(Box<()>),
    ObjectMaxQualifiedCardinality(Box<()>),
    ObjectExactQualifiedCardinality(Box<()>),
    DataMinQualifiedCardinality(Box<()>),
    DataMaxQualifiedCardinality(Box<()>),
    DataExactQualifiedCardinality(Box<()>),
    Import(Box<core::ImportAxiom>),
    Collection(Box<core::CollectionAxiom>),
    Container(Box<()>),
    Reification(Box<()>),
}

impl LegacyAxiom {
    fn try_from_current(axiom: &Axiom) -> IoResult<Self> {
        Ok(match axiom {
            Axiom::SubClassOf(axiom) => Self::SubClassOf(Box::new(LegacySubClassOfAxiom {
                sub_class: old_class_expression(axiom.sub_class())?,
                super_class: old_class_expression(axiom.super_class())?,
            })),
            Axiom::EquivalentClasses(axiom) => Self::EquivalentClasses(axiom.clone()),
            Axiom::DisjointClasses(axiom) => Self::DisjointClasses(axiom.clone()),
            Axiom::ClassAssertion(axiom) => {
                Self::ClassAssertion(Box::new(LegacyClassAssertionAxiom {
                    individual: axiom.individual().clone(),
                    class_expr: old_class_expression(axiom.class_expr())?,
                }))
            }
            Axiom::PropertyAssertion(axiom) => Self::PropertyAssertion(axiom.clone()),
            Axiom::DataPropertyAssertion(axiom) => Self::DataPropertyAssertion(axiom.clone()),
            Axiom::NegativeObjectPropertyAssertion(axiom) => {
                Self::NegativeObjectPropertyAssertion(axiom.clone())
            }
            Axiom::NegativeDataPropertyAssertion(axiom) => {
                Self::NegativeDataPropertyAssertion(axiom.clone())
            }
            Axiom::SubObjectProperty(axiom) => Self::SubObjectProperty(axiom.clone()),
            Axiom::SubDataProperty(axiom) => Self::SubDataProperty(axiom.clone()),
            Axiom::EquivalentObjectProperties(axiom) => {
                Self::EquivalentObjectProperties(axiom.clone())
            }
            Axiom::EquivalentDataProperties(axiom) => Self::EquivalentDataProperties(axiom.clone()),
            Axiom::DisjointObjectProperties(axiom) => Self::DisjointObjectProperties(axiom.clone()),
            Axiom::DisjointDataProperties(axiom) => Self::DisjointDataProperties(axiom.clone()),
            Axiom::InverseObjectProperties(axiom) => Self::InverseObjectProperties(axiom.clone()),
            Axiom::FunctionalProperty(axiom) => Self::FunctionalProperty(axiom.clone()),
            Axiom::FunctionalDataProperty(axiom) => Self::FunctionalDataProperty(axiom.clone()),
            Axiom::InverseFunctionalProperty(axiom) => {
                Self::InverseFunctionalProperty(axiom.clone())
            }
            Axiom::ReflexiveProperty(axiom) => Self::ReflexiveProperty(axiom.clone()),
            Axiom::IrreflexiveProperty(axiom) => Self::IrreflexiveProperty(axiom.clone()),
            Axiom::SymmetricProperty(axiom) => Self::SymmetricProperty(axiom.clone()),
            Axiom::AsymmetricProperty(axiom) => Self::AsymmetricProperty(axiom.clone()),
            Axiom::TransitiveProperty(axiom) => Self::TransitiveProperty(axiom.clone()),
            Axiom::ObjectPropertyDomain(axiom) => Self::ObjectPropertyDomain(axiom.clone()),
            Axiom::ObjectPropertyRange(axiom) => Self::ObjectPropertyRange(axiom.clone()),
            Axiom::DataPropertyDomain(axiom) => Self::DataPropertyDomain(axiom.clone()),
            Axiom::DataPropertyRange(axiom) => Self::DataPropertyRange(axiom.clone()),
            Axiom::SubPropertyChainOf(axiom) => Self::SubPropertyChainOf(axiom.clone()),
            Axiom::HasKey(axiom) => Self::HasKey(axiom.clone()),
            Axiom::SameIndividual(axiom) => Self::SameIndividual(axiom.clone()),
            Axiom::DifferentIndividuals(axiom) => Self::DifferentIndividuals(axiom.clone()),
            Axiom::AnnotationAssertion(axiom) => Self::AnnotationAssertion(axiom.clone()),
            Axiom::SubAnnotationPropertyOf(axiom) => Self::SubAnnotationPropertyOf(axiom.clone()),
            Axiom::AnnotationPropertyDomain(axiom) => Self::AnnotationPropertyDomain(axiom.clone()),
            Axiom::AnnotationPropertyRange(axiom) => Self::AnnotationPropertyRange(axiom.clone()),
            Axiom::ObjectMinQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::ObjectMaxQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::ObjectExactQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::DataPropertyEnumeratedRange(_) => return Err(unrepresentable_legacy()),
            Axiom::DataMinQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::DataMaxQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::DataExactQualifiedCardinality(_) => return Err(unrepresentable_legacy()),
            Axiom::Import(axiom) => Self::Import(axiom.clone()),
            Axiom::Collection(axiom) => Self::Collection(axiom.clone()),
            Axiom::Container(_) => return Err(unrepresentable_legacy()),
            Axiom::Reification(_) => return Err(unrepresentable_legacy()),
        })
    }

    fn into_current(self) -> Axiom {
        match self {
            Self::SubClassOf(axiom) => Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
                axiom.sub_class.into_current(),
                axiom.super_class.into_current(),
            ))),
            Self::EquivalentClasses(axiom) => Axiom::EquivalentClasses(axiom),
            Self::DisjointClasses(axiom) => Axiom::DisjointClasses(axiom),
            Self::ClassAssertion(axiom) => Axiom::ClassAssertion(Box::new(
                ClassAssertionAxiom::new(axiom.individual, axiom.class_expr.into_current()),
            )),
            Self::PropertyAssertion(axiom) => Axiom::PropertyAssertion(axiom),
            Self::DataPropertyAssertion(axiom) => Axiom::DataPropertyAssertion(axiom),
            Self::NegativeObjectPropertyAssertion(axiom) => {
                Axiom::NegativeObjectPropertyAssertion(axiom)
            }
            Self::NegativeDataPropertyAssertion(axiom) => {
                Axiom::NegativeDataPropertyAssertion(axiom)
            }
            Self::SubObjectProperty(axiom) => Axiom::SubObjectProperty(axiom),
            Self::SubDataProperty(axiom) => Axiom::SubDataProperty(axiom),
            Self::EquivalentObjectProperties(axiom) => Axiom::EquivalentObjectProperties(axiom),
            Self::EquivalentDataProperties(axiom) => Axiom::EquivalentDataProperties(axiom),
            Self::DisjointObjectProperties(axiom) => Axiom::DisjointObjectProperties(axiom),
            Self::DisjointDataProperties(axiom) => Axiom::DisjointDataProperties(axiom),
            Self::InverseObjectProperties(axiom) => Axiom::InverseObjectProperties(axiom),
            Self::FunctionalProperty(axiom) => Axiom::FunctionalProperty(axiom),
            Self::FunctionalDataProperty(axiom) => Axiom::FunctionalDataProperty(axiom),
            Self::InverseFunctionalProperty(axiom) => Axiom::InverseFunctionalProperty(axiom),
            Self::ReflexiveProperty(axiom) => Axiom::ReflexiveProperty(axiom),
            Self::IrreflexiveProperty(axiom) => Axiom::IrreflexiveProperty(axiom),
            Self::SymmetricProperty(axiom) => Axiom::SymmetricProperty(axiom),
            Self::AsymmetricProperty(axiom) => Axiom::AsymmetricProperty(axiom),
            Self::TransitiveProperty(axiom) => Axiom::TransitiveProperty(axiom),
            Self::ObjectPropertyDomain(axiom) => Axiom::ObjectPropertyDomain(axiom),
            Self::ObjectPropertyRange(axiom) => Axiom::ObjectPropertyRange(axiom),
            Self::DataPropertyDomain(axiom) => Axiom::DataPropertyDomain(axiom),
            Self::DataPropertyRange(axiom) => Axiom::DataPropertyRange(axiom),
            Self::SubPropertyChainOf(axiom) => Axiom::SubPropertyChainOf(axiom),
            Self::HasKey(axiom) => Axiom::HasKey(axiom),
            Self::SameIndividual(axiom) => Axiom::SameIndividual(axiom),
            Self::DifferentIndividuals(axiom) => Axiom::DifferentIndividuals(axiom),
            Self::AnnotationAssertion(axiom) => Axiom::AnnotationAssertion(axiom),
            Self::SubAnnotationPropertyOf(axiom) => Axiom::SubAnnotationPropertyOf(axiom),
            Self::AnnotationPropertyDomain(axiom) => Axiom::AnnotationPropertyDomain(axiom),
            Self::AnnotationPropertyRange(axiom) => Axiom::AnnotationPropertyRange(axiom),
            Self::ObjectMinQualifiedCardinality(axiom) => {
                Axiom::ObjectMinQualifiedCardinality(axiom)
            }
            Self::ObjectMaxQualifiedCardinality(axiom) => {
                Axiom::ObjectMaxQualifiedCardinality(axiom)
            }
            Self::ObjectExactQualifiedCardinality(axiom) => {
                Axiom::ObjectExactQualifiedCardinality(axiom)
            }
            Self::DataMinQualifiedCardinality(axiom) => Axiom::DataMinQualifiedCardinality(axiom),
            Self::DataMaxQualifiedCardinality(axiom) => Axiom::DataMaxQualifiedCardinality(axiom),
            Self::DataExactQualifiedCardinality(axiom) => {
                Axiom::DataExactQualifiedCardinality(axiom)
            }
            Self::Import(axiom) => Axiom::Import(axiom),
            Self::Collection(axiom) => Axiom::Collection(axiom),
            Self::Container(axiom) => Axiom::Container(axiom),
            Self::Reification(axiom) => Axiom::Reification(axiom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LegacySubClassOfAxiom {
    sub_class: LegacyClassExpression,
    super_class: LegacyClassExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LegacyClassAssertionAxiom {
    individual: Arc<IRI>,
    class_expr: LegacyClassExpression,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum LegacyClassExpression {
    Class(Class),
    ObjectIntersectionOf(SmallVec<[Box<LegacyClassExpression>; 4]>),
    ObjectUnionOf(SmallVec<[Box<LegacyClassExpression>; 4]>),
    ObjectComplementOf(Box<LegacyClassExpression>),
    ObjectOneOf(Box<SmallVec<[Individual; 8]>>),
    ObjectSomeValuesFrom(Box<ObjectPropertyExpression>, Box<LegacyClassExpression>),
    ObjectAllValuesFrom(Box<ObjectPropertyExpression>, Box<LegacyClassExpression>),
    ObjectHasValue(Box<ObjectPropertyExpression>, Individual),
    ObjectHasSelf(Box<ObjectPropertyExpression>),
    ObjectMinCardinality(u32, Box<ObjectPropertyExpression>),
    ObjectMaxCardinality(u32, Box<ObjectPropertyExpression>),
    ObjectExactCardinality(u32, Box<ObjectPropertyExpression>),
    DataSomeValuesFrom(Box<DataPropertyExpression>, Box<DataRange>),
    DataAllValuesFrom(Box<DataPropertyExpression>, Box<DataRange>),
    DataHasValue(Box<DataPropertyExpression>, Literal),
    DataMinCardinality(u32, Box<DataPropertyExpression>),
    DataMaxCardinality(u32, Box<DataPropertyExpression>),
    DataExactCardinality(u32, Box<DataPropertyExpression>),
}

impl LegacyClassExpression {
    fn into_current(self) -> ClassExpression {
        match self {
            Self::Class(class) => ClassExpression::Class(class),
            Self::ObjectIntersectionOf(operands) => ClassExpression::ObjectIntersectionOf(
                operands
                    .into_iter()
                    .map(|expr| Box::new(expr.into_current()))
                    .collect(),
            ),
            Self::ObjectUnionOf(operands) => ClassExpression::ObjectUnionOf(
                operands
                    .into_iter()
                    .map(|expr| Box::new(expr.into_current()))
                    .collect(),
            ),
            Self::ObjectComplementOf(expr) => {
                ClassExpression::ObjectComplementOf(Box::new(expr.into_current()))
            }
            Self::ObjectOneOf(individuals) => ClassExpression::ObjectOneOf(individuals),
            Self::ObjectSomeValuesFrom(property, filler) => {
                ClassExpression::ObjectSomeValuesFrom(property, Box::new(filler.into_current()))
            }
            Self::ObjectAllValuesFrom(property, filler) => {
                ClassExpression::ObjectAllValuesFrom(property, Box::new(filler.into_current()))
            }
            Self::ObjectHasValue(property, individual) => {
                ClassExpression::ObjectHasValue(property, individual)
            }
            Self::ObjectHasSelf(property) => ClassExpression::ObjectHasSelf(property),
            Self::ObjectMinCardinality(cardinality, property) => {
                ClassExpression::ObjectMinCardinality(cardinality, property, None)
            }
            Self::ObjectMaxCardinality(cardinality, property) => {
                ClassExpression::ObjectMaxCardinality(cardinality, property, None)
            }
            Self::ObjectExactCardinality(cardinality, property) => {
                ClassExpression::ObjectExactCardinality(cardinality, property, None)
            }
            Self::DataSomeValuesFrom(property, range) => {
                ClassExpression::DataSomeValuesFrom(property, range)
            }
            Self::DataAllValuesFrom(property, range) => {
                ClassExpression::DataAllValuesFrom(property, range)
            }
            Self::DataHasValue(property, literal) => {
                ClassExpression::DataHasValue(property, literal)
            }
            Self::DataMinCardinality(cardinality, property) => {
                ClassExpression::DataMinCardinality(cardinality, property, None)
            }
            Self::DataMaxCardinality(cardinality, property) => {
                ClassExpression::DataMaxCardinality(cardinality, property, None)
            }
            Self::DataExactCardinality(cardinality, property) => {
                ClassExpression::DataExactCardinality(cardinality, property, None)
            }
        }
    }
}

fn old_class_expression(expr: &ClassExpression) -> IoResult<LegacyClassExpression> {
    Ok(match expr {
        ClassExpression::Class(class) => LegacyClassExpression::Class(class.clone()),
        ClassExpression::ObjectIntersectionOf(operands) => {
            LegacyClassExpression::ObjectIntersectionOf(old_class_expression_list(operands)?)
        }
        ClassExpression::ObjectUnionOf(operands) => {
            LegacyClassExpression::ObjectUnionOf(old_class_expression_list(operands)?)
        }
        ClassExpression::ObjectComplementOf(expr) => {
            LegacyClassExpression::ObjectComplementOf(Box::new(old_class_expression(expr)?))
        }
        ClassExpression::ObjectOneOf(individuals) => {
            LegacyClassExpression::ObjectOneOf(individuals.clone())
        }
        ClassExpression::ObjectSomeValuesFrom(property, filler) => {
            LegacyClassExpression::ObjectSomeValuesFrom(
                property.clone(),
                Box::new(old_class_expression(filler)?),
            )
        }
        ClassExpression::ObjectAllValuesFrom(property, filler) => {
            LegacyClassExpression::ObjectAllValuesFrom(
                property.clone(),
                Box::new(old_class_expression(filler)?),
            )
        }
        ClassExpression::ObjectHasValue(property, individual) => {
            LegacyClassExpression::ObjectHasValue(property.clone(), individual.clone())
        }
        ClassExpression::ObjectHasSelf(property) => {
            LegacyClassExpression::ObjectHasSelf(property.clone())
        }
        ClassExpression::ObjectMinCardinality(cardinality, property, None) => {
            LegacyClassExpression::ObjectMinCardinality(*cardinality, property.clone())
        }
        ClassExpression::ObjectMaxCardinality(cardinality, property, None) => {
            LegacyClassExpression::ObjectMaxCardinality(*cardinality, property.clone())
        }
        ClassExpression::ObjectExactCardinality(cardinality, property, None) => {
            LegacyClassExpression::ObjectExactCardinality(*cardinality, property.clone())
        }
        ClassExpression::ObjectMinCardinality(_, _, Some(_))
        | ClassExpression::ObjectMaxCardinality(_, _, Some(_))
        | ClassExpression::ObjectExactCardinality(_, _, Some(_)) => {
            return Err(unrepresentable_legacy());
        }
        ClassExpression::DataSomeValuesFrom(property, range) => {
            LegacyClassExpression::DataSomeValuesFrom(property.clone(), range.clone())
        }
        ClassExpression::DataAllValuesFrom(property, range) => {
            LegacyClassExpression::DataAllValuesFrom(property.clone(), range.clone())
        }
        ClassExpression::DataHasValue(property, literal) => {
            LegacyClassExpression::DataHasValue(property.clone(), literal.clone())
        }
        ClassExpression::DataMinCardinality(cardinality, property, None) => {
            LegacyClassExpression::DataMinCardinality(*cardinality, property.clone())
        }
        ClassExpression::DataMaxCardinality(cardinality, property, None) => {
            LegacyClassExpression::DataMaxCardinality(*cardinality, property.clone())
        }
        ClassExpression::DataExactCardinality(cardinality, property, None) => {
            LegacyClassExpression::DataExactCardinality(*cardinality, property.clone())
        }
        ClassExpression::DataMinCardinality(_, _, Some(_))
        | ClassExpression::DataMaxCardinality(_, _, Some(_))
        | ClassExpression::DataExactCardinality(_, _, Some(_)) => {
            return Err(unrepresentable_legacy());
        }
    })
}

fn old_class_expression_list(
    operands: &SmallVec<[Box<ClassExpression>; 4]>,
) -> IoResult<SmallVec<[Box<LegacyClassExpression>; 4]>> {
    let mut legacy = SmallVec::with_capacity(operands.len());
    for operand in operands {
        legacy.push(Box::new(old_class_expression(operand)?));
    }
    Ok(legacy)
}

fn unrepresentable_legacy() -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        "qualified cardinality is not representable in legacy binary v2/v3",
    )
}
