//! Tableaux expansion rules
use super::core::{NodeId, ReasoningRules};
use super::graph::{GraphChangeLog, TableauxGraph};
use super::memory::{MemoryChangeLog, MemoryManager};
use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
use crate::core::entities::Individual;
use crate::core::error::OwlResult;
use smallvec::SmallVec;
use std::collections::VecDeque;

/// Expansion engine for applying tableaux rules
#[derive(Debug)]
pub struct ExpansionEngine {
    rules: Option<ReasoningRules>,
}

impl ExpansionEngine {
    pub fn new() -> Self {
        Self { rules: None }
    }

    /// Set the reasoning rules for this engine
    pub fn with_reasoning_rules(mut self, rules: ReasoningRules) -> Self {
        self.rules = Some(rules);
        self
    }

    /// Expand nodes in the graph
    pub fn expand(
        &mut self,
        graph: &mut TableauxGraph,
        memory_manager: &mut MemoryManager,
        max_depth: u32,
        graph_log: &mut GraphChangeLog,
        memory_log: &mut MemoryChangeLog,
    ) -> OwlResult<()> {
        // Simplified expansion logic
        // In a full implementation, this would:
        // 1. Find nodes to expand
        // 2. Apply expansion rules
        // 3. Update graph and memory
        // 4. Log changes

        let _ = (graph, memory_manager, max_depth, graph_log, memory_log);
        Ok(())
    }

    /// Expand a single node with the given task
    pub fn expand_node(&mut self, node_id: NodeId, _task: ExpansionTask) -> Vec<ExpansionResult> {
        // Simplified expansion - would apply actual tableaux rules
        vec![ExpansionResult::Success { node_id }]
    }
}

impl Default for ExpansionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of an expansion operation
#[derive(Debug, Clone)]
pub enum ExpansionResult {
    /// Expansion succeeded
    Success { node_id: NodeId },
    /// Expansion created new nodes
    NewNodes { parent_id: NodeId, new_nodes: Vec<NodeId> },
    /// Expansion detected a clash (contradiction)
    Clash { node_id: NodeId, reason: String },
}

/// Task for expansion rule application
#[derive(Debug, Clone)]
pub enum ExpansionTask {
    /// Apply conjunction rule (⊓-rule)
    Conjunction {
        node_id: NodeId,
        expressions: SmallVec<[Box<ClassExpression>; 4]>,
    },
    /// Apply disjunction rule (⊔-rule)
    Disjunction {
        node_id: NodeId,
        expressions: SmallVec<[Box<ClassExpression>; 4]>,
    },
    /// Apply existential rule (∃-rule)
    Existential {
        node_id: NodeId,
        property: ObjectPropertyExpression,
        class_expression: Box<ClassExpression>,
    },
    /// Apply universal rule (∀-rule)
    Universal {
        node_id: NodeId,
        property: ObjectPropertyExpression,
        class_expression: Box<ClassExpression>,
    },
    /// Apply nominal rule (O-rule)
    Nominal {
        node_id: NodeId,
        individual: Individual,
    },
    /// Apply cardinality rule
    Cardinality {
        node_id: NodeId,
        property: ObjectPropertyExpression,
        min_cardinality: Option<usize>,
        max_cardinality: Option<usize>,
    },
}

/// Collection of expansion rules for tableaux reasoning
pub struct ExpansionRules {
    rules: Vec<Box<dyn ExpansionRule>>,
}

impl std::fmt::Debug for ExpansionRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpansionRules")
            .field("rule_count", &self.rules.len())
            .finish()
    }
}

impl Default for ExpansionRules {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpansionRules {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: Box<dyn ExpansionRule>) {
        self.rules.push(rule);
    }

    pub fn get_rules(&self) -> &[Box<dyn ExpansionRule>] {
        &self.rules
    }

    /// Create standard SROIQ expansion rules
    pub fn standard_rules() -> Self {
        let mut rules = Self::new();
        rules.add_rule(Box::new(ConjunctionRule));
        rules.add_rule(Box::new(DisjunctionRule));
        rules.add_rule(Box::new(ExistentialRule));
        rules.add_rule(Box::new(UniversalRule));
        rules
    }
}

/// Trait for expansion rules
pub trait ExpansionRule: Send + Sync {
    /// Returns the name of the rule
    fn name(&self) -> &str;
    /// Checks if this rule is applicable to the given node
    fn is_applicable(&self, node_id: NodeId, expression: &ClassExpression) -> bool;
    /// Applies the rule and returns resulting tasks
    fn apply(&self, node_id: NodeId, expression: &ClassExpression) -> Vec<ExpansionTask>;
}

