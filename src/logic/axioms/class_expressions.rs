//! Class expressions in OWL2
//!
//! Defines complex class expressions for building class hierarchies.

use super::property_expressions::{DataPropertyExpression, ObjectPropertyExpression};
use crate::core::entities::Class;
use crate::core::iri::IRI;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// A class expression in OWL2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClassExpression {
    /// Named class
    Class(Class),
    /// Object intersection of (C and D)
    ObjectIntersectionOf(SmallVec<[Box<ClassExpression>; 4]>),
    /// Object union of (C or D)
    ObjectUnionOf(SmallVec<[Box<ClassExpression>; 4]>),
    /// Object complement of (not C)
    ObjectComplementOf(Box<ClassExpression>),
    /// Object one of {a, b, c}
    ObjectOneOf(Box<SmallVec<[crate::core::entities::Individual; 8]>>),
    /// Object some values from (∃R.C)
    ObjectSomeValuesFrom(Box<ObjectPropertyExpression>, Box<ClassExpression>),
    /// Object all values from (∀R.C)
    ObjectAllValuesFrom(Box<ObjectPropertyExpression>, Box<ClassExpression>),
    /// Object has value (R(a))
    ObjectHasValue(Box<ObjectPropertyExpression>, crate::core::entities::Individual),
    /// Object has self (R(a,a))
    ObjectHasSelf(Box<ObjectPropertyExpression>),
    /// Object min cardinality (≥ n R)
    ObjectMinCardinality(u32, Box<ObjectPropertyExpression>),
    /// Object max cardinality (≤ n R)
    ObjectMaxCardinality(u32, Box<ObjectPropertyExpression>),
    /// Object exact cardinality (= n R)
    ObjectExactCardinality(u32, Box<ObjectPropertyExpression>),
    /// Data some values from (∃P.D)
    DataSomeValuesFrom(Box<DataPropertyExpression>, Box<DataRange>),
    /// Data all values from (∀P.D)
    DataAllValuesFrom(Box<DataPropertyExpression>, Box<DataRange>),
    /// Data has value (P(v))
    DataHasValue(Box<DataPropertyExpression>, crate::core::entities::Literal),
    /// Data min cardinality (≥ n P)
    DataMinCardinality(u32, Box<DataPropertyExpression>),
    /// Data max cardinality (≤ n P)
    DataMaxCardinality(u32, Box<DataPropertyExpression>),
    /// Data exact cardinality (= n P)
    DataExactCardinality(u32, Box<DataPropertyExpression>),
}

impl ClassExpression {
    /// Get the simplest form of this class expression
    pub fn simplify(&self) -> ClassExpression {
        match self {
            ClassExpression::ObjectIntersectionOf(operands) => {
                let simplified: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.iter().map(|op| Box::new(op.simplify())).collect();
                if simplified.len() == 1 {
                    *simplified[0].clone()
                } else {
                    ClassExpression::ObjectIntersectionOf(simplified)
                }
            }
            ClassExpression::ObjectUnionOf(operands) => {
                let simplified: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.iter().map(|op| Box::new(op.simplify())).collect();
                if simplified.len() == 1 {
                    *simplified[0].clone()
                } else {
                    ClassExpression::ObjectUnionOf(simplified)
                }
            }
            _ => self.clone(),
        }
    }

    /// Check if this is a simple named class
    pub fn is_named(&self) -> bool {
        matches!(self, ClassExpression::Class(_))
    }

    /// Get the named class if this is a simple class expression
    pub fn as_named(&self) -> Option<&Class> {
        match self {
            ClassExpression::Class(class) => Some(class),
            _ => None,
        }
    }

    /// Collect all subexpressions recursively
    pub fn collect_subexpressions(&self) -> Vec<&ClassExpression> {
        let mut result = Vec::new();
        self._collect_subexpressions(&mut result);
        result
    }

    /// Internal helper for collecting subexpressions
    fn _collect_subexpressions<'a>(&'a self, result: &mut Vec<&'a ClassExpression>) {
        result.push(self);

