//! Ontology Structure Analysis
//!
//! Analyzes ontology characteristics to determine optimal reasoning strategy.
//! Used to select between fast path (hierarchical) and full reasoning.

use crate::core::ontology::Ontology;
use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::Axiom;

/// Characteristics of an ontology that determine reasoning complexity
#[derive(Debug, Clone)]
pub struct OntologyCharacteristics {
    /// Number of classes in the ontology
    pub class_count: usize,

    /// Number of object properties
    pub property_count: usize,

    /// Number of individuals (ABox size)
    pub individual_count: usize,

    /// Number of disjunctions (ObjectUnionOf)
    pub disjunction_count: usize,

    /// Number of complex class expressions (existentials, universals)
    pub complex_expression_count: usize,

    /// Number of disjointness axioms
    pub disjointness_axiom_count: usize,

    /// Number of equivalence axioms
    pub equivalence_axiom_count: usize,

    /// Maximum nesting depth of class expressions
    pub max_expression_depth: usize,

    /// Estimated hierarchy depth
    pub hierarchy_depth: usize,

    /// Whether the hierarchy is tree-like (no multiple inheritance)
    pub is_tree_like: bool,

    /// Complexity score from 0.0 (simple) to 1.0 (highly complex)
    pub complexity_score: f64,

    /// Recommended strategy for this ontology
    pub recommended_strategy: ReasoningStrategy,
}

/// Recommended reasoning strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningStrategy {
    /// Simple hierarchical classification (fastest)
    Hierarchical,

    /// Batch classification with early termination
    BatchIncremental,

    /// Full tableaux with speculative parallelism
    SpeculativeParallel,

    /// Conservative sequential for debugging
    Sequential,
}

impl OntologyCharacteristics {
    /// Analyze an ontology and compute its characteristics
    pub fn analyze(ontology: &Ontology) -> Self {
        let class_count = ontology.classes().len();
        let property_count = ontology.object_properties().len();
        let individual_count = ontology.named_individuals().len();

        let mut disjunction_count = 0;
        let mut complex_expression_count = 0;
        let mut disjointness_axiom_count = 0;
        let mut equivalence_axiom_count = 0;
        let mut max_expression_depth = 0;

        // Analyze axioms
        for axiom in ontology.axioms() {
            match axiom.as_ref() {
                Axiom::DisjointClasses(_) => {
                    disjointness_axiom_count += 1;
                }
                Axiom::EquivalentClasses(_) => {
                    equivalence_axiom_count += 1;
                }
                Axiom::SubClassOf(sub) => {
                    // Check for disjunctions and complex expressions
                    disjunction_count += count_disjunctions(sub.super_class());
                    complex_expression_count += count_complex_expressions(sub.super_class());
                    max_expression_depth =
                        max_expression_depth.max(expression_depth(sub.super_class()));
                }
                _ => {}
            }
        }

        // Estimate hierarchy depth and tree-likeness
        let (hierarchy_depth, is_tree_like) = analyze_hierarchy_structure(ontology);

        // Calculate complexity score
        let complexity_score = calculate_complexity_score(
            class_count,
            disjunction_count,
            complex_expression_count,
            disjointness_axiom_count,
            max_expression_depth,
        );

        // Determine recommended strategy
        let recommended_strategy = select_strategy(
            complexity_score,
            disjunction_count,
            class_count,
            is_tree_like,
        );

        Self {
            class_count,
            property_count,
            individual_count,
            disjunction_count,
            complex_expression_count,
            disjointness_axiom_count,
            equivalence_axiom_count,
            max_expression_depth,
            hierarchy_depth,
            is_tree_like,
            complexity_score,
            recommended_strategy,
        }
    }

    /// Check if this ontology can use the fast hierarchical path
    pub fn can_use_fast_path(&self) -> bool {
        self.recommended_strategy == ReasoningStrategy::Hierarchical
    }

    /// Check if this ontology is small enough for simple sequential processing
    pub fn is_small(&self) -> bool {
        self.class_count < 100 && self.disjunction_count < 10
    }

    /// Get estimated classification time in milliseconds (rough heuristic)
    pub fn estimated_classification_time_ms(&self) -> u64 {
        let base_time = match self.class_count {
            0..=100 => 10,
            101..=1000 => 100,
            1001..=10000 => 1000,
            10001..=50000 => 5000,
            _ => 10000,
        };

        let complexity_multiplier = 1.0 + self.complexity_score * 10.0;

        (base_time as f64 * complexity_multiplier) as u64
    }

    /// Get a human-readable complexity description
    pub fn complexity_description(&self) -> &'static str {
        match self.complexity_score {
            s if s < 0.1 => "Very Simple",
            s if s < 0.3 => "Simple",
            s if s < 0.5 => "Moderate",
            s if s < 0.7 => "Complex",
            s if s < 0.9 => "Very Complex",
            _ => "Extremely Complex",
        }
    }
}

