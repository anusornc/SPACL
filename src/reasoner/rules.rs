//! Rule-based reasoning engine for OWL2 ontologies

use crate::logic::axioms::*;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Rule-based reasoning engine
pub struct RuleEngine {
    ontology: Arc<Ontology>,
    rules: Vec<ReasoningRule>,
    config: RuleConfig,
    derived_facts: DerivedFacts,
}

/// Rule engine configuration
#[derive(Debug, Clone)]
pub struct RuleConfig {
    /// Maximum number of rule applications
    pub max_iterations: usize,
    /// Enable forward chaining
    pub forward_chaining: bool,
    /// Enable backward chaining
    pub backward_chaining: bool,
    /// Enable debugging output
    pub debug: bool,
}

impl Default for RuleConfig {
    fn default() -> Self {
        RuleConfig {
            max_iterations: 1000,
            forward_chaining: true,
            backward_chaining: false,
            debug: false,
        }
    }
}

/// A reasoning rule
#[derive(Debug, Clone)]
pub struct ReasoningRule {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    description: String,
    pattern: RulePattern,
    action: RuleAction,
    #[allow(dead_code)]
    priority: u32,
}

/// Rule pattern (conditions for rule application)
#[derive(Debug, Clone)]
pub struct RulePattern {
    conditions: Vec<PatternCondition>,
}

/// Pattern condition
#[derive(Debug, Clone)]
pub enum PatternCondition {
    /// Class assertion condition
    ClassAssertion {
        individual: PatternVar,
        class: PatternVar,
    },
    /// Property assertion condition
    PropertyAssertion {
        subject: PatternVar,
        property: PatternVar,
        object: PatternVar,
    },
    /// Subclass condition
    SubClassOf {
        sub_class: PatternVar,
        super_class: PatternVar,
    },
    /// Equivalent classes condition
    EquivalentClasses { classes: Vec<PatternVar> },
    /// Disjoint classes condition
    DisjointClasses { classes: Vec<PatternVar> },
}

/// Rule action (consequences of rule application)
#[derive(Debug, Clone)]
pub struct RuleAction {
    consequences: Vec<RuleConsequence>,
}

/// Rule consequence
#[derive(Debug, Clone)]
pub enum RuleConsequence {
    /// Add class assertion
    AddClassAssertion {
        individual: PatternVar,
        class: PatternVar,
    },
    /// Add property assertion
    AddPropertyAssertion {
        subject: PatternVar,
        property: PatternVar,
        object: PatternVar,
    },
    /// Add subclass relationship
    AddSubClassOf {
        sub_class: PatternVar,
        super_class: PatternVar,
    },
}

/// Pattern variable
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PatternVar {
    /// Variable that can match any value
    Variable(String),
    /// Constant value
    Constant(IRI),
}

/// Derived facts from rule application
#[derive(Debug, Default)]
pub struct DerivedFacts {
    class_assertions: HashSet<(IRI, IRI)>,
    property_assertions: HashSet<(IRI, IRI, IRI)>,
    subclass_relationships: HashSet<(IRI, IRI)>,
}

impl RuleEngine {
    /// Create a new rule engine
    pub fn new(ontology: Ontology) -> Self {
        Self::with_config(ontology, RuleConfig::default())
    }

    /// Create a new rule engine with custom configuration
    pub fn with_config(ontology: Ontology, config: RuleConfig) -> Self {
        let rules = Self::create_standard_rules();
        let derived_facts = DerivedFacts::default();

        RuleEngine {
            ontology: Arc::new(ontology),
            rules,
            config,
            derived_facts,
        }
    }

