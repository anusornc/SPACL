//! # Dependency-Directed Backtracking
//!
//! Implements dependency management for efficient backtracking in the tableaux reasoning algorithm.
//! This module tracks relationships between reasoning decisions and enables intelligent backtracking
//! when contradictions are discovered.
//!
//! ## Key Components
//!
//! - **[`DependencyManager`]** - Central coordinator for dependency tracking
//! - **[`Dependency`]** - Represents relationships between nodes and choices
//! - **[`ChoicePoint`]** - Records branching points in the reasoning process
//! - **[`DependencySource`]** - Types of dependency sources (ChoicePoint, Node, GlobalConstraint)
//! - **[`DependencyType`]** - Categories of dependencies (Subclass, Property, Disjointness, etc.)
//!
//! ## Dependency-Directed Backtracking
//!
//! Unlike naive backtracking that explores all possibilities, dependency-directed backtracking:
//!
//! 1. **Track Dependencies**: Record which reasoning steps depend on which choices
//! 2. **Identify Contradictions**: When a clash is found, trace back to responsible choices
//! 3. **Smart Backtracking**: Jump directly to the choice that caused the contradiction
//! 4. **Avoid Redundant Work**: Skip exploration of paths that would lead to the same contradiction
//!
//! ## Dependency Types
//!
//! - **Subclass**: Dependencies from subclass reasoning steps
//! - **Property**: Dependencies from property axioms and restrictions
//! - **Disjointness**: Dependencies from disjoint class axioms
//! - **Existential**: Dependencies from existential restrictions
//! - **Universal**: Dependencies from universal restrictions
//! - **Nominal**: Dependencies from individual assertions
//!
//! ## Performance Benefits
//!
//! - **Reduced Backtracking**: Skip irrelevant branches
//! - **Faster Contradiction Detection**: Direct tracing to source
//! - **Memory Efficiency**: Only track necessary dependencies
//! - **Scalability**: Better performance on complex ontologies
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use owl2_reasoner::reasoning::tableaux::{DependencyManager, Dependency, DependencyType, NodeId, ChoiceType};
//!
//! // Create dependency manager
//! let mut dependency_manager = DependencyManager::new();
//!
//! // Create a choice point (branching decision)
//! let choice_point_id = dependency_manager.create_choice_point(
//!     NodeId::new(1),
//!     ChoiceType::NonDeterministic
//! );
//!
//! // Add dependencies for reasoning steps
//! dependency_manager.add_dependency(NodeId::new(2));
//!
//! // When a contradiction is found, backtrack intelligently
//! let contradiction_detected = true; // Example condition
//! if contradiction_detected {
//!     if let Some(backtrack_to) = dependency_manager.find_backtrack_point(NodeId::new(2)) {
//!         println!("Backtrack to choice point {}", backtrack_to);
//!     }
//! }
//! ```

use super::core::NodeId;
use super::expansion::ExpansionTask;
use super::graph::GraphChangeLog;
use super::memory::MemoryChangeLog;
use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
use crate::core::entities::{Class, Individual};
use crate::core::error::OwlResult;
use hashbrown::HashMap;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Dependency between tableaux nodes and choices
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The node that created this dependency
    pub source_node: NodeId,
    /// The reasoning choice that led to this dependency
    pub choice: ReasoningChoice,
    /// Nodes that depend on this choice
    pub dependent_nodes: Vec<NodeId>,
    /// The level at which this dependency was created
    pub level: usize,
    /// Whether this dependency has been resolved
    pub resolved: bool,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum ReasoningChoice {
    /// Choice rule application: which rule to apply
    RuleApplication {
        concept: ClassExpression,
        node_id: NodeId,
        rule_applied: String,
    },
    /// Non-deterministic choice: which branch to explore
    BranchChoice {
        node_id: NodeId,
        branch_options: Vec<ClassExpression>,
        chosen_branch: usize,
    },
    /// Individual selection for nominal reasoning
    IndividualSelection {
        nominal_node: NodeId,
        selected_individual: Individual,
        available_individuals: Vec<Individual>,
    },
    /// Cardinality constraint handling
    CardinalityHandling {
        node_id: NodeId,
        property: ObjectPropertyExpression,
        min_cardinality: Option<usize>,
        max_cardinality: Option<usize>,
        created_fillers: Vec<NodeId>,
    },
}