/// Count disjunctions (ObjectUnionOf) in a class expression
fn count_disjunctions(expr: &ClassExpression) -> usize {
    match expr {
        ClassExpression::ObjectUnionOf(operands) => {
            1 + operands
                .iter()
                .map(|op| count_disjunctions(op))
                .sum::<usize>()
        }
        ClassExpression::ObjectIntersectionOf(operands) => operands
            .iter()
            .map(|op| count_disjunctions(op))
            .sum::<usize>(),
        ClassExpression::ObjectComplementOf(inner) => count_disjunctions(inner),
        ClassExpression::ObjectSomeValuesFrom(_, inner) => count_disjunctions(inner),
        ClassExpression::ObjectAllValuesFrom(_, inner) => count_disjunctions(inner),
        _ => 0,
    }
}

/// Count complex expressions (existentials, universals, complements)
fn count_complex_expressions(expr: &ClassExpression) -> usize {
    match expr {
        ClassExpression::ObjectUnionOf(operands) => operands
            .iter()
            .map(|op| count_complex_expressions(op))
            .sum::<usize>(),
        ClassExpression::ObjectIntersectionOf(operands) => operands
            .iter()
            .map(|op| count_complex_expressions(op))
            .sum::<usize>(),
        ClassExpression::ObjectComplementOf(_) => 1,
        ClassExpression::ObjectSomeValuesFrom(_, inner) => 1 + count_complex_expressions(inner),
        ClassExpression::ObjectAllValuesFrom(_, inner) => 1 + count_complex_expressions(inner),
        _ => 0,
    }
}

/// Calculate the maximum depth of a class expression
fn expression_depth(expr: &ClassExpression) -> usize {
    match expr {
        ClassExpression::ObjectUnionOf(operands) => {
            1 + operands
                .iter()
                .map(|op| expression_depth(op))
                .max()
                .unwrap_or(0)
        }
        ClassExpression::ObjectIntersectionOf(operands) => {
            1 + operands
                .iter()
                .map(|op| expression_depth(op))
                .max()
                .unwrap_or(0)
        }
        ClassExpression::ObjectComplementOf(inner) => 1 + expression_depth(inner),
        ClassExpression::ObjectSomeValuesFrom(_, inner) => 1 + expression_depth(inner),
        ClassExpression::ObjectAllValuesFrom(_, inner) => 1 + expression_depth(inner),
        _ => 1,
    }
}

/// Analyze hierarchy structure to estimate depth and tree-likeness
fn analyze_hierarchy_structure(ontology: &Ontology) -> (usize, bool) {
    use std::collections::{HashMap, HashSet, VecDeque};

    // Build parent-child maps
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut class_set: HashSet<String> = HashSet::new();

    for axiom in ontology.subclass_axioms() {
        if let (ClassExpression::Class(sub_class), ClassExpression::Class(super_class)) =
            (axiom.sub_class(), axiom.super_class())
        {
            let sub_iri = sub_class.iri().to_string();
            let super_iri = super_class.iri().to_string();

            class_set.insert(sub_iri.clone());
            class_set.insert(super_iri.clone());

            parents
                .entry(sub_iri.clone())
                .or_default()
                .push(super_iri.clone());
            children.entry(super_iri).or_default().push(sub_iri);
        }
    }

    // Check tree-likeness: count classes with multiple parents
    let mut multiple_parents = 0;
    for (_, parent_list) in &parents {
        if parent_list.len() > 1 {
            multiple_parents += 1;
        }
    }

    let is_tree_like = if class_set.is_empty() {
        true
    } else {
        (multiple_parents as f64 / class_set.len() as f64) < 0.1
    };

    // Estimate max depth using an iterative longest-path pass over the hierarchy graph.
    // This avoids deep recursion on long synthetic chains (e.g., hierarchy_100000.owl).
    let mut indegree: HashMap<String, usize> = HashMap::with_capacity(class_set.len());
    for class in &class_set {
        let parent_count = parents.get(class).map(|p| p.len()).unwrap_or(0);
        indegree.insert(class.clone(), parent_count);
    }

    let mut depth: HashMap<String, usize> = HashMap::with_capacity(class_set.len());
    let mut queue: VecDeque<String> = VecDeque::new();

    for class in &class_set {
        if indegree.get(class).copied().unwrap_or(0) == 0 {
            depth.insert(class.clone(), 1);
            queue.push_back(class.clone());
        }
    }

    // Fallback for cyclic/no-root graphs: seed all classes at depth 1.
    if queue.is_empty() {
        for class in &class_set {
            depth.insert(class.clone(), 1);
            queue.push_back(class.clone());
        }
    }

    while let Some(node) = queue.pop_front() {
        let current_depth = depth.get(&node).copied().unwrap_or(1);
        if let Some(child_list) = children.get(&node) {
            for child in child_list {
                let candidate = current_depth + 1;
                let entry = depth.entry(child.clone()).or_insert(1);
                if candidate > *entry {
                    *entry = candidate;
                }

                if let Some(ind) = indegree.get_mut(child) {
                    if *ind > 0 {
                        *ind -= 1;
                        if *ind == 0 {
                            queue.push_back(child.clone());
                        }
                    }
                }
            }
        }
    }

    let max_depth = depth.values().copied().max().unwrap_or(1);

    (max_depth, is_tree_like)
}