    /// Create standard OWL2 reasoning rules
    fn create_standard_rules() -> Vec<ReasoningRule> {
        vec![
            // Transitivity rule for properties
            ReasoningRule {
                name: "TransitiveProperty".to_string(),
                description: "If R is transitive and R(a,b) and R(b,c), then R(a,c)".to_string(),
                pattern: RulePattern {
                    conditions: vec![
                        PatternCondition::PropertyAssertion {
                            subject: PatternVar::Variable("?a".to_string()),
                            property: PatternVar::Variable("?r".to_string()),
                            object: PatternVar::Variable("?b".to_string()),
                        },
                        PatternCondition::PropertyAssertion {
                            subject: PatternVar::Variable("?b".to_string()),
                            property: PatternVar::Variable("?r".to_string()),
                            object: PatternVar::Variable("?c".to_string()),
                        },
                    ],
                },
                action: RuleAction {
                    consequences: vec![RuleConsequence::AddPropertyAssertion {
                        subject: PatternVar::Variable("?a".to_string()),
                        property: PatternVar::Variable("?r".to_string()),
                        object: PatternVar::Variable("?c".to_string()),
                    }],
                },
                priority: 100,
            },
            // Subclass transitivity rule
            ReasoningRule {
                name: "SubClassTransitivity".to_string(),
                description: "If A ⊑ B and B ⊑ C, then A ⊑ C".to_string(),
                pattern: RulePattern {
                    conditions: vec![
                        PatternCondition::SubClassOf {
                            sub_class: PatternVar::Variable("?a".to_string()),
                            super_class: PatternVar::Variable("?b".to_string()),
                        },
                        PatternCondition::SubClassOf {
                            sub_class: PatternVar::Variable("?b".to_string()),
                            super_class: PatternVar::Variable("?c".to_string()),
                        },
                    ],
                },
                action: RuleAction {
                    consequences: vec![RuleConsequence::AddSubClassOf {
                        sub_class: PatternVar::Variable("?a".to_string()),
                        super_class: PatternVar::Variable("?c".to_string()),
                    }],
                },
                priority: 90,
            },
            // Inheritance rule: if C ⊑ D and a ∈ C, then a ∈ D
            ReasoningRule {
                name: "ClassInheritance".to_string(),
                description: "If C ⊑ D and a ∈ C, then a ∈ D".to_string(),
                pattern: RulePattern {
                    conditions: vec![
                        PatternCondition::SubClassOf {
                            sub_class: PatternVar::Variable("?c".to_string()),
                            super_class: PatternVar::Variable("?d".to_string()),
                        },
                        PatternCondition::ClassAssertion {
                            individual: PatternVar::Variable("?a".to_string()),
                            class: PatternVar::Variable("?c".to_string()),
                        },
                    ],
                },
                action: RuleAction {
                    consequences: vec![RuleConsequence::AddClassAssertion {
                        individual: PatternVar::Variable("?a".to_string()),
                        class: PatternVar::Variable("?d".to_string()),
                    }],
                },
                priority: 80,
            },
            // Symmetric property rule
            ReasoningRule {
                name: "SymmetricProperty".to_string(),
                description: "If R is symmetric and R(a,b), then R(b,a)".to_string(),
                pattern: RulePattern {
                    conditions: vec![PatternCondition::PropertyAssertion {
                        subject: PatternVar::Variable("?a".to_string()),
                        property: PatternVar::Variable("?r".to_string()),
                        object: PatternVar::Variable("?b".to_string()),
                    }],
                },
                action: RuleAction {
                    consequences: vec![RuleConsequence::AddPropertyAssertion {
                        subject: PatternVar::Variable("?b".to_string()),
                        property: PatternVar::Variable("?r".to_string()),
                        object: PatternVar::Variable("?a".to_string()),
                    }],
                },
                priority: 70,
            },
        ]
    }

    /// Run forward chaining reasoning
    pub fn run_forward_chaining(&mut self) -> OwlResult<usize> {
        let mut rules_applied = 0;
        let mut iterations = 0;

        while iterations < self.config.max_iterations {
            let mut new_facts_this_iteration = 0;

            let rules: Vec<ReasoningRule> = self.rules.clone();
            for rule in rules {
                if let Some(new_facts) = self.apply_rule(&rule)? {
                    rules_applied += 1;
                    new_facts_this_iteration += new_facts;
                }
            }

            if new_facts_this_iteration == 0 {
                // Fixed point reached
                break;
            }

            iterations += 1;
        }

        Ok(rules_applied)
    }