/// Conjunction rule (⊓-rule): If C ⊓ D ∈ L(x), add C and D to L(x)
#[derive(Debug)]
pub struct ConjunctionRule;

impl ExpansionRule for ConjunctionRule {
    fn name(&self) -> &str {
        "Conjunction (⊓-rule)"
    }

    fn is_applicable(&self, _node_id: NodeId, expression: &ClassExpression) -> bool {
        matches!(expression, ClassExpression::ObjectIntersectionOf(_))
    }

    fn apply(&self, node_id: NodeId, expression: &ClassExpression) -> Vec<ExpansionTask> {
        if let ClassExpression::ObjectIntersectionOf(expressions) = expression {
            vec![ExpansionTask::Conjunction {
                node_id,
                expressions: expressions.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Disjunction rule (⊔-rule): If C ⊔ D ∈ L(x), branch with C or D
#[derive(Debug)]
pub struct DisjunctionRule;

impl ExpansionRule for DisjunctionRule {
    fn name(&self) -> &str {
        "Disjunction (⊔-rule)"
    }

    fn is_applicable(&self, _node_id: NodeId, expression: &ClassExpression) -> bool {
        matches!(expression, ClassExpression::ObjectUnionOf(_))
    }

    fn apply(&self, node_id: NodeId, expression: &ClassExpression) -> Vec<ExpansionTask> {
        if let ClassExpression::ObjectUnionOf(expressions) = expression {
            vec![ExpansionTask::Disjunction {
                node_id,
                expressions: expressions.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Existential rule (∃-rule): If ∃R.C ∈ L(x), create new y with R(x,y) and C(y)
#[derive(Debug)]
pub struct ExistentialRule;

impl ExpansionRule for ExistentialRule {
    fn name(&self) -> &str {
        "Existential (∃-rule)"
    }

    fn is_applicable(&self, _node_id: NodeId, expression: &ClassExpression) -> bool {
        matches!(expression, ClassExpression::ObjectSomeValuesFrom(_, _))
    }

    fn apply(&self, node_id: NodeId, expression: &ClassExpression) -> Vec<ExpansionTask> {
        if let ClassExpression::ObjectSomeValuesFrom(property, class_expr) = expression {
            vec![ExpansionTask::Existential {
                node_id,
                property: (**property).clone(),
                class_expression: class_expr.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Universal rule (∀-rule): If ∀R.C ∈ L(x) and R(x,y), add C(y)
#[derive(Debug)]
pub struct UniversalRule;

impl ExpansionRule for UniversalRule {
    fn name(&self) -> &str {
        "Universal (∀-rule)"
    }

    fn is_applicable(&self, _node_id: NodeId, expression: &ClassExpression) -> bool {
        matches!(expression, ClassExpression::ObjectAllValuesFrom(_, _))
    }

    fn apply(&self, node_id: NodeId, expression: &ClassExpression) -> Vec<ExpansionTask> {
        if let ClassExpression::ObjectAllValuesFrom(property, class_expr) = expression {
            vec![ExpansionTask::Universal {
                node_id,
                property: (**property).clone(),
                class_expression: class_expr.clone(),
            }]
        } else {
            Vec::new()
        }
    }
}

/// Rule application tracker
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleApplication {
    pub node_id: NodeId,
    pub rule_name: String,
    pub expression: ClassExpression,
}

impl RuleApplication {
    pub fn new(node_id: NodeId, rule_name: &str, expression: ClassExpression) -> Self {
        Self {
            node_id,
            rule_name: rule_name.to_string(),
            expression,
        }
    }
}

/// Expansion queue for managing pending tasks
#[derive(Debug, Default)]
pub struct ExpansionQueue {
    tasks: VecDeque<ExpansionTask>,
}

impl ExpansionQueue {
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }

    pub fn push(&mut self, task: ExpansionTask) {
        self.tasks.push_back(task);
    }

    pub fn push_front(&mut self, task: ExpansionTask) {
        self.tasks.push_front(task);
    }

    pub fn pop(&mut self) -> Option<ExpansionTask> {
        self.tasks.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn extend(&mut self, tasks: Vec<ExpansionTask>) {
        self.tasks.extend(tasks);
    }
}