/// Calculate overall complexity score
fn calculate_complexity_score(
    class_count: usize,
    disjunction_count: usize,
    complex_expression_count: usize,
    disjointness_count: usize,
    max_depth: usize,
) -> f64 {
    // Normalize each factor
    let class_factor = (class_count as f64 / 10000.0).min(1.0);
    let disjunction_factor = if class_count > 0 {
        (disjunction_count as f64 / class_count as f64 * 10.0).min(1.0)
    } else {
        0.0
    };
    let complex_factor = if class_count > 0 {
        (complex_expression_count as f64 / class_count as f64).min(1.0)
    } else {
        0.0
    };
    let disjointness_factor = if class_count > 0 {
        (disjointness_count as f64 / class_count as f64 * 5.0).min(1.0)
    } else {
        0.0
    };
    let depth_factor = (max_depth as f64 / 10.0).min(1.0);

    // Weighted average
    let score = class_factor * 0.2
        + disjunction_factor * 0.3
        + complex_factor * 0.2
        + disjointness_factor * 0.15
        + depth_factor * 0.15;

    score.min(1.0)
}

/// Select the best reasoning strategy based on characteristics
fn select_strategy(
    complexity_score: f64,
    disjunction_count: usize,
    class_count: usize,
    is_tree_like: bool,
) -> ReasoningStrategy {
    if complexity_score < 0.1 && disjunction_count == 0 && is_tree_like {
        // Very simple hierarchical ontology
        ReasoningStrategy::Hierarchical
    } else if class_count > 10000 || complexity_score > 0.5 {
        // Large or complex ontology - use batch processing
        ReasoningStrategy::BatchIncremental
    } else if disjunction_count > 100 {
        // Many disjunctions - speculative parallelism helps
        ReasoningStrategy::SpeculativeParallel
    } else {
        // Default to batch incremental for safety
        ReasoningStrategy::BatchIncremental
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::Class;
    use crate::core::ontology::Ontology;
    use crate::logic::axioms::{ClassExpression, SubClassOfAxiom};

    #[test]
    fn test_simple_hierarchy() {
        let mut ontology = Ontology::new();

        // Create simple hierarchy: A ⊑ B ⊑ C
        let class_a = Class::new("http://example.org/A");
        let class_b = Class::new("http://example.org/B");
        let class_c = Class::new("http://example.org/C");

        ontology.add_class(class_a.clone()).unwrap();
        ontology.add_class(class_b.clone()).unwrap();
        ontology.add_class(class_c.clone()).unwrap();

        let axiom1 = SubClassOfAxiom::new(
            ClassExpression::Class(class_a.clone()),
            ClassExpression::Class(class_b.clone()),
        );
        let axiom2 = SubClassOfAxiom::new(
            ClassExpression::Class(class_b.clone()),
            ClassExpression::Class(class_c.clone()),
        );

        ontology.add_subclass_axiom(axiom1).unwrap();
        ontology.add_subclass_axiom(axiom2).unwrap();

        let chars = OntologyCharacteristics::analyze(&ontology);

        assert_eq!(chars.class_count, 3);
        assert_eq!(chars.disjunction_count, 0);
        assert!(chars.can_use_fast_path());
        assert!(chars.is_tree_like);
    }

    #[test]
    fn test_many_classes_no_hierarchy() {
        let mut ontology = Ontology::new();

        // Add many classes without hierarchy
        for i in 0..100 {
            let class = Class::new(format!("http://example.org/Class{}", i).as_str());
            ontology.add_class(class).unwrap();
        }

        let chars = OntologyCharacteristics::analyze(&ontology);

        assert_eq!(chars.class_count, 100);
        // No hierarchy = no multiple inheritance = technically tree-like (forest)
        assert!(chars.is_tree_like);
        // Can use fast path because no complex axioms
        assert!(chars.can_use_fast_path());
    }

    #[test]
    fn test_deep_hierarchy_no_stack_overflow() {
        let mut ontology = Ontology::new();

        // Deep linear chain to guard against recursive depth overflow.
        let n = 8000usize;
        for i in 0..=n {
            let class = Class::new(format!("http://example.org/C{}", i).as_str());
            ontology.add_class(class).unwrap();
        }

        for i in 0..n {
            let sub = Class::new(format!("http://example.org/C{}", i).as_str());
            let sup = Class::new(format!("http://example.org/C{}", i + 1).as_str());
            ontology
                .add_subclass_axiom(SubClassOfAxiom::new(
                    ClassExpression::Class(sub),
                    ClassExpression::Class(sup),
                ))
                .unwrap();
        }

        let chars = OntologyCharacteristics::analyze(&ontology);
        assert!(chars.hierarchy_depth >= n);
    }
}