    /// Apply a single rule to the ontology
    fn apply_rule(&mut self, rule: &ReasoningRule) -> OwlResult<Option<usize>> {
        let mut new_facts = 0;

        // Find all matches for the rule pattern
        let matches = self.find_matches(&rule.pattern)?;

        // Apply the rule action to each match
        for match_binding in matches {
            new_facts += self.apply_action(&rule.action, &match_binding)?;
        }

        if new_facts > 0 {
            Ok(Some(new_facts))
        } else {
            Ok(None)
        }
    }

    /// Find all matches for a rule pattern
    fn find_matches(&self, pattern: &RulePattern) -> OwlResult<Vec<HashMap<String, IRI>>> {
        let mut matches = Vec::new();

        // Start with empty bindings
        let mut current_bindings = vec![HashMap::new()];

        for condition in &pattern.conditions {
            let mut new_bindings = Vec::new();

            for binding in current_bindings {
                let condition_matches = self.match_condition(condition, &binding)?;

                for condition_match in condition_matches {
                    let mut combined_binding = binding.clone();
                    combined_binding.extend(condition_match);
                    new_bindings.push(combined_binding);
                }
            }

            current_bindings = new_bindings;

            if current_bindings.is_empty() {
                break; // No matches found
            }
        }

        matches.extend(current_bindings);
        Ok(matches)
    }

    /// Match a single condition against the ontology
    fn match_condition(
        &self,
        condition: &PatternCondition,
        bindings: &HashMap<String, IRI>,
    ) -> OwlResult<Vec<HashMap<String, IRI>>> {
        let mut matches = Vec::new();

        match condition {
            PatternCondition::ClassAssertion { individual, class } => {
                let individual_iri = self.resolve_pattern_var(individual, bindings)?;
                let class_iri = self.resolve_pattern_var(class, bindings)?;

                // Check against ontology class assertions
                for axiom in self.ontology.class_assertions() {
                    let individual_matches =
                        match_individual_iri(&individual_iri, axiom.individual());
                    let class_matches = match_class_expr(&class_iri, axiom.class_expr());

                    if individual_matches && class_matches {
                        let mut binding = HashMap::new();

                        if let PatternVar::Variable(var_name) = individual {
                            binding.insert(var_name.clone(), (**axiom.individual()).clone());
                        }

                        if let PatternVar::Variable(var_name) = class {
                            if let Some(iri) = extract_class_iri(axiom.class_expr()) {
                                binding.insert(var_name.clone(), iri);
                            }
                        }

                        matches.push(binding);
                    }
                }

                // Check against derived facts
                for (derived_individual, derived_class) in &self.derived_facts.class_assertions {
                    let individual_matches =
                        match_individual_iri(&individual_iri, derived_individual);
                    let class_matches = match_iri(&class_iri, derived_class);

                    if individual_matches && class_matches {
                        let mut binding = HashMap::new();

                        if let PatternVar::Variable(var_name) = individual {
                            binding.insert(var_name.clone(), derived_individual.clone());
                        }

                        if let PatternVar::Variable(var_name) = class {
                            binding.insert(var_name.clone(), derived_class.clone());
                        }

                        matches.push(binding);
                    }
                }
            }

            PatternCondition::SubClassOf {
                sub_class,
                super_class,
            } => {
                let sub_iri = self.resolve_pattern_var(sub_class, bindings)?;
                let super_iri = self.resolve_pattern_var(super_class, bindings)?;

                // Check against ontology subclass axioms
                for axiom in self.ontology.subclass_axioms() {
                    if let (
                        ClassExpression::Class(sub_axiom),
                        ClassExpression::Class(super_axiom),
                    ) = (axiom.sub_class(), axiom.super_class())
                    {
                        let sub_matches = match_iri(&sub_iri, sub_axiom.iri());
                        let super_matches = match_iri(&super_iri, super_axiom.iri());

                        if sub_matches && super_matches {
                            let mut binding = HashMap::new();

                            if let PatternVar::Variable(var_name) = sub_class {
                                binding.insert(var_name.clone(), (**sub_axiom.iri()).clone());
                            }

                            if let PatternVar::Variable(var_name) = super_class {
                                binding.insert(var_name.clone(), (**super_axiom.iri()).clone());
                            }

                            matches.push(binding);
                        }
                    }
                }
            }

            // Add more condition matching as needed
            _ => {
                // Other conditions not yet implemented
            }
        }

        Ok(matches)
    }

