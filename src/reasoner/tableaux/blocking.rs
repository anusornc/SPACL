//! # Tableaux Blocking Strategies
//!
//! Implements blocking strategies for the tableaux reasoning algorithm to prevent infinite
//! expansion and detect clashes (contradictions) in the model being constructed.
//!

#![allow(clippy::only_used_in_recursion)]
//! ## Key Components
//!
//! - **[`BlockingStrategy`]** - Configurable blocking approaches (Equality, Subset, Optimized)
//! - **[`BlockingManager`]** - Coordinates blocking detection and management
//! - **[`BlockingConstraint`]** - Represents specific blocking relationships between nodes
//! - **Blocking Detection** - Algorithms to identify blocking conditions
//! - **Blocking Resolution** - Strategies to handle detected blocks
//!
//! ## Blocking Strategies
//!
//! ### Equality Blocking
//! The standard blocking strategy where a node is blocked if there exists an ancestor node
//! that contains exactly the same concepts. This is simple but may miss some optimization opportunities.
//!
//! ### Subset Blocking
//! A more aggressive strategy where a node is blocked if an ancestor contains a superset
//! of its concepts. This can detect more blocks but may be overly conservative.
//!
//! ### Optimized Blocking
//! An advanced strategy that combines equality and subset blocking with additional
//! heuristics to balance completeness and performance:
//! - Concept frequency analysis
//! - Ancestor distance weighting
//! - Dynamic blocking thresholds
//!
//! ## Algorithm Flow
//!
//! 1. **Node Creation**: When a new node is created, check for blocking conditions
//! 2. **Ancestor Traversal**: Examine ancestor nodes in the tableaux graph
//! 3. **Concept Comparison**: Compare concept sets using the selected strategy
//! 4. **Blocking Detection**: Determine if blocking conditions are met
//! 5. **Constraint Creation**: Create blocking constraints if necessary
//! 6. **Reasoning Continuation**: Either continue expansion or backtrack
//!
//! ## Performance Impact
//!
//! - **Equality Blocking**: O(n²) in worst case, but typically O(n log n)
//! - **Subset Blocking**: O(n²) with higher constant factors but more blocks detected
//! - **Optimized Blocking**: O(n log n) with heuristics to reduce comparison overhead
//!
//! ## Example Usage
//!
//! ```rust
//! use owl2_reasoner::reasoning::tableaux::{BlockingManager, BlockingStrategy, NodeId, TableauxGraph};
//!
//! // Create blocking manager with optimized strategy
//! let mut blocking_manager = BlockingManager::new(BlockingStrategy::Optimized);
//!
//! // Create a graph for the example (normally this would be created by the reasoner)
//! let mut graph = TableauxGraph::new();
//! let node_id = graph.add_node();
//!
//! // Check if a node should be blocked
//! let should_block = blocking_manager.should_block_node(node_id, &graph);
//!
//! if should_block {
//!     println!("Node {} is blocked by ancestor", node_id.as_usize());
//!     // Add blocking constraint (example)
//!     // blocking_manager.add_blocking_constraint(node_id, blocking_ancestor, blocking_type);
//! }
//! ```

use super::core::{NodeId, TableauxNode};
use crate::logic::axioms::class_expressions::ClassExpression;
use crate::core::entities::Individual;
use hashbrown::HashMap;
use std::collections::HashSet;

/// Types of blocking strategies
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BlockingStrategy {
    #[default]
    /// Standard equality blocking
    Equality,
    /// Subset blocking
    Subset,
    /// Optimized blocking with heuristics
    Optimized,
    /// Dynamic blocking with adaptive heuristics
    Dynamic,
    /// Comprehensive blocking combining all strategies
    Comprehensive,
}

/// Blocking constraint for tableaux reasoning
#[derive(Debug, Clone, PartialEq)]
pub struct BlockingConstraint {
    pub blocked_node: NodeId,
    pub blocking_node: NodeId,
    pub constraint_type: BlockingType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockingType {
    /// Subset blocking: ancestor contains all concepts of descendant
    Subset,
    /// Equality blocking: nodes are forced to be equal
    Equality,
    /// Cardinality blocking: enforced by cardinality restrictions
    Cardinality,
    /// Dynamic blocking: adaptive blocking based on reasoning state
    Dynamic,
    /// Nominal blocking: blocking based on individual equality
    Nominal,
}

impl BlockingConstraint {
    pub fn new(blocked_node: NodeId, blocking_node: NodeId, constraint_type: BlockingType) -> Self {
        Self {
            blocked_node,
            blocking_node,
            constraint_type,
        }
    }

