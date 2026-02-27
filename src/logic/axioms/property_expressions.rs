//! Property expressions in OWL2
//!
//! Defines complex property expressions for building property hierarchies.

use crate::core::entities::{DataProperty, ObjectProperty};
use serde::{Deserialize, Serialize};

/// Object property expressions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectPropertyExpression {
    /// Named object property
    ObjectProperty(Box<ObjectProperty>),
    /// Inverse object property (R⁻)
    ObjectInverseOf(Box<ObjectPropertyExpression>),
}

impl ObjectPropertyExpression {
    /// Get the inverse of this property expression
    pub fn inverse(&self) -> ObjectPropertyExpression {
        match self {
            ObjectPropertyExpression::ObjectProperty(prop) => {
                ObjectPropertyExpression::ObjectInverseOf(Box::new(
                    ObjectPropertyExpression::ObjectProperty(prop.clone()),
                ))
            }
            ObjectPropertyExpression::ObjectInverseOf(prop) => *prop.clone(),
        }
    }

    /// Check if this is a simple named property
    pub fn is_named(&self) -> bool {
        matches!(self, ObjectPropertyExpression::ObjectProperty(_))
    }

    /// Get the named property if this is a simple property expression
    pub fn as_named(&self) -> Option<&ObjectProperty> {
        match self {
            ObjectPropertyExpression::ObjectProperty(prop) => Some(prop),
            _ => None,
        }
    }
}

impl From<ObjectProperty> for ObjectPropertyExpression {
    fn from(prop: ObjectProperty) -> Self {
        ObjectPropertyExpression::ObjectProperty(Box::new(prop))
    }
}

/// Data property expressions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataPropertyExpression {
    /// Named data property
    DataProperty(DataProperty),
}

impl DataPropertyExpression {
    /// Check if this is a simple named property
    pub fn is_named(&self) -> bool {
        matches!(self, DataPropertyExpression::DataProperty(_))
    }

    /// Get the named property if this is a simple property expression
    pub fn as_named(&self) -> Option<&DataProperty> {
        match self {
            DataPropertyExpression::DataProperty(prop) => Some(prop),
        }
    }
}

impl From<DataProperty> for DataPropertyExpression {
    fn from(prop: DataProperty) -> Self {
        DataPropertyExpression::DataProperty(prop)
    }
}