        match self {
            ClassExpression::Class(_) => {
                // No subexpressions for named classes
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                for operand in operands {
                    operand._collect_subexpressions(result);
                }
            }
            ClassExpression::ObjectUnionOf(operands) => {
                for operand in operands {
                    operand._collect_subexpressions(result);
                }
            }
            ClassExpression::ObjectComplementOf(operand) => {
                operand._collect_subexpressions(result);
            }
            ClassExpression::ObjectOneOf(_) => {
                // No subexpressions for individuals
            }
            ClassExpression::ObjectSomeValuesFrom(_, filler) => {
                filler._collect_subexpressions(result);
            }
            ClassExpression::ObjectAllValuesFrom(_, filler) => {
                filler._collect_subexpressions(result);
            }
            ClassExpression::ObjectHasValue(_, _) => {
                // No subexpressions for individuals
            }
            ClassExpression::ObjectHasSelf(_) => {
                // No subexpressions
            }
            ClassExpression::ObjectMinCardinality(_, _) => {
                // No subexpressions
            }
            ClassExpression::ObjectMaxCardinality(_, _) => {
                // No subexpressions
            }
            ClassExpression::ObjectExactCardinality(_, _) => {
                // No subexpressions
            }
            ClassExpression::DataSomeValuesFrom(_, _) => {
                // No class subexpressions for data ranges
            }
            ClassExpression::DataAllValuesFrom(_, _) => {
                // No class subexpressions for data ranges
            }
            ClassExpression::DataHasValue(_, _) => {
                // No class subexpressions
            }
            ClassExpression::DataMinCardinality(_, _) => {
                // No class subexpressions
            }
            ClassExpression::DataMaxCardinality(_, _) => {
                // No class subexpressions
            }
            ClassExpression::DataExactCardinality(_, _) => {
                // No class subexpressions
            }
        }
    }
}

impl ClassExpression {
    /// Check if this class expression contains a specific class
    pub fn contains_class(&self, class_iri: &IRI) -> bool {
        match self {
            ClassExpression::Class(class) => class.iri().as_ref() == class_iri,
            ClassExpression::ObjectIntersectionOf(operands) => {
                operands.iter().any(|op| op.contains_class(class_iri))
            }
            ClassExpression::ObjectUnionOf(operands) => {
                operands.iter().any(|op| op.contains_class(class_iri))
            }
            ClassExpression::ObjectComplementOf(expr) => expr.contains_class(class_iri),
            ClassExpression::ObjectOneOf(_) => false,
            ClassExpression::ObjectSomeValuesFrom(_, expr) => expr.contains_class(class_iri),
            ClassExpression::ObjectAllValuesFrom(_, expr) => expr.contains_class(class_iri),
            ClassExpression::ObjectHasValue(_, _) => false,
            ClassExpression::ObjectHasSelf(_) => false,
            ClassExpression::ObjectMinCardinality(_, _) => false,
            ClassExpression::ObjectMaxCardinality(_, _) => false,
            ClassExpression::ObjectExactCardinality(_, _) => false,
            ClassExpression::DataSomeValuesFrom(_, _) => false,
            ClassExpression::DataAllValuesFrom(_, _) => false,
            ClassExpression::DataHasValue(_, _) => false,
            ClassExpression::DataMinCardinality(_, _) => false,
            ClassExpression::DataMaxCardinality(_, _) => false,
            ClassExpression::DataExactCardinality(_, _) => false,
        }
    }
}

impl From<Class> for ClassExpression {
    fn from(class: Class) -> Self {
        ClassExpression::Class(class)
    }
}

/// Data ranges for data property expressions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataRange {
    /// Datatype restriction
    Datatype(IRI),
    /// Data intersection of
    DataIntersectionOf(Vec<DataRange>),
    /// Data union of
    DataUnionOf(Vec<DataRange>),
    /// Data complement of
    DataComplementOf(Box<DataRange>),
    /// Data one of
    DataOneOf(Vec<crate::core::entities::Literal>),
    /// Datatype restriction
    DatatypeRestriction(IRI, Vec<FacetRestriction>),
}

/// Facet restrictions for datatype restrictions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FacetRestriction {
    /// The facet (e.g., xsd:minInclusive)
    facet: IRI,
    /// The restriction value
    value: crate::core::entities::Literal,
}

impl FacetRestriction {
    /// Create a new facet restriction
    pub fn new<F: Into<IRI>, V: Into<crate::core::entities::Literal>>(facet: F, value: V) -> Self {
        FacetRestriction {
            facet: facet.into(),
            value: value.into(),
        }
    }

    /// Get the facet IRI
    pub fn facet(&self) -> &IRI {
        &self.facet
    }

    /// Get the restriction value
    pub fn value(&self) -> &crate::core::entities::Literal {
        &self.value
    }
}