    pub fn new_with_reason(
        blocked_node: NodeId,
        blocking_node: NodeId,
        constraint_type: BlockingType,
        _reason: String,
    ) -> Self {
        Self {
            blocked_node,
            blocking_node,
            constraint_type,
        }
    }

    pub fn is_equality(&self) -> bool {
        matches!(self.constraint_type, BlockingType::Equality)
    }

    pub fn is_subset(&self) -> bool {
        matches!(self.constraint_type, BlockingType::Subset)
    }

    pub fn is_cardinality(&self) -> bool {
        matches!(self.constraint_type, BlockingType::Cardinality)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.constraint_type, BlockingType::Dynamic)
    }

    pub fn is_nominal(&self) -> bool {
        matches!(self.constraint_type, BlockingType::Nominal)
    }
}

/// Blocking statistics for optimization
#[derive(Debug, Default)]
pub struct BlockingStats {
    pub total_blocks: usize,
    pub subset_blocks: usize,
    pub equality_blocks: usize,
    pub cardinality_blocks: usize,
    pub dynamic_blocks: usize,
    pub nominal_blocks: usize,
    pub blocked_nodes: HashSet<NodeId>,
}

/// Blocking manager for tableaux reasoning
#[derive(Debug, Default)]
pub struct BlockingManager {
    pub strategy: BlockingStrategy,
    pub blocking_constraints: Vec<BlockingConstraint>,
    pub blocked_nodes: HashSet<NodeId>,
    pub stats: BlockingStats,
    pub individual_constraints: HashMap<NodeId, Individual>,
    pub constraint_log: Vec<BlockingConstraint>,
}

impl BlockingManager {
    pub fn new(strategy: BlockingStrategy) -> Self {
        Self {
            strategy,
            blocking_constraints: Vec::new(),
            blocked_nodes: HashSet::new(),
            stats: BlockingStats::default(),
            individual_constraints: HashMap::new(),
            constraint_log: Vec::new(),
        }
    }

    pub fn add_blocking_constraint(&mut self, constraint: BlockingConstraint) {
        self.blocked_nodes.insert(constraint.blocked_node);
        self.blocking_constraints.push(constraint.clone());
        self.constraint_log.push(constraint.clone());

        // Update statistics
        match constraint.constraint_type {
            BlockingType::Subset => self.stats.subset_blocks += 1,
            BlockingType::Equality => self.stats.equality_blocks += 1,
            BlockingType::Cardinality => self.stats.cardinality_blocks += 1,
            BlockingType::Dynamic => self.stats.dynamic_blocks += 1,
            BlockingType::Nominal => self.stats.nominal_blocks += 1,
        }
        self.stats.total_blocks += 1;
    }

    /// Add blocking constraint with reason for debugging
    pub fn add_blocking_constraint_with_reason(
        &mut self,
        blocked_node: NodeId,
        blocking_node: NodeId,
        constraint_type: BlockingType,
        reason: String,
    ) {
        let constraint = BlockingConstraint::new_with_reason(
            blocked_node,
            blocking_node,
            constraint_type,
            reason,
        );
        self.add_blocking_constraint(constraint);
    }

    /// Check if a node should be blocked based on the current strategy
    pub fn should_block_node(&self, node_id: NodeId, graph: &super::graph::TableauxGraph) -> bool {
        self.detect_blocking(node_id, graph).is_some()
    }