/// Backtracking decision point
#[derive(Debug, Clone)]
pub struct BacktrackPoint {
    /// The node where the decision was made
    pub node_id: NodeId,
    /// The choice that was made
    pub choice: ReasoningChoice,
    /// Dependencies created by this choice
    pub dependencies: Vec<Dependency>,
    /// Alternative choices that could have been made
    pub alternatives: Vec<ReasoningChoice>,
    /// The reasoning level/depth
    pub level: usize,
    /// Whether this point has been fully explored
    pub exhausted: bool,
}

/// Backtracking statistics
#[derive(Debug, Default)]
pub struct BacktrackStats {
    pub total_backtracks: usize,
    pub dependency_directed_backtracks: usize,
    pub naive_backtracks: usize,
    pub choices_explored: usize,
    pub contradictions_detected: usize,
    pub _average_backtrack_depth: f64,
}

/// Identifier for an active tableaux branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(usize);

impl BranchId {
    pub fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Runtime state associated with a branch under exploration.
#[allow(dead_code)]
#[derive(Debug)]
pub struct BranchState {
    pub id: BranchId,
    pub change_log: GraphChangeLog,
    pub memory_log: MemoryChangeLog,
    pub open_tasks: VecDeque<ExpansionTask>,
    pub blocked_nodes: HashSet<NodeId>,
    pub depth: usize,
    pub start_time: Instant,
}

impl BranchState {
    pub fn new(id: BranchId, tasks: VecDeque<ExpansionTask>) -> Self {
        Self {
            id,
            change_log: GraphChangeLog::new(),
            memory_log: MemoryChangeLog::new(),
            open_tasks: tasks,
            blocked_nodes: HashSet::new(),
            depth: 0,
            start_time: Instant::now(),
        }
    }

    pub fn push_tasks(&mut self, tasks: Vec<ExpansionTask>) {
        self.open_tasks.extend(tasks);
    }

    pub fn take_next_task(&mut self) -> Option<ExpansionTask> {
        self.open_tasks.pop_front()
    }

    pub fn record_changes(&mut self, log: GraphChangeLog) {
        self.change_log.extend(log);
    }

    pub fn record_memory(&mut self, log: MemoryChangeLog) {
        self.memory_log.extend(log);
    }

    pub fn reset_timer(&mut self) {
        self.start_time = Instant::now();
    }
}

/// Scheduler for active branches (placeholder until full integration).
#[allow(dead_code)]
#[derive(Debug)]
pub struct BranchManager {
    branches: HashMap<BranchId, BranchState>,
    queue: VecDeque<BranchId>,
    next_id: usize,
    timeout: Option<Duration>,
}

impl BranchManager {
    pub fn new(timeout: Option<Duration>) -> Self {
        Self {
            branches: HashMap::new(),
            queue: VecDeque::new(),
            next_id: 0,
            timeout,
        }
    }

