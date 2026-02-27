//! Consistency checking for OWL2 ontologies
//!
//! Provides algorithms for checking ontology consistency and detecting contradictions.

use crate::core::entities::*;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::reasoner::tableaux::TableauxReasoner;
use crate::logic::axioms::Axiom;

use smallvec::SmallVec;
use std::sync::Arc;

/// Consistency checker for OWL2 ontologies
pub struct ConsistencyChecker {
    tableaux_reasoner: TableauxReasoner,
    config: ConsistencyConfig,
}

/// Consistency checking configuration
#[derive(Debug, Clone)]
pub struct ConsistencyConfig {
    /// Enable detailed explanation generation
    pub explain_inconsistencies: bool,
    /// Maximum number of explanations to generate
    pub max_explanations: usize,
    /// Enable incremental consistency checking
    pub incremental: bool,
    /// Timeout in milliseconds
    pub timeout: Option<u64>,
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        ConsistencyConfig {
            explain_inconsistencies: true,
            max_explanations: 10,
            incremental: true,
            timeout: Some(30000), // 30 seconds default
        }
    }
}

/// Consistency check result
#[derive(Debug, Clone)]
pub struct ConsistencyResult {
    pub is_consistent: bool,
    pub explanations: Vec<InconsistencyExplanation>,
    pub stats: ConsistencyStats,
}

/// Explanation for inconsistency
#[derive(Debug, Clone)]
pub struct InconsistencyExplanation {
    pub description: String,
    pub involved_axioms: SmallVec<[crate::Axiom; 8]>,
    pub contradiction_type: ContradictionType,
}

/// Types of contradictions that can be detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContradictionType {
    /// Direct class contradiction (C and ¬C)
    ClassContradiction(IRI),
    /// Property range contradiction
    PropertyRangeContradiction(IRI),
    /// Property domain contradiction
    PropertyDomainContradiction(IRI),
    /// Cardinality contradiction
    CardinalityContradiction,
    /// Disjoint classes contradiction
    DisjointClassesContradiction(Vec<IRI>),
    /// Unsatisfiable class
    UnsatisfiableClass(IRI),
    /// Other contradiction
    Other(String),
}

/// Consistency checking statistics
#[derive(Debug, Clone)]
pub struct ConsistencyStats {
    pub checks_performed: usize,
    pub contradictions_found: usize,
    pub time_ms: u64,
    pub axioms_analyzed: usize,
}

impl ConsistencyChecker {
    /// Create a new consistency checker
    pub fn new(ontology: Ontology) -> Self {
        Self::with_config(&ontology, ConsistencyConfig::default())
    }

    /// Create a new consistency checker with custom configuration
    pub fn with_config(ontology: &Ontology, config: ConsistencyConfig) -> Self {
        let tableaux_reasoner = TableauxReasoner::new(ontology.clone());

        ConsistencyChecker {
            tableaux_reasoner,
            config,
        }
    }

    /// Check if the ontology is consistent
    pub fn is_consistent(&mut self) -> OwlResult<bool> {
        let result = self.check_consistency()?;
        Ok(result.is_consistent)
    }