    /// Apply a rule action with given bindings
    fn apply_action(
        &mut self,
        action: &RuleAction,
        bindings: &HashMap<String, IRI>,
    ) -> OwlResult<usize> {
        let mut new_facts = 0;

        for consequence in &action.consequences {
            if self.apply_consequence(consequence, bindings)? {
                new_facts += 1;
            }
        }

        Ok(new_facts)
    }

    /// Apply a single consequence with given bindings
    fn apply_consequence(
        &mut self,
        consequence: &RuleConsequence,
        bindings: &HashMap<String, IRI>,
    ) -> OwlResult<bool> {
        match consequence {
            RuleConsequence::AddClassAssertion { individual, class } => {
                let individual_iri = self.resolve_pattern_var(individual, bindings)?;
                let class_iri = self.resolve_pattern_var(class, bindings)?;

                if let (PatternVar::Constant(ind_iri), PatternVar::Constant(cls_iri)) =
                    (individual_iri, class_iri)
                {
                    let fact = (ind_iri.clone(), cls_iri.clone());

                    if !self.derived_facts.class_assertions.contains(&fact) {
                        self.derived_facts.class_assertions.insert(fact);
                        return Ok(true);
                    }
                }
            }

            RuleConsequence::AddSubClassOf {
                sub_class,
                super_class,
            } => {
                let sub_iri = self.resolve_pattern_var(sub_class, bindings)?;
                let super_iri = self.resolve_pattern_var(super_class, bindings)?;

                if let (PatternVar::Constant(sub_iri), PatternVar::Constant(super_iri)) =
                    (sub_iri, super_iri)
                {
                    let fact = (sub_iri.clone(), super_iri.clone());

                    if !self.derived_facts.subclass_relationships.contains(&fact) {
                        self.derived_facts.subclass_relationships.insert(fact);
                        return Ok(true);
                    }
                }
            }

            // Add more consequence applications as needed
            _ => {
                // Other consequences not yet implemented
            }
        }

        Ok(false)
    }

    /// Resolve a pattern variable to an IRI using bindings
    fn resolve_pattern_var(
        &self,
        var: &PatternVar,
        bindings: &HashMap<String, IRI>,
    ) -> OwlResult<PatternVar> {
        match var {
            PatternVar::Variable(name) => {
                if let Some(iri) = bindings.get(name) {
                    Ok(PatternVar::Constant(iri.clone()))
                } else {
                    Ok(var.clone())
                }
            }
            PatternVar::Constant(_iri) => Ok(var.clone()),
        }
    }

    /// Get all derived class assertions
    pub fn derived_class_assertions(&self) -> &HashSet<(IRI, IRI)> {
        &self.derived_facts.class_assertions
    }

    /// Get all derived subclass relationships
    pub fn derived_subclass_relationships(&self) -> &HashSet<(IRI, IRI)> {
        &self.derived_facts.subclass_relationships
    }

    /// Get all derived property assertions
    pub fn derived_property_assertions(&self) -> &HashSet<(IRI, IRI, IRI)> {
        &self.derived_facts.property_assertions
    }
}

// Helper functions for pattern matching
fn match_individual_iri(pattern: &PatternVar, individual: &IRI) -> bool {
    match pattern {
        PatternVar::Variable(_) => true,
        PatternVar::Constant(iri) => iri == individual,
    }
}

fn match_class_expr(pattern: &PatternVar, class_expr: &ClassExpression) -> bool {
    match pattern {
        PatternVar::Variable(_) => true,
        PatternVar::Constant(iri) => class_expr.contains_class(iri),
    }
}

fn match_iri(pattern: &PatternVar, iri: &IRI) -> bool {
    match pattern {
        PatternVar::Variable(_) => true,
        PatternVar::Constant(pattern_iri) => pattern_iri == iri,
    }
}

fn extract_class_iri(class_expr: &ClassExpression) -> Option<IRI> {
    match class_expr {
        ClassExpression::Class(class) => Some((**class.iri()).clone()),
        _ => None,
    }
}