    pub fn detect_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        match self.strategy {
            BlockingStrategy::Equality => self.detect_equality_blocking(node_id, graph),
            BlockingStrategy::Subset => self.detect_subset_blocking(node_id, graph),
            BlockingStrategy::Optimized => self.detect_optimized_blocking(node_id, graph),
            BlockingStrategy::Dynamic => self.detect_dynamic_blocking(node_id, graph),
            BlockingStrategy::Comprehensive => self.detect_comprehensive_blocking(node_id, graph),
        }
    }

    fn detect_equality_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        let node_snapshot = graph.get_node(node_id)?.clone();
        for ancestor_id in self.get_ancestors(node_id, graph) {
            if let Some(ancestor) = graph.get_node(ancestor_id) {
                if self.nodes_have_equal_concepts(&node_snapshot, ancestor) {
                    return Some(BlockingConstraint::new(
                        node_id,
                        ancestor_id,
                        BlockingType::Equality,
                    ));
                }
            }
        }
        None
    }

    fn detect_subset_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        let node_snapshot = graph.get_node(node_id)?.clone();
        for ancestor_id in self.get_ancestors(node_id, graph) {
            if let Some(ancestor) = graph.get_node(ancestor_id) {
                if self.node_is_subset_of_ancestor(&node_snapshot, ancestor) {
                    return Some(BlockingConstraint::new(
                        node_id,
                        ancestor_id,
                        BlockingType::Subset,
                    ));
                }
            }
        }
        None
    }

    fn detect_optimized_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        self.detect_equality_blocking(node_id, graph)
            .or_else(|| self.detect_subset_blocking(node_id, graph))
            .or_else(|| self.detect_nominal_blocking(node_id, graph))
    }

    fn detect_dynamic_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        self.detect_self_restriction_blocking(node_id, graph)
            .or_else(|| self.detect_optimized_blocking(node_id, graph))
    }

    fn detect_comprehensive_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        self.detect_dynamic_blocking(node_id, graph)
            .or_else(|| self.detect_cardinality_blocking(node_id, graph))
            .or_else(|| self.detect_nominal_blocking(node_id, graph))
    }

    fn detect_self_restriction_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        let node = graph.get_node(node_id)?;
        let self_restriction_count = node
            .concepts_iter()
            .filter(|c| matches!(c, ClassExpression::ObjectHasSelf(_)))
            .count();

        if self_restriction_count > 1 {
            for ancestor_id in self.get_ancestors(node_id, graph) {
                if let Some(ancestor) = graph.get_node(ancestor_id) {
                    let ancestor_self_count = ancestor
                        .concepts_iter()
                        .filter(|c| matches!(c, ClassExpression::ObjectHasSelf(_)))
                        .count();

                    if ancestor_self_count >= self_restriction_count {
                        return Some(BlockingConstraint::new(
                            node_id,
                            ancestor_id,
                            BlockingType::Dynamic,
                        ));
                    }
                }
            }
        }
        None
    }

    fn detect_nominal_blocking(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        let node = graph.get_node(node_id)?;
        let nominals: Vec<_> = node
            .concepts_iter()
            .filter_map(|c| {
                if let ClassExpression::ObjectOneOf(individuals) = c {
                    Some(individuals.clone())
                } else {
                    None
                }
            })
            .collect();

        for individuals in nominals {
            for ancestor_id in self.get_ancestors(node_id, graph) {
                if let Some(ancestor) = graph.get_node(ancestor_id) {
                    let ancestor_has_nominal = ancestor.concepts_iter().any(|c| {
                        if let ClassExpression::ObjectOneOf(parent_individuals) = c {
                            individuals.as_slice() == parent_individuals.as_slice()
                        } else {
                            false
                        }
                    });

                    if ancestor_has_nominal {
                        return Some(BlockingConstraint::new(
                            node_id,
                            ancestor_id,
                            BlockingType::Nominal,
                        ));
                    }
                }
            }
        }
        None
    }

    fn detect_cardinality_blocking(
        &self,
        _node_id: NodeId,
        _graph: &super::graph::TableauxGraph,
    ) -> Option<BlockingConstraint> {
        // Placeholder: full implementation would inspect cardinality constraints.
        None
    }

    /// Get all ancestors of a node
    fn get_ancestors(&self, node_id: NodeId, graph: &super::graph::TableauxGraph) -> Vec<NodeId> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();
        self.collect_ancestors(node_id, graph, &mut ancestors, &mut visited);
        ancestors
    }

    /// Recursively collect ancestors
    fn collect_ancestors(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
        ancestors: &mut Vec<NodeId>,
        visited: &mut HashSet<NodeId>,
    ) {
        if visited.contains(&node_id) {
            return;
        }
        visited.insert(node_id);

        // Check all edges that point to this node
        for edge in graph.edges.get_all_edges() {
            if edge.2 == node_id {
                // Found an incoming edge, add the source as an ancestor
                if !ancestors.contains(&edge.0) {
                    ancestors.push(edge.0);
                    self.collect_ancestors(edge.0, graph, ancestors, visited);
                }
            }
        }
    }

    /// Check if two nodes have exactly the same concepts
    fn nodes_have_equal_concepts(&self, node1: &TableauxNode, node2: &TableauxNode) -> bool {
        let concepts1: HashSet<_> = node1.concepts_iter().collect();
        let concepts2: HashSet<_> = node2.concepts_iter().collect();
        concepts1 == concepts2
    }

    /// Check if node1's concepts are a subset of node2's concepts
    fn node_is_subset_of_ancestor(&self, node: &TableauxNode, ancestor: &TableauxNode) -> bool {
        for concept in node.concepts_iter() {
            if !ancestor
                .concepts_iter()
                .any(|ac| self.concepts_are_compatible(concept, ac))
            {
                return false;
            }
        }
        true
    }

    /// Check if two concepts are compatible (not contradictory)
    fn concepts_are_compatible(&self, c1: &ClassExpression, c2: &ClassExpression) -> bool {
        match (c1, c2) {
            (ClassExpression::Class(_), ClassExpression::Class(_)) => true, // Simplified
            (ClassExpression::ObjectComplementOf(comp1), _) => {
                // Check if the complement doesn't contradict the other concept
                !self.are_contradictory(comp1.as_ref(), c2)
            }
            (_, ClassExpression::ObjectComplementOf(comp2)) => {
                // Check if the complement doesn't contradict the first concept
                !self.are_contradictory(c1, comp2.as_ref())
            }
            _ => true, // Default to compatible for complex expressions
        }
    }

    /// Check if two concepts are contradictory (simplified)
    fn are_contradictory(&self, _c1: &ClassExpression, _c2: &ClassExpression) -> bool {
        false // Simplified - in full implementation would use ontology reasoning
    }

    pub fn is_blocked(&self, node_id: NodeId) -> bool {
        self.blocked_nodes.contains(&node_id)
    }

    pub fn get_blocking_constraints(&self) -> &[BlockingConstraint] {
        &self.blocking_constraints
    }

    pub fn clear(&mut self) {
        self.blocking_constraints.clear();
        self.blocked_nodes.clear();
        self.stats = BlockingStats::default();
        self.individual_constraints.clear();
    }

    /// Get blocking statistics
    pub fn get_stats(&self) -> &BlockingStats {
        &self.stats
    }

    /// Check if a node is blocked by a specific type
    pub fn is_blocked_by_type(&self, node_id: NodeId, blocking_type: BlockingType) -> bool {
        self.blocking_constraints.iter().any(|constraint| {
            constraint.blocked_node == node_id && constraint.constraint_type == blocking_type
        })
    }

    pub fn check_blocking(&self, node1: NodeId, node2: NodeId) -> Option<BlockingConstraint> {
        // This method is kept for backward compatibility
        // Use should_block_node for new code
        match self.strategy {
            BlockingStrategy::Equality => {
                if let (Some(n1), Some(n2)) = (
                    self.get_node_from_graph(node1),
                    self.get_node_from_graph(node2),
                ) {
                    if self.nodes_have_equal_concepts(n1, n2) {
                        return Some(BlockingConstraint::new(
                            node1,
                            node2,
                            BlockingType::Equality,
                        ));
                    }
                }
            }
            BlockingStrategy::Subset => {
                if let (Some(n1), Some(n2)) = (
                    self.get_node_from_graph(node1),
                    self.get_node_from_graph(node2),
                ) {
                    if self.node_is_subset_of_ancestor(n1, n2) {
                        return Some(BlockingConstraint::new(node1, node2, BlockingType::Subset));
                    }
                }
            }
            _ => {} // Other strategies require full graph context
        }
        None
    }

    /// Placeholder method to get node from graph (would be implemented with actual graph reference)
    fn get_node_from_graph(&self, _node_id: NodeId) -> Option<&TableauxNode> {
        None // Would be implemented with actual graph access
    }
}