    /// Perform comprehensive consistency checking
    pub fn check_consistency(&mut self) -> OwlResult<ConsistencyResult> {
        let start_time = std::time::Instant::now();
        let mut explanations = Vec::new();
        let mut contradictions_found = 0;
        let mut axioms_analyzed = 0;

        // Check basic consistency using tableaux reasoning
        let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing").map_err(|e| {
            OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Thing".to_string(),
                context: format!("Failed to create owl:Thing IRI: {}", e),
            }
        })?;
        let tableaux_result = self.tableaux_reasoner.is_class_satisfiable(&thing_iri)?;

        if !tableaux_result {
            explanations.push(InconsistencyExplanation {
                description: "Ontology is inconsistent: owl:Thing is unsatisfiable".to_string(),
                involved_axioms: SmallVec::new(),
                contradiction_type: ContradictionType::Other("owl:Thing unsatisfiable".to_string()),
            });
            contradictions_found += 1;
        }

        // Check for direct contradictions
        let direct_contradictions = self.find_direct_contradictions()?;
        contradictions_found += direct_contradictions.len();
        explanations.extend(direct_contradictions);
        axioms_analyzed += self.tableaux_reasoner.ontology.axioms().len();

        // Check for unsatisfiable classes
        let unsatisfiable_classes = self.find_unsatisfiable_classes()?;
        contradictions_found += unsatisfiable_classes.len();
        explanations.extend(unsatisfiable_classes);

        // Limit number of explanations
        if explanations.len() > self.config.max_explanations {
            explanations.truncate(self.config.max_explanations);
        }

        let time_ms = start_time.elapsed().as_millis() as u64;

        Ok(ConsistencyResult {
            is_consistent: contradictions_found == 0,
            explanations,
            stats: ConsistencyStats {
                checks_performed: 1 + contradictions_found,
                contradictions_found,
                time_ms,
                axioms_analyzed,
            },
        })
    }

    /// Find direct contradictions in the ontology
    fn find_direct_contradictions(&self) -> OwlResult<Vec<InconsistencyExplanation>> {
        let mut contradictions = Vec::new();

        // Check for disjoint class contradictions
        for disjoint_axiom in self.tableaux_reasoner.ontology.disjoint_classes_axioms() {
            if let Some(contradiction) = self.check_disjoint_classes_contradiction(disjoint_axiom) {
                contradictions.push(contradiction);
            }
        }

        // Check for equivalent class contradictions
        for equiv_axiom in self.tableaux_reasoner.ontology.equivalent_classes_axioms() {
            if let Some(contradiction) = self.check_equivalent_classes_contradiction(equiv_axiom) {
                contradictions.push(contradiction);
            }
        }

        // Check for property characteristic contradictions
        contradictions.extend(self.check_property_contradictions()?);

        Ok(contradictions)
    }

    /// Check for contradictions in disjoint classes axioms
    fn check_disjoint_classes_contradiction(
        &self,
        axiom: &crate::logic::axioms::DisjointClassesAxiom,
    ) -> Option<InconsistencyExplanation> {
        let classes = axiom.classes();

        // Check if any two disjoint classes are declared equivalent
        for i in 0..classes.len() {
            for j in i + 1..classes.len() {
                let class1 = &classes[i];
                let class2 = &classes[j];

                // Check if class1 and class2 are declared equivalent
                for equiv_axiom in self.tableaux_reasoner.ontology.equivalent_classes_axioms() {
                    let equiv_classes = equiv_axiom.classes();
                    if equiv_classes.contains(class1) && equiv_classes.contains(class2) {
                        return Some(InconsistencyExplanation {
                            description: format!(
                                "Classes {} and {} are both disjoint and equivalent",
                                class1, class2
                            ),
                            involved_axioms: vec![
                                Axiom::DisjointClasses(Box::new(axiom.clone())),
                                Axiom::EquivalentClasses(Box::new(equiv_axiom.clone())),
                            ]
                            .into(),
                            contradiction_type: ContradictionType::DisjointClassesContradiction(
                                vec![(**class1).clone(), (**class2).clone()],
                            ),
                        });
                    }
                }
            }
        }

        None
    }

    /// Check for contradictions in equivalent classes axioms
    fn check_equivalent_classes_contradiction(
        &self,
        axiom: &crate::logic::axioms::EquivalentClassesAxiom,
    ) -> Option<InconsistencyExplanation> {
        let classes = axiom.classes();

        // Check if any two equivalent classes are declared disjoint
        for i in 0..classes.len() {
            for j in i + 1..classes.len() {
                let class1 = &classes[i];
                let class2 = &classes[j];

                // Check if class1 and class2 are declared disjoint
                for disjoint_axiom in self.tableaux_reasoner.ontology.disjoint_classes_axioms() {
                    let disjoint_classes = disjoint_axiom.classes();
                    if disjoint_classes.contains(class1) && disjoint_classes.contains(class2) {
                        return Some(InconsistencyExplanation {
                            description: format!(
                                "Classes {} and {} are both equivalent and disjoint",
                                class1, class2
                            ),
                            involved_axioms: vec![
                                Axiom::EquivalentClasses(Box::new(axiom.clone())),
                                Axiom::DisjointClasses(Box::new(disjoint_axiom.clone())),
                            ]
                            .into(),
                            contradiction_type: ContradictionType::DisjointClassesContradiction(
                                vec![(**class1).clone(), (**class2).clone()],
                            ),
                        });
                    }
                }
            }
        }

        None
    }

    /// Check for property characteristic contradictions
    fn check_property_contradictions(&self) -> OwlResult<Vec<InconsistencyExplanation>> {
        let mut contradictions = Vec::new();

        for prop in self.tableaux_reasoner.ontology.object_properties() {
            let contradictions_for_prop =
                self.check_property_characteristic_contradictions(prop)?;
            contradictions.extend(contradictions_for_prop);
        }

        Ok(contradictions)
    }

    /// Check for contradictions in property characteristics
    fn check_property_characteristic_contradictions(
        &self,
        prop: &ObjectProperty,
    ) -> OwlResult<Vec<InconsistencyExplanation>> {
        let mut contradictions = Vec::new();
        let characteristics = prop.characteristics();

        // Check for incompatible characteristics
        if characteristics.contains(&ObjectPropertyCharacteristic::Functional)
            && characteristics.contains(&ObjectPropertyCharacteristic::InverseFunctional)
        {
            // Check if the property has both functional and inverse-functional characteristics
            // This might lead to contradictions in certain scenarios
        }

        if characteristics.contains(&ObjectPropertyCharacteristic::Asymmetric)
            && characteristics.contains(&ObjectPropertyCharacteristic::Reflexive)
        {
            // Asymmetric and reflexive properties are contradictory
            contradictions.push(InconsistencyExplanation {
                description: format!("Property {} is both asymmetric and reflexive", prop.iri()),
                involved_axioms: SmallVec::new(),
                contradiction_type: ContradictionType::Other(format!(
                    "Asymmetric and reflexive property: {}",
                    prop.iri()
                )),
            });
        }

        if characteristics.contains(&ObjectPropertyCharacteristic::Irreflexive)
            && characteristics.contains(&ObjectPropertyCharacteristic::Reflexive)
        {
            // Irreflexive and reflexive properties are contradictory
            contradictions.push(InconsistencyExplanation {
                description: format!("Property {} is both irreflexive and reflexive", prop.iri()),
                involved_axioms: SmallVec::new(),
                contradiction_type: ContradictionType::Other(format!(
                    "Irreflexive and reflexive property: {}",
                    prop.iri()
                )),
            });
        }

        Ok(contradictions)
    }

    /// Find unsatisfiable classes in the ontology
    fn find_unsatisfiable_classes(&mut self) -> OwlResult<Vec<InconsistencyExplanation>> {
        let mut unsatisfiable = Vec::new();

        let classes: Vec<_> = self
            .tableaux_reasoner
            .ontology
            .classes()
            .iter()
            .cloned()
            .collect();
        for class in classes {
            let class_iri = class.iri();

            // Check if the class is satisfiable
            let is_satisfiable = self.tableaux_reasoner.is_class_satisfiable(class_iri)?;

            if !is_satisfiable {
                unsatisfiable.push(InconsistencyExplanation {
                    description: format!("Class {} is unsatisfiable", class_iri),
                    involved_axioms: self.find_axioms_involving_class(class_iri)?.into(),
                    contradiction_type: ContradictionType::UnsatisfiableClass(
                        (**class_iri).clone(),
                    ),
                });
            }
        }

        Ok(unsatisfiable)
    }

    /// Find all axioms that involve a specific class
    fn find_axioms_involving_class(&self, class_iri: &IRI) -> OwlResult<Vec<Axiom>> {
        let mut axioms = Vec::new();

        // Check subclass axioms
        for axiom in self.tableaux_reasoner.ontology.subclass_axioms() {
            if axiom.involves_class(class_iri) {
                axioms.push(Axiom::SubClassOf(Box::new(axiom.clone())));
            }
        }

        // Check equivalent classes axioms
        for axiom in self.tableaux_reasoner.ontology.equivalent_classes_axioms() {
            if axiom.classes().contains(&Arc::new((*class_iri).clone())) {
                axioms.push(Axiom::EquivalentClasses(Box::new(axiom.clone())));
            }
        }

        // Check disjoint classes axioms
        for axiom in self.tableaux_reasoner.ontology.disjoint_classes_axioms() {
            if axiom.classes().contains(&Arc::new((*class_iri).clone())) {
                axioms.push(Axiom::DisjointClasses(Box::new(axiom.clone())));
            }
        }

        // Check class assertions
        for axiom in self.tableaux_reasoner.ontology.class_assertions() {
            if axiom.class_expr().contains_class(class_iri) {
                axioms.push(Axiom::ClassAssertion(Box::new(axiom.clone())));
            }
        }

        Ok(axioms)
    }

    /// Check if adding an axiom would make the ontology inconsistent
    pub fn would_be_consistent_with(&mut self, axiom: &Axiom) -> OwlResult<bool> {
        // Create a temporary ontology with the new axiom
        let mut temp_ontology = self.tableaux_reasoner.ontology.as_ref().clone();

        // Add the axiom to the temporary ontology
        match axiom {
            Axiom::SubClassOf(axiom) => temp_ontology.add_subclass_axiom((**axiom).clone())?,
            Axiom::EquivalentClasses(axiom) => {
                temp_ontology.add_equivalent_classes_axiom((**axiom).clone())?
            }
            Axiom::DisjointClasses(axiom) => {
                temp_ontology.add_disjoint_classes_axiom((**axiom).clone())?
            }
            Axiom::ClassAssertion(axiom) => temp_ontology.add_class_assertion((**axiom).clone())?,
            Axiom::PropertyAssertion(axiom) => {
                temp_ontology.add_property_assertion((**axiom).clone())?
            }
            _ => return Ok(true), // Other axiom types not yet supported
        }

        // Check consistency of the temporary ontology
        let mut temp_checker = ConsistencyChecker::new(temp_ontology);
        temp_checker.is_consistent()
    }

    /// Get minimal explanations for inconsistency
    pub fn get_minimal_explanations(&mut self) -> OwlResult<Vec<InconsistencyExplanation>> {
        let result = self.check_consistency()?;

        // Filter for minimal explanations (those with fewest involved axioms)
        let mut explanations = result.explanations;
        explanations.sort_by_key(|exp| exp.involved_axioms.len());

        // Take explanations with minimal size
        if let Some(min_size) = explanations.first().map(|exp| exp.involved_axioms.len()) {
            explanations.retain(|exp| exp.involved_axioms.len() == min_size);
        }

        Ok(explanations)
    }
}
