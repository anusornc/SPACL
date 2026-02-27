//! Class-related OWL2 axioms
//!
//! This module contains axioms that define relationships between classes
//! and class assertions about individuals.

use crate::logic::axioms::class_expressions;
use crate::core::iri::IRI;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Subclass axiom: C ⊑ D
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubClassOfAxiom {
    sub_class: class_expressions::ClassExpression,
    super_class: class_expressions::ClassExpression,
}

impl SubClassOfAxiom {
    /// Create a new subclass axiom
    pub fn new(
        sub_class: class_expressions::ClassExpression,
        super_class: class_expressions::ClassExpression,
    ) -> Self {
        SubClassOfAxiom {
            sub_class,
            super_class,
        }
    }

    /// Get the subclass
    pub fn sub_class(&self) -> &class_expressions::ClassExpression {
        &self.sub_class
    }

    /// Get the superclass
    pub fn super_class(&self) -> &class_expressions::ClassExpression {
        &self.super_class
    }

    /// Check if this axiom involves a specific class
    pub fn involves_class(&self, class_iri: &IRI) -> bool {
        self.sub_class.contains_class(class_iri) || self.super_class.contains_class(class_iri)
    }
}

/// Equivalent classes axiom: C ≡ D
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalentClassesAxiom {
    classes: Vec<Arc<IRI>>,
}

impl EquivalentClassesAxiom {
    /// Create a new equivalent classes axiom
    pub fn new(classes: Vec<Arc<IRI>>) -> Self {
        EquivalentClassesAxiom { classes }
    }

    /// Get the equivalent classes
    pub fn classes(&self) -> &Vec<Arc<IRI>> {
        &self.classes
    }

    /// Check if a class is in this equivalence set
    pub fn contains_class(&self, class_iri: &IRI) -> bool {
        self.classes.iter().any(|c| c.as_ref() == class_iri)
    }
}

/// Disjoint classes axiom: C ⊓ D ⊑ ⊥
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisjointClassesAxiom {
    classes: Vec<Arc<IRI>>,
}

impl DisjointClassesAxiom {
    /// Create a new disjoint classes axiom
    pub fn new(classes: Vec<Arc<IRI>>) -> Self {
        DisjointClassesAxiom { classes }
    }

    /// Get the disjoint classes
    pub fn classes(&self) -> &Vec<Arc<IRI>> {
        &self.classes
    }

    /// Check if a class is in this disjointness set
    pub fn contains_class(&self, class_iri: &IRI) -> bool {
        self.classes.iter().any(|c| c.as_ref() == class_iri)
    }
}

/// Class assertion axiom: a ∈ C
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassAssertionAxiom {
    individual: Arc<IRI>,
    class_expr: class_expressions::ClassExpression,
}

impl ClassAssertionAxiom {
    /// Create a new class assertion axiom
    pub fn new(individual: Arc<IRI>, class_expr: class_expressions::ClassExpression) -> Self {
        ClassAssertionAxiom {
            individual,
            class_expr,
        }
    }

    /// Get the individual
    pub fn individual(&self) -> &Arc<IRI> {
        &self.individual
    }

    /// Get the class expression
    pub fn class_expr(&self) -> &class_expressions::ClassExpression {
        &self.class_expr
    }

    /// Check if this assertion involves a specific individual
    pub fn involves_individual(&self, individual_iri: &IRI) -> bool {
        self.individual.as_ref() == individual_iri
    }

    /// Check if this assertion involves a specific class
    pub fn involves_class(&self, class_iri: &IRI) -> bool {
        self.class_expr.contains_class(class_iri)
    }
}
