//! Manchester Syntax Abstract Syntax Tree (AST) Definitions
//!
//! This module defines the AST structures for Manchester Syntax parsing,
//! including class expressions, property expressions, and ontology constructs.

use smallvec;
use smallvec::SmallVec;

/// Abstract Syntax Tree for Manchester Syntax
#[derive(Debug, Clone, PartialEq)]
pub enum ManchesterAST {
    /// Prefix declaration: Prefix: prefix: `<iri>`
    PrefixDeclaration { prefix: String, iri: String },

    /// Class declaration: Class: className
    ClassDeclaration {
        name: String,
        sub_class_of: SmallVec<[Box<ClassExpression>; 4]>,
        equivalent_to: SmallVec<[Box<ClassExpression>; 4]>,
        disjoint_with: SmallVec<[Box<ClassExpression>; 4]>,
        annotations: SmallVec<[Annotation; 2]>,
    },

    /// Object property declaration: ObjectProperty: propName
    ObjectPropertyDeclaration {
        name: String,
        domain: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        range: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        characteristics: Box<SmallVec<[PropertyCharacteristic; 4]>>,
        sub_property_of: Box<SmallVec<[ObjectPropertyExpression; 4]>>,
        equivalent_to: Box<SmallVec<[ObjectPropertyExpression; 4]>>,
        disjoint_with: Box<SmallVec<[ObjectPropertyExpression; 4]>>,
        inverse_of: Box<SmallVec<[ObjectPropertyExpression; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Data property declaration: DataProperty: propName
    DataPropertyDeclaration {
        name: String,
        domain: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        range: Box<SmallVec<[DataRange; 4]>>,
        characteristics: Box<SmallVec<[PropertyCharacteristic; 4]>>,
        sub_property_of: Box<SmallVec<[DataPropertyExpression; 4]>>,
        equivalent_to: Box<SmallVec<[DataPropertyExpression; 4]>>,
        disjoint_with: Box<SmallVec<[DataPropertyExpression; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Individual declaration: Individual: individualName
    IndividualDeclaration {
        name: String,
        types: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        facts: Box<SmallVec<[PropertyAssertion; 8]>>,
        same_as: Box<SmallVec<[IndividualExpression; 4]>>,
        different_from: Box<SmallVec<[IndividualExpression; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Annotation declaration
    AnnotationDeclaration {
        name: String,
        annotations: SmallVec<[Annotation; 2]>,
    },

    /// Rule declaration (for SWRL rules)
    RuleDeclaration {
        name: Option<String>,
        body: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        head: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Disjoint classes axiom
    DisjointClasses {
        classes: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Equivalent classes axiom
    EquivalentClasses {
        classes: Box<SmallVec<[Box<ClassExpression>; 4]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Different individuals axiom
    DifferentIndividuals {
        individuals: Box<SmallVec<[IndividualExpression; 6]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },

    /// Same individual axiom
    SameIndividual {
        individuals: Box<SmallVec<[IndividualExpression; 6]>>,
        annotations: Box<SmallVec<[Annotation; 2]>>,
    },
}

/// Class expressions in Manchester Syntax
#[derive(Debug, Clone, PartialEq)]
pub enum ClassExpression {
    /// Named class: Class
    NamedClass(String),

    /// Object intersection: (Class1 and Class2 and ...)
    ObjectIntersection(SmallVec<[Box<ClassExpression>; 4]>),

    /// Object union: (Class1 or Class2 or ...)
    ObjectUnion(SmallVec<[Box<ClassExpression>; 4]>),

    /// Object complement: (not Class)
    ObjectComplement(Box<ClassExpression>),

    /// Object one of: {Individual1, Individual2, ...}
    ObjectOneOf(Vec<String>),

    /// Object some values from: (Property some Class)
    ObjectSomeValuesFrom(ObjectPropertyExpression, Box<ClassExpression>),

    /// Object all values from: (Property only Class)
    ObjectAllValuesFrom(ObjectPropertyExpression, Box<ClassExpression>),

    /// Object has value: (Property value Individual)
    ObjectHasValue(ObjectPropertyExpression, String),

    /// Object has self: (Property Self)
    ObjectHasSelf(ObjectPropertyExpression),

    /// Object min cardinality: (Property min n)
    ObjectMinCardinality(ObjectPropertyExpression, u32),

    /// Object max cardinality: (Property max n)
    ObjectMaxCardinality(ObjectPropertyExpression, u32),

    /// Object exact cardinality: (Property exactly n)
    ObjectExactCardinality(ObjectPropertyExpression, u32),

    /// Data some values from: (Property some DataRange)
    DataSomeValuesFrom(DataPropertyExpression, Box<DataRange>),

    /// Data all values from: (Property only DataRange)
    DataAllValuesFrom(DataPropertyExpression, Box<DataRange>),

    /// Data has value: (Property value Literal)
    DataHasValue(DataPropertyExpression, String),

    /// Data min cardinality: (Property min n)
    DataMinCardinality(DataPropertyExpression, u32),

    /// Data max cardinality: (Property max n)
    DataMaxCardinality(DataPropertyExpression, u32),

    /// Data exact cardinality: (Property exactly n)
    DataExactCardinality(DataPropertyExpression, u32),
}

/// Object property expressions
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectPropertyExpression {
    /// Named object property
    NamedProperty(String),

    /// Inverse property: inverse(Property)
    InverseProperty(Box<ObjectPropertyExpression>),
}

/// Data property expressions
#[derive(Debug, Clone, PartialEq)]
pub enum DataPropertyExpression {
    /// Named data property
    NamedProperty(String),
}

/// Data ranges
#[derive(Debug, Clone, PartialEq)]
pub enum DataRange {
    /// Named datatype: Datatype
    Datatype(String),

    /// Data intersection: (DataRange1 and DataRange2 and ...)
    DataIntersection(Vec<DataRange>),

    /// Data union: (DataRange1 or DataRange2 or ...)
    DataUnion(Vec<DataRange>),

    /// Data complement: (not DataRange)
    DataComplement(Box<DataRange>),

    /// Data one of: {"Literal1", "Literal2", ...}
    DataOneOf(Vec<String>),

    /// Datatype restriction: Datatype[facet restrictions]
    DatatypeRestriction {
        datatype: String,
        restrictions: Vec<FacetRestriction>,
    },
}

/// Individual expressions
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum IndividualExpression {
    /// Named individual
    NamedIndividual(String),

    /// Anonymous individual (not supported in standard Manchester Syntax)
    AnonymousIndividual,
}

/// Property assertions
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyAssertion {
    /// Object property assertion: Individual Property Individual
    ObjectPropertyAssertion {
        subject: String,
        property: ObjectPropertyExpression,
        object: String,
    },

    /// Data property assertion: Individual Property Literal
    DataPropertyAssertion {
        subject: String,
        property: DataPropertyExpression,
        object: String,
    },

    /// Negative object property assertion: Individual not (Property Individual)
    NegativeObjectPropertyAssertion {
        subject: String,
        property: ObjectPropertyExpression,
        object: String,
    },

    /// Negative data property assertion: Individual not (Property Literal)
    NegativeDataPropertyAssertion {
        subject: String,
        property: DataPropertyExpression,
        object: String,
    },
}

/// Property characteristics
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PropertyCharacteristic {
    /// Functional property
    Functional,

    /// Inverse functional property
    InverseFunctional,

    /// Transitive property
    Transitive,

    /// Symmetric property
    Symmetric,

    /// Asymmetric property
    Asymmetric,

    /// Reflexive property
    Reflexive,

    /// Irreflexive property
    Irreflexive,

    /// Annotation property
    Annotation,

    /// Ontology property
    Ontology,

    /// Data property
    Data,

    /// Object property
    Object,
}

/// Facet restrictions for datatype restrictions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FacetRestriction {
    /// The facet (e.g., xsd:minInclusive)
    pub facet: String,

    /// The restriction value
    pub value: String,
}

/// Annotations
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The annotation property
    pub property: String,

    /// The annotation value
    pub value: AnnotationValue,
}

/// Annotation values
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnnotationValue {
    /// IRI reference
    IRI(String),

    /// Literal value
    Literal(String),

    /// Anonymous individual
    AnonymousIndividual,
}

impl ManchesterAST {
    /// Get all prefix declarations in this AST
    pub fn get_prefix_declarations(&self) -> Vec<(String, String)> {
        let mut prefixes = Vec::new();

        match self {
            ManchesterAST::PrefixDeclaration { prefix, iri } => {
                prefixes.push((prefix.clone(), iri.clone()));
            }
            ManchesterAST::ClassDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::ObjectPropertyDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::DataPropertyDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::IndividualDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::AnnotationDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::RuleDeclaration { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::DisjointClasses { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::EquivalentClasses { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::DifferentIndividuals { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
            ManchesterAST::SameIndividual { annotations, .. } => {
                prefixes.extend(annotations.iter().flat_map(|a| a.get_prefix_declarations()));
            }
        }

        prefixes
    }

    /// Validate this AST node
    pub fn validate(&self) -> Result<(), String> {
        match self {
            ManchesterAST::PrefixDeclaration { prefix, iri } => {
                if prefix.is_empty() {
                    return Err("Prefix cannot be empty".to_string());
                }
                if iri.is_empty() {
                    return Err("IRI cannot be empty".to_string());
                }
            }
            ManchesterAST::ClassDeclaration { name, .. } => {
                if name.is_empty() {
                    return Err("Class name cannot be empty".to_string());
                }
            }
            ManchesterAST::ObjectPropertyDeclaration { name, .. } => {
                if name.is_empty() {
                    return Err("Property name cannot be empty".to_string());
                }
            }
            ManchesterAST::DataPropertyDeclaration { name, .. } => {
                if name.is_empty() {
                    return Err("Property name cannot be empty".to_string());
                }
            }
            ManchesterAST::IndividualDeclaration { name, .. } => {
                if name.is_empty() {
                    return Err("Individual name cannot be empty".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl ClassExpression {
    /// Get the simplest form of this class expression
    pub fn simplify(&self) -> ClassExpression {
        match self {
            ClassExpression::ObjectIntersection(operands) => {
                let simplified: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.iter().map(|op| Box::new(op.simplify())).collect();
                if simplified.len() == 1 {
                    *simplified[0].clone()
                } else {
                    ClassExpression::ObjectIntersection(simplified)
                }
            }
            ClassExpression::ObjectUnion(operands) => {
                let simplified: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.iter().map(|op| Box::new(op.simplify())).collect();
                if simplified.len() == 1 {
                    *simplified[0].clone()
                } else {
                    ClassExpression::ObjectUnion(simplified)
                }
            }
            ClassExpression::ObjectComplement(expr) => {
                ClassExpression::ObjectComplement(Box::new(expr.simplify()))
            }
            _ => self.clone(),
        }
    }

    /// Check if this is a simple named class
    pub fn is_named(&self) -> bool {
        matches!(self, ClassExpression::NamedClass(_))
    }

    /// Get the named class name if this is a simple class expression
    pub fn as_named(&self) -> Option<&str> {
        match self {
            ClassExpression::NamedClass(name) => Some(name),
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
            ClassExpression::ObjectIntersection(operands) => {
                for operand in operands {
                    operand._collect_subexpressions(result);
                }
            }
            ClassExpression::ObjectUnion(operands) => {
                for operand in operands {
                    operand._collect_subexpressions(result);
                }
            }
            ClassExpression::ObjectComplement(expr) => {
                expr._collect_subexpressions(result);
            }
            ClassExpression::ObjectSomeValuesFrom(_, expr) => {
                expr._collect_subexpressions(result);
            }
            ClassExpression::ObjectAllValuesFrom(_, expr) => {
                expr._collect_subexpressions(result);
            }
            _ => {}
        }
    }
}

impl Annotation {
    /// Get all prefix declarations in this annotation
    pub fn get_prefix_declarations(&self) -> Vec<(String, String)> {
        match &self.value {
            AnnotationValue::IRI(iri) => {
                // Extract prefix from IRI if possible
                if let Some((prefix, rest)) = iri.split_once(':') {
                    vec![(prefix.to_string(), format!("{}:{}", prefix, rest))]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

impl Default for ManchesterAST {
    fn default() -> Self {
        ManchesterAST::ClassDeclaration {
            name: String::new(),
            sub_class_of: SmallVec::new(),
            equivalent_to: SmallVec::new(),
            disjoint_with: SmallVec::new(),
            annotations: SmallVec::new(),
        }
    }
}
