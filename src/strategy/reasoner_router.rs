//! Adaptive reasoner routing using OWL2 profile validation plus ontology structure analysis.
//!
//! This module centralizes runtime strategy decisions so benchmark and production
//! entry points use the same policy.

use std::sync::Arc;

use crate::core::ontology::Ontology;
use crate::logic::axioms::Axiom;
use crate::reasoner::hierarchical_classification::HierarchicalClassificationEngine;
use crate::strategy::ontology_analysis::OntologyCharacteristics;
use crate::strategy::profiles::{Owl2Profile, Owl2ProfileValidator};

const SPACL_MIN_CLASSES: usize = 100;
const SPACL_MIN_DISJUNCTION_DENSITY: f64 = 0.01;

/// Source used for the final routing decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingSource {
    Profile,
    Structural,
}

/// Selected reasoner for consistency checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsistencyReasoner {
    Simple,
    Speculative,
}

impl ConsistencyReasoner {
    pub fn as_str(self) -> &'static str {
        match self {
            ConsistencyReasoner::Simple => "SimpleReasoner",
            ConsistencyReasoner::Speculative => "SpeculativeTableauxReasoner",
        }
    }
}

/// Selected reasoner for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationReasoner {
    Hierarchical,
    Simple,
    Speculative,
}

impl ClassificationReasoner {
    pub fn as_str(self) -> &'static str {
        match self {
            ClassificationReasoner::Hierarchical => "HierarchicalClassificationEngine",
            ClassificationReasoner::Simple => "SimpleReasoner",
            ClassificationReasoner::Speculative => "SpeculativeTableauxReasoner",
        }
    }
}

/// Full routing decision for consistency checks.
#[derive(Debug, Clone)]
pub struct ConsistencyRoutingDecision {
    pub profile: Option<Owl2Profile>,
    pub characteristics: OntologyCharacteristics,
    pub reasoner: ConsistencyReasoner,
    pub source: RoutingSource,
    pub rationale: &'static str,
}

/// Full routing decision for classification tasks.
#[derive(Debug, Clone)]
pub struct ClassificationRoutingDecision {
    pub profile: Option<Owl2Profile>,
    pub characteristics: OntologyCharacteristics,
    pub reasoner: ClassificationReasoner,
    pub source: RoutingSource,
    pub rationale: &'static str,
}

/// Detect the most restrictive valid OWL2 profile for an ontology.
pub fn detect_profile(ontology: &Arc<Ontology>) -> Option<Owl2Profile> {
    let validator = Owl2ProfileValidator::new(Arc::clone(ontology)).ok()?;
    validator.get_most_restrictive_profile().ok().flatten()
}

/// Select the best consistency-checking reasoner.
pub fn select_consistency_reasoner(
    ontology: &Ontology,
    profile: Option<Owl2Profile>,
) -> ConsistencyRoutingDecision {
    let characteristics = OntologyCharacteristics::analyze(ontology);

    if let Some(profile) = profile {
        match profile {
            Owl2Profile::EL | Owl2Profile::QL | Owl2Profile::RL => {
                return ConsistencyRoutingDecision {
                    profile: Some(profile),
                    characteristics,
                    reasoner: ConsistencyReasoner::Simple,
                    source: RoutingSource::Profile,
                    rationale: "OWL2 profile permits fast deterministic path",
                };
            }
            Owl2Profile::Full => {}
        }
    }

    if should_use_speculative(ontology, &characteristics) {
        return ConsistencyRoutingDecision {
            profile,
            characteristics,
            reasoner: ConsistencyReasoner::Speculative,
            source: RoutingSource::Structural,
            rationale: "Disjunction-heavy ontology benefits from speculative branch parallelism",
        };
    }

    ConsistencyRoutingDecision {
        profile,
        characteristics,
        reasoner: ConsistencyReasoner::Simple,
        source: RoutingSource::Structural,
        rationale: "Low branching complexity favors cache-friendly simple reasoning",
    }
}