    fn allocate_id(&mut self) -> BranchId {
        let id = BranchId::new(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn create_root_branch(&mut self, tasks: VecDeque<ExpansionTask>) -> BranchId {
        let id = self.allocate_id();
        let state = BranchState::new(id, tasks);
        self.queue.push_back(id);
        self.branches.insert(id, state);
        id
    }

    pub fn schedule_branch(&mut self, state: BranchState) {
        let id = state.id;
        self.queue.push_back(id);
        self.branches.insert(id, state);
    }

    pub fn next_branch(&mut self) -> Option<BranchId> {
        self.queue.pop_front()
    }

    pub fn get_branch(&self, id: BranchId) -> Option<&BranchState> {
        self.branches.get(&id)
    }

    pub fn get_branch_mut(&mut self, id: BranchId) -> Option<&mut BranchState> {
        self.branches.get_mut(&id)
    }

    pub fn remove_branch(&mut self, id: BranchId) -> Option<BranchState> {
        self.branches.remove(&id)
    }

    pub fn branch_timeout(&self, state: &BranchState) -> bool {
        if let Some(limit) = self.timeout {
            state.start_time.elapsed() >= limit
        } else {
            false
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum DependencySource {
    /// Choice point in the reasoning process
    ChoicePoint(usize),
    /// Another node
    Node(NodeId),
    /// Global constraint
    GlobalConstraint,
    /// Advanced reasoning choice
    ReasoningChoice(ReasoningChoice),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyType {
    /// Subclass dependency
    Subclass,
    /// Property dependency
    Property,
    /// Individual dependency
    Individual,
    /// Concept dependency
    Concept,
    /// Rule application dependency
    RuleApplication,
    /// Branch choice dependency
    BranchChoice,
    /// Cardinality dependency
    Cardinality,
}

impl Dependency {
    pub fn new(
        dependent_node: NodeId,
        dependency_source: DependencySource,
        _dependency_type: DependencyType,
    ) -> Self {
        Self {
            source_node: match dependency_source {
                DependencySource::Node(node_id) => node_id,
                DependencySource::ChoicePoint(_) => NodeId::new(0), // Placeholder
                DependencySource::GlobalConstraint => NodeId::new(0), // Placeholder
                DependencySource::ReasoningChoice(ref choice) => match choice {
                    ReasoningChoice::RuleApplication { node_id, .. } => *node_id,
                    ReasoningChoice::BranchChoice { node_id, .. } => *node_id,
                    ReasoningChoice::IndividualSelection { nominal_node, .. } => *nominal_node,
                    ReasoningChoice::CardinalityHandling { node_id, .. } => *node_id,
                },
            },
            choice: match dependency_source {
                DependencySource::ReasoningChoice(choice) => choice,
                _ => ReasoningChoice::RuleApplication {
                    concept: ClassExpression::Class(Class::new(
                        "http://www.w3.org/2002/07/owl#Thing",
                    )),
                    node_id: dependent_node,
                    rule_applied: "unknown".to_string(),
                },
            },
            dependent_nodes: vec![dependent_node],
            level: 0,
            resolved: false,
        }
    }

    /// Create a dependency from a reasoning choice
    pub fn from_reasoning_choice(
        source_node: NodeId,
        choice: ReasoningChoice,
        dependent_nodes: Vec<NodeId>,
        level: usize,
    ) -> Self {
        Self {
            source_node,
            choice,
            dependent_nodes,
            level,
            resolved: false,
        }
    }
}

/// Choice point for backtracking
#[derive(Debug, Clone)]
pub struct ChoicePoint {
    pub id: usize,
    pub node_id: NodeId,
    pub choice_type: ChoiceType,
    pub dependencies: HashSet<NodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChoiceType {
    /// Disjunction choice
    Disjunction,
    /// existential restriction choice
    ExistentialRestriction,
    /// nominal choice
    Nominal,
    /// data range choice
    DataRange,
}

impl ChoicePoint {
    pub fn new(id: usize, node_id: NodeId, choice_type: ChoiceType) -> Self {
        Self {
            id,
            node_id,
            choice_type,
            dependencies: HashSet::new(),
        }
    }

    pub fn add_dependency(&mut self, node_id: NodeId) {
        self.dependencies.insert(node_id);
    }
}

/// Dependency manager for backtracking and dependency-directed reasoning
#[derive(Debug)]
pub struct DependencyManager {
    pub dependencies: HashMap<NodeId, Vec<Dependency>>,
    pub choice_points: Vec<ChoicePoint>,
    pub next_choice_id: usize,
    pub dependency_graph: HashMap<NodeId, HashSet<NodeId>>,
    /// Advanced backtracking features
    pub backtrack_stack: Vec<BacktrackPoint>,
    pub node_dependencies: HashMap<NodeId, Vec<Dependency>>,
    pub contradictory_choices: HashSet<ReasoningChoice>,
    pub current_level: usize,
    pub stats: BacktrackStats,
}

impl DependencyManager {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            choice_points: Vec::new(),
            next_choice_id: 0,
            dependency_graph: HashMap::new(),
            backtrack_stack: Vec::new(),
            node_dependencies: HashMap::new(),
            contradictory_choices: HashSet::new(),
            current_level: 0,
            stats: BacktrackStats::default(),
        }
    }

    pub fn add_dependency(&mut self, dependency: Dependency) {
        let dependent_node = dependency
            .dependent_nodes
            .first()
            .copied()
            .unwrap_or(NodeId::new(0));
        self.dependencies
            .entry(dependent_node)
            .or_default()
            .push(dependency.clone());

        // Also add to node dependencies for advanced backtracking
        self.node_dependencies
            .entry(dependent_node)
            .or_default()
            .push(dependency.clone());

        // Update dependency graph
        let source_node = dependency.source_node;
        for &dep_node in &dependency.dependent_nodes {
            self.dependency_graph
                .entry(source_node)
                .or_default()
                .insert(dep_node);
        }
    }

    /// Add a dependency created by the current choice
    pub fn add_dependency_from_choice(
        &mut self,
        dependent_node: NodeId,
        source_node: NodeId,
        choice: &ReasoningChoice,
    ) {
        if let Some(current_point) = self.backtrack_stack.last_mut() {
            let dependency = Dependency {
                source_node,
                choice: choice.clone(),
                dependent_nodes: vec![dependent_node],
                level: self.current_level - 1,
                resolved: false,
            };

            current_point.dependencies.push(dependency.clone());

            // Add to node dependency index
            self.node_dependencies
                .entry(dependent_node)
                .or_default()
                .push(dependency);
        }
    }

    /// Push a new reasoning choice onto the stack
    pub fn push_choice(
        &mut self,
        node_id: NodeId,
        choice: ReasoningChoice,
        alternatives: Vec<ReasoningChoice>,
    ) {
        let backtrack_point = BacktrackPoint {
            node_id,
            choice: choice.clone(),
            dependencies: Vec::new(),
            alternatives,
            level: self.current_level,
            exhausted: false,
        };

        self.backtrack_stack.push(backtrack_point);
        self.current_level += 1;
        self.stats.choices_explored += 1;
    }

    /// Mark a choice as contradictory
    pub fn mark_contradictory(&mut self, choice: &ReasoningChoice) {
        self.contradictory_choices.insert(choice.clone());
        self.stats.contradictions_detected += 1;
    }

    /// Find the best backtrack point based on dependencies
    pub fn find_backtrack_point(&mut self, contradiction_node: NodeId) -> Option<usize> {
        // First, try dependency-directed backtracking
        if let Some(dependencies) = self.node_dependencies.get(&contradiction_node) {
            for dependency in dependencies {
                // Find the backtrack point that created this dependency
                for (i, point) in self.backtrack_stack.iter().enumerate().rev() {
                    if point.choice == dependency.choice && !point.exhausted {
                        // Check if there are unexplored alternatives
                        if !point.alternatives.is_empty() {
                            self.stats.dependency_directed_backtracks += 1;
                            return Some(i);
                        }
                    }
                }
            }
        }

        // Fall back to naive backtracking (most recent choice with alternatives)
        for (i, point) in self.backtrack_stack.iter().enumerate().rev() {
            if !point.exhausted && !point.alternatives.is_empty() {
                self.stats.naive_backtracks += 1;
                return Some(i);
            }
        }

        None
    }

    /// Execute backtracking to a specific point
    pub fn backtrack_to_level(&mut self, target_level: usize) -> OwlResult<()> {
        // Remove all choice points after the specified level
        self.backtrack_stack
            .retain(|point| point.level <= target_level);

        // Remove dependencies after target level
        self.node_dependencies
            .retain(|_, dependencies| dependencies.iter().any(|dep| dep.level <= target_level));

        // Mark backtrack points as exhausted up to target level
        for point in &mut self.backtrack_stack {
            if point.level > target_level {
                point.exhausted = true;
            }
        }

        self.current_level = target_level;
        self.stats.total_backtracks += 1;

        Ok(())
    }

    /// Check if a choice is known to be contradictory
    pub fn is_contradictory_choice(&self, choice: &ReasoningChoice) -> bool {
        self.contradictory_choices.contains(choice)
    }

    /// Get backtracking statistics
    pub fn get_backtrack_stats(&self) -> &BacktrackStats {
        &self.stats
    }

    /// Get current reasoning level
    pub fn current_level(&self) -> usize {
        self.current_level
    }

    /// Get the latest backtrack point
    pub fn latest_backtrack_point(&self) -> Option<&BacktrackPoint> {
        self.backtrack_stack.last()
    }

    /// Check if there are pending backtrack points
    pub fn has_pending_choices(&self) -> bool {
        self.backtrack_stack
            .iter()
            .any(|point| !point.exhausted && !point.alternatives.is_empty())
    }

    pub fn create_choice_point(
        &mut self,
        node_id: NodeId,
        choice_type: ChoiceType,
    ) -> &mut ChoicePoint {
        let choice_id = self.next_choice_id;
        self.next_choice_id += 1;

        let choice_point = ChoicePoint::new(choice_id, node_id, choice_type);
        self.choice_points.push(choice_point);

        // SAFETY: We just pushed a choice_point above, so last_mut() cannot be None
        self.choice_points
            .last_mut()
            .expect("Choice point must exist after push")
    }

    pub fn get_dependencies(&self, node_id: NodeId) -> &[Dependency] {
        self.dependencies
            .get(&node_id)
            .map(|vec| vec.as_slice())
            .unwrap_or(&[])
    }

    pub fn get_dependent_nodes(&self, node_id: NodeId) -> HashSet<NodeId> {
        self.dependency_graph
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn backtrack_to_choice(&mut self, choice_id: usize) {
        // Remove all choice points after the specified one
        self.choice_points.retain(|cp| cp.id <= choice_id);

        // Rebuild dependencies based on remaining choice points
        self.rebuild_dependencies();
    }

    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.choice_points.clear();
        self.next_choice_id = 0;
        self.dependency_graph.clear();
        self.backtrack_stack.clear();
        self.node_dependencies.clear();
        self.contradictory_choices.clear();
        self.current_level = 0;
        self.stats = BacktrackStats::default();
    }

    fn rebuild_dependencies(&mut self) {
        // This is a simplified version - in practice, you'd want to
        // properly rebuild the dependency graph from remaining choice points
        self.dependencies.clear();
        self.dependency_graph.clear();

        // Rebuild from remaining backtrack points
        for point in &self.backtrack_stack {
            for dependency in &point.dependencies {
                for &dependent_node in &dependency.dependent_nodes {
                    self.dependencies
                        .entry(dependent_node)
                        .or_default()
                        .push(dependency.clone());
                }
            }
        }
    }

    pub fn get_latest_choice_point(&self) -> Option<&ChoicePoint> {
        self.choice_points.last()
    }

    pub fn has_choices(&self) -> bool {
        !self.choice_points.is_empty()
    }
}

impl Default for DependencyManager {
    fn default() -> Self {
        Self::new()
    }
}