/// Select the best classification reasoner.
pub fn select_classification_reasoner(
    ontology: &Ontology,
    profile: Option<Owl2Profile>,
) -> ClassificationRoutingDecision {
    let characteristics = OntologyCharacteristics::analyze(ontology);
    let can_use_hierarchy = HierarchicalClassificationEngine::can_handle(ontology);

    if let Some(profile) = profile {
        match profile {
            Owl2Profile::EL if can_use_hierarchy => {
                return ClassificationRoutingDecision {
                    profile: Some(profile),
                    characteristics,
                    reasoner: ClassificationReasoner::Hierarchical,
                    source: RoutingSource::Profile,
                    rationale: "OWL2 EL hierarchy can be classified with linear-style indexing",
                };
            }
            Owl2Profile::QL | Owl2Profile::RL => {
                return ClassificationRoutingDecision {
                    profile: Some(profile),
                    characteristics,
                    reasoner: ClassificationReasoner::Simple,
                    source: RoutingSource::Profile,
                    rationale: "OWL2 QL/RL profile mapped to conservative cached classifier",
                };
            }
            Owl2Profile::EL | Owl2Profile::Full => {}
        }
    }

    if can_use_hierarchy {
        return ClassificationRoutingDecision {
            profile,
            characteristics,
            reasoner: ClassificationReasoner::Hierarchical,
            source: RoutingSource::Structural,
            rationale: "Taxonomic structure is suitable for hierarchical fast path",
        };
    }

    if should_use_speculative(ontology, &characteristics) {
        return ClassificationRoutingDecision {
            profile,
            characteristics,
            reasoner: ClassificationReasoner::Speculative,
            source: RoutingSource::Structural,
            rationale: "Disjunction-heavy structure favors speculative parallel exploration",
        };
    }

    ClassificationRoutingDecision {
        profile,
        characteristics,
        reasoner: ClassificationReasoner::Simple,
        source: RoutingSource::Structural,
        rationale: "Defaulting to stable cached classification path",
    }
}

fn should_use_speculative(ontology: &Ontology, characteristics: &OntologyCharacteristics) -> bool {
    let axiom_count = ontology.axioms().len();
    if axiom_count == 0 {
        return false;
    }

    let union_count = count_union_axioms(ontology);
    let disjunction_density = union_count as f64 / axiom_count as f64;

    characteristics.class_count >= SPACL_MIN_CLASSES
        && disjunction_density >= SPACL_MIN_DISJUNCTION_DENSITY
}

fn count_union_axioms(ontology: &Ontology) -> usize {
    ontology
        .axioms()
        .iter()
        .filter_map(|axiom| match axiom.as_ref() {
            Axiom::SubClassOf(sub) => Some(count_unions_in_expression(sub.super_class())),
            _ => None,
        })
        .sum()
}

fn count_unions_in_expression(expr: &crate::logic::axioms::ClassExpression) -> usize {
    use crate::logic::axioms::ClassExpression;

    match expr {
        ClassExpression::ObjectUnionOf(ops) => {
            1 + ops
                .iter()
                .map(|op| count_unions_in_expression(op))
                .sum::<usize>()
        }
        ClassExpression::ObjectIntersectionOf(ops) => {
            ops.iter().map(|op| count_unions_in_expression(op)).sum()
        }
        ClassExpression::ObjectComplementOf(inner) => count_unions_in_expression(inner),
        ClassExpression::ObjectSomeValuesFrom(_, inner) => count_unions_in_expression(inner),
        ClassExpression::ObjectAllValuesFrom(_, inner) => count_unions_in_expression(inner),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::Class;
    use crate::logic::axioms::{ClassExpression, SubClassOfAxiom};
    use crate::Ontology;
    use smallvec::smallvec;

    #[test]
    fn detects_el_profile_for_simple_hierarchy() {
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(a),
                ClassExpression::Class(b),
            ))
            .unwrap();

        let profile = detect_profile(&Arc::new(ontology));
        assert_eq!(profile, Some(Owl2Profile::EL));
    }

    #[test]
    fn routes_el_hierarchy_to_hierarchical_classification() {
        let mut ontology = Ontology::new();
        let a = Class::new("http://example.org/A");
        let b = Class::new("http://example.org/B");
        ontology.add_class(a.clone()).unwrap();
        ontology.add_class(b.clone()).unwrap();
        ontology
            .add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::Class(a),
                ClassExpression::Class(b),
            ))
            .unwrap();

        let decision = select_classification_reasoner(&ontology, Some(Owl2Profile::EL));
        assert_eq!(decision.reasoner, ClassificationReasoner::Hierarchical);
        assert_eq!(decision.source, RoutingSource::Profile);
    }

    #[test]
    fn routes_union_heavy_ontology_to_speculative_consistency() {
        let mut ontology = Ontology::new();

        for i in 0..120 {
            let sub = Class::new(format!("http://example.org/Sub{}", i));
            let left = Class::new(format!("http://example.org/Left{}", i));
            let right = Class::new(format!("http://example.org/Right{}", i));
            ontology.add_class(sub.clone()).unwrap();
            ontology.add_class(left.clone()).unwrap();
            ontology.add_class(right.clone()).unwrap();

            let union = ClassExpression::ObjectUnionOf(smallvec![
                Box::new(ClassExpression::Class(left)),
                Box::new(ClassExpression::Class(right)),
            ]);
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(ClassExpression::Class(sub), union))
                .unwrap();
        }

        let decision = select_consistency_reasoner(&ontology, None);
        assert_eq!(decision.reasoner, ConsistencyReasoner::Speculative);
    }

    #[test]
    fn routes_ql_profile_to_simple_classification() {
        let ontology = Ontology::new();
        let decision = select_classification_reasoner(&ontology, Some(Owl2Profile::QL));
        assert_eq!(decision.reasoner, ClassificationReasoner::Simple);
        assert_eq!(decision.source, RoutingSource::Profile);
    }
}
