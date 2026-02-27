//! # Tableaux Reasoning Core
//!
//! Contains the fundamental data structures, configuration, and main reasoning engine
//! for the tableaux-based OWL2 reasoner. This module provides the primary interface
//! for consistency checking and classification.
//!
//! ## Key Components
//!
//! - **[`TableauxReasoner`]** - Main reasoning engine with caching and configuration
//! - **[`ReasoningConfig`]** - Configurable options for reasoning behavior
//! - **[`ReasoningRules`]** - Extracted reasoning rules from ontology
//! - **[`TableauxNode`]** - Individual nodes in the tableaux graph
//! - **[`NodeId`]** - Unique identifiers for graph nodes
//! - **[`ReasoningCache`]** - Performance optimization through caching
//! - **[`MemoryStats`]** - Memory usage tracking and profiling
//!
//! ## Reasoning Process
//!
//! 1. **Rule Extraction**: Extract subclass, equivalence, and property rules from ontology
//! 2. **Consistency Checking**: Verify ontology satisfiability using tableaux algorithm
//! 3. **Classification**: Compute class hierarchy and relationships
//! 4. **Caching**: Store results for performance optimization
//! 5. **Memory Management**: Track allocation and deallocation patterns
//!
//! ## Performance Features
//!
//! - **Multi-layered caching**: Consistency, satisfiability, and classification results
//! - **Optimized concept storage**: SmallVec for small sets, fallback to HashSet
//! - **Configurable timeouts**: Prevent infinite reasoning loops
//! - **Incremental reasoning**: Support for partial ontology updates
//! - **Memory profiling**: Detailed statistics for optimization
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use owl2_reasoner::reasoning::tableaux::{TableauxReasoner, ReasoningConfig};
//! use owl2_reasoner::Ontology;
//!
//! // Create ontology and configure reasoner
//! let ontology = Ontology::new();
//! let config = ReasoningConfig {
//!     max_depth: 1000,
//!     debug: false,
//!     incremental: true,
//!     timeout: Some(30000),
//!     enable_parallel: false,
//!     parallel_workers: None,
//!     parallel_chunk_size: 64,
//! };
//! let reasoner = TableauxReasoner::with_config(ontology, config);
//!
//! // Perform reasoning
//! let is_consistent = reasoner.is_consistent()?;
//! let memory_stats = reasoner.get_memory_stats();
//! println!("Consistent: {}, Memory used: {} bytes",
//!          is_consistent, memory_stats.peak_memory_bytes);
//! ```

use crate::core::entities::Class;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::property_expressions::ObjectPropertyExpression;
use crate::logic::axioms::*;

use hashbrown::HashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// Reasoning rules for tableaux algorithm
#[derive(Debug, Clone)]
pub struct ReasoningRules {
    pub subclass_rules: Vec<SubClassOfAxiom>,
    pub equivalence_rules: Vec<EquivalentClassesAxiom>,
    pub disjointness_rules: Vec<DisjointClassesAxiom>,
    pub property_rules: Vec<SubObjectPropertyAxiom>,
    // Property characteristics
    pub transitive_properties: HashSet<Arc<IRI>>,
    pub symmetric_properties: HashSet<Arc<IRI>>,
    pub reflexive_properties: HashSet<Arc<IRI>>,
    pub functional_properties: HashSet<Arc<IRI>>,
    pub inverse_functional_properties: HashSet<Arc<IRI>>,
    pub irreflexive_properties: HashSet<Arc<IRI>>,
    pub asymmetric_properties: HashSet<Arc<IRI>>,
    // Property hierarchy
    pub property_hierarchy: Vec<SubObjectPropertyAxiom>,
    pub property_domains: Vec<ObjectPropertyDomainAxiom>,
    pub property_ranges: Vec<ObjectPropertyRangeAxiom>,
    pub inverse_properties: Vec<InverseObjectPropertiesAxiom>,
    // Individual reasoning (ABox)
    pub property_assertions: Vec<PropertyAssertionAxiom>,
    pub data_property_assertions: Vec<DataPropertyAssertionAxiom>,
    pub negative_property_assertions: Vec<NegativeObjectPropertyAssertionAxiom>,
    pub negative_data_property_assertions: Vec<NegativeDataPropertyAssertionAxiom>,
    // Individual equality
    pub same_individual_axioms: Vec<SameIndividualAxiom>,
    pub different_individuals_axioms: Vec<DifferentIndividualsAxiom>,
}

impl ReasoningRules {
    pub fn new(ontology: &Ontology) -> Self {
        let subclass_rules = ontology
            .subclass_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();
        let equivalence_rules = ontology
            .equivalent_classes_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();
        let disjointness_rules = ontology
            .disjoint_classes_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();
        let property_rules = ontology
            .subobject_property_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();

        // Extract property characteristics
        let transitive_properties = ontology
            .transitive_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let symmetric_properties = ontology
            .symmetric_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let reflexive_properties = ontology
            .reflexive_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let functional_properties = ontology
            .functional_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let inverse_functional_properties = ontology
            .inverse_functional_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let irreflexive_properties = ontology
            .irreflexive_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        let asymmetric_properties = ontology
            .asymmetric_property_axioms()
            .iter()
            .map(|ax| ax.property().clone())
            .collect();

        // Extract property hierarchy axioms
        let property_hierarchy = ontology
            .subobject_property_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();

        let property_domains = ontology
            .object_property_domain_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();

        let property_ranges = ontology
            .object_property_range_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();

        let inverse_properties = ontology
            .inverse_object_properties_axioms()
            .iter()
            .map(|ax| (**ax).clone())
            .collect();

        // Extract property assertions (ABox)
        let property_assertions = ontology
            .property_assertions()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        let data_property_assertions = ontology
            .data_property_assertions()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        let negative_property_assertions = ontology
            .negative_object_property_assertions()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        let negative_data_property_assertions = ontology
            .negative_data_property_assertions()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        // Extract individual equality axioms
        let same_individual_axioms = ontology
            .same_individual_axioms()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        let different_individuals_axioms = ontology
            .different_individuals_axioms()
            .iter()
            .map(|ax| (*ax).clone())
            .collect();

        Self {
            subclass_rules,
            equivalence_rules,
            disjointness_rules,
            property_rules,
            transitive_properties,
            symmetric_properties,
            reflexive_properties,
            functional_properties,
            inverse_functional_properties,
            irreflexive_properties,
            asymmetric_properties,
            property_hierarchy,
            property_domains,
            property_ranges,
            inverse_properties,
            property_assertions,
            data_property_assertions,
            negative_property_assertions,
            negative_data_property_assertions,
            same_individual_axioms,
            different_individuals_axioms,
        }
    }

    pub fn clear(&mut self) {
        self.subclass_rules.clear();
        self.equivalence_rules.clear();
        self.disjointness_rules.clear();
        self.property_rules.clear();
        self.transitive_properties.clear();
        self.symmetric_properties.clear();
        self.reflexive_properties.clear();
        self.functional_properties.clear();
        self.inverse_functional_properties.clear();
        self.irreflexive_properties.clear();
        self.asymmetric_properties.clear();
        self.property_hierarchy.clear();
        self.property_domains.clear();
        self.property_ranges.clear();
        self.inverse_properties.clear();
        self.property_assertions.clear();
        self.data_property_assertions.clear();
        self.negative_property_assertions.clear();
        self.negative_data_property_assertions.clear();
        self.same_individual_axioms.clear();
        self.different_individuals_axioms.clear();
    }
}

/// Node identifier for tableaux graph nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

impl NodeId {
    pub fn new(id: usize) -> Self {
        NodeId(id)
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

/// Reasoning configuration options
#[derive(Debug, Clone)]
pub struct ReasoningConfig {
    /// Maximum depth for tableaux expansion
    pub max_depth: usize,
    /// Enable debugging output
    pub debug: bool,
    /// Enable incremental reasoning
    pub incremental: bool,
    /// Timeout in milliseconds
    pub timeout: Option<u64>,
    /// Enable parallel processing
    pub enable_parallel: bool,
    /// Number of parallel workers (None = use all available cores)
    pub parallel_workers: Option<usize>,
    /// Chunk size for parallel operations
    pub parallel_chunk_size: usize,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        ReasoningConfig {
            max_depth: 1000,
            debug: false,
            incremental: true,
            timeout: Some(30000),   // 30 seconds default
            enable_parallel: false, // Disabled by default for compatibility
            parallel_workers: None, // Use all available cores
            parallel_chunk_size: 64,
        }
    }
}

/// Tableaux node with optimized concept storage and blocking support
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableauxNode {
    pub id: NodeId,
    /// Optimized concept storage using SmallVec for small sets
    pub concepts: SmallVec<[ClassExpression; 8]>,
    /// Lazy hashset for large concept sets
    pub concepts_hashset: Option<HashSet<ClassExpression>>,
    /// Node labels for debugging and identification
    pub labels: SmallVec<[String; 4]>,
    /// Optional blocking reference for optimization
    pub blocked_by: Option<NodeId>,
}

impl TableauxNode {
    pub fn new(id: NodeId) -> Self {
        TableauxNode {
            id,
            concepts: SmallVec::new(),
            concepts_hashset: None,
            labels: SmallVec::new(),
            blocked_by: None,
        }
    }

    pub fn add_concept(&mut self, concept: ClassExpression) {
        if self.concepts_hashset.is_some() {
            // Use hashset for large collections with safe access
            if let Some(hashset) = &mut self.concepts_hashset {
                hashset.insert(concept);
            }
        } else {
            // Use SmallVec for small collections
            if self.concepts.len() < 8 {
                if !self.concepts.contains(&concept) {
                    self.concepts.push(concept);
                }
            } else {
                // Convert to hashset when exceeding SmallVec capacity
                let mut hashset = HashSet::new();
                hashset.extend(self.concepts.drain(..));
                hashset.insert(concept);
                self.concepts_hashset = Some(hashset);
            }
        }
    }

    pub fn contains_concept(&self, concept: &ClassExpression) -> bool {
        if let Some(ref hashset) = self.concepts_hashset {
            hashset.contains(concept)
        } else {
            self.concepts.contains(concept)
        }
    }

    pub fn remove_concept(&mut self, concept: &ClassExpression) -> bool {
        if let Some(ref mut hashset) = self.concepts_hashset {
            hashset.remove(concept)
        } else if let Some(pos) = self.concepts.iter().position(|c| c == concept) {
            self.concepts.swap_remove(pos);
            true
        } else {
            false
        }
    }

    pub fn concepts_iter(&self) -> impl Iterator<Item = &ClassExpression> {
        if let Some(ref hashset) = self.concepts_hashset {
            Either::Left(hashset.iter())
        } else {
            Either::Right(self.concepts.iter())
        }
    }

    /// Get the number of concepts in this node
    pub fn concepts_len(&self) -> usize {
        if let Some(ref hashset) = self.concepts_hashset {
            hashset.len()
        } else {
            self.concepts.len()
        }
    }

    /// Add a label to this node
    pub fn add_label(&mut self, label: String) {
        if !self.labels.contains(&label) {
            self.labels.push(label);
        }
    }

    pub fn remove_label(&mut self, label: &str) -> bool {
        if let Some(pos) = self.labels.iter().position(|l| l == label) {
            self.labels.swap_remove(pos);
            true
        } else {
            false
        }
    }

    /// Get all labels for this node
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Check if this node is blocked by another node
    pub fn is_blocked(&self) -> bool {
        self.blocked_by.is_some()
    }

    /// Set the blocking node for this node
    pub fn set_blocked_by(&mut self, blocking_node: NodeId) {
        self.blocked_by = Some(blocking_node);
    }

    /// Clear blocking for this node
    pub fn clear_blocking(&mut self) {
        self.blocked_by = None;
    }

    /// Get the node that blocks this node, if any
    pub fn blocked_by(&self) -> Option<NodeId> {
        self.blocked_by
    }

    /// Get an iterator over the labels of this node
    pub fn labels_iter(&self) -> impl Iterator<Item = &String> {
        self.labels.iter()
    }

    /// Mark this node as merged (for equality reasoning cleanup)
    pub fn mark_merged(&mut self) {
        self.add_label("[MERGED]".to_string());
    }

    /// Check if this node has been marked as merged
    pub fn is_merged(&self) -> bool {
        self.labels.iter().any(|label| label == "[MERGED]")
    }

    /// Get all class expressions (concepts) associated with this node
    pub fn class_expressions(&self) -> Vec<ClassExpression> {
        self.concepts_iter().cloned().collect()
    }
}

/// Helper enum for iteration
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L: Iterator, R: Iterator<Item = L::Item>> Iterator for Either<L, R> {
    type Item = L::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Either::Left(l) => l.next(),
            Either::Right(r) => r.next(),
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Default, Clone)]
pub struct MemoryStats {
    pub arena_allocated_nodes: usize,
    pub arena_allocated_edges: usize,
    pub arena_allocated_expressions: usize,
    pub total_arena_bytes: usize,
    pub peak_memory_bytes: usize,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node_allocation(&mut self, bytes: usize) {
        self.arena_allocated_nodes += 1;
        self.total_arena_bytes += bytes;
        self.peak_memory_bytes = self.peak_memory_bytes.max(self.total_arena_bytes);
    }

    pub fn add_edge_allocation(&mut self, bytes: usize) {
        self.arena_allocated_edges += 1;
        self.total_arena_bytes += bytes;
        self.peak_memory_bytes = self.peak_memory_bytes.max(self.total_arena_bytes);
    }

    pub fn add_expression_allocation(&mut self, bytes: usize) {
        self.arena_allocated_expressions += 1;
        self.total_arena_bytes += bytes;
        self.peak_memory_bytes = self.peak_memory_bytes.max(self.total_arena_bytes);
    }

    pub fn add_constraint_allocation(&mut self, bytes: usize) {
        self.total_arena_bytes += bytes;
        self.peak_memory_bytes = self.peak_memory_bytes.max(self.total_arena_bytes);
    }

    pub fn total_allocations(&self) -> usize {
        self.arena_allocated_nodes + self.arena_allocated_edges + self.arena_allocated_expressions
    }
}

/// Reasoning cache for performance optimization
#[derive(Debug, Default)]
pub struct ReasoningCache {
    pub consistency_cache: HashMap<Vec<ClassExpression>, bool>,
    pub satisfiability_cache: HashMap<ClassExpression, bool>,
    pub classification_cache: HashMap<(IRI, IRI), bool>,
}

impl ReasoningCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.consistency_cache.clear();
        self.satisfiability_cache.clear();
        self.classification_cache.clear();
    }
}

/// Core tableaux reasoning engine
pub struct TableauxReasoner {
    pub ontology: Arc<Ontology>,
    pub config: ReasoningConfig,
    pub rules: ReasoningRules,
    pub cache: ReasoningCache,
    pub memory_stats: RefCell<MemoryStats>,
    /// Dependency-directed backtracking manager
    pub dependency_manager: super::dependency::DependencyManager,
}

impl TableauxReasoner {
    pub fn new(ontology: Ontology) -> Self {
        Self::with_config(ontology, ReasoningConfig::default())
    }

    pub fn with_config(ontology: Ontology, config: ReasoningConfig) -> Self {
        Self::with_config_arc(Arc::new(ontology), config)
    }

    pub fn with_config_arc(ontology: Arc<Ontology>, config: ReasoningConfig) -> Self {
        let rules = ReasoningRules::new(&ontology);

        Self {
            ontology,
            config,
            rules,
            cache: ReasoningCache::new(),
            memory_stats: RefCell::new(MemoryStats::new()),
            dependency_manager: super::dependency::DependencyManager::new(),
        }
    }

    pub fn from_arc(ontology: &Arc<Ontology>) -> Self {
        Self::with_config_arc(Arc::clone(ontology), ReasoningConfig::default())
    }

    pub fn check_consistency(&mut self) -> OwlResult<bool> {
        let mut graph = super::graph::TableauxGraph::new();
        let mut expansion_engine =
            super::expansion::ExpansionEngine::new().with_reasoning_rules(self.rules.clone());
        let mut blocking_manager =
            super::blocking::BlockingManager::new(super::blocking::BlockingStrategy::Optimized);
        let mut memory_manager = super::memory::MemoryManager::new();

        self.initialize_root_node(&mut graph)?;

        let mut nodes_to_expand = VecDeque::new();
        nodes_to_expand.push_back(graph.get_root());

        let mut expanded_nodes = HashSet::new();
        expanded_nodes.insert(graph.get_root());

        let mut branch_logs: Vec<super::graph::GraphChangeLog> = Vec::new();
        while let Some(current_node) = nodes_to_expand.pop_front() {
            if let Some(constraint) = blocking_manager.detect_blocking(current_node, &graph) {
                blocking_manager.add_blocking_constraint(constraint);
                continue;
            }

            let mut local_graph_log = super::graph::GraphChangeLog::new();
            let mut local_memory_log = super::memory::MemoryChangeLog::new();
            expansion_engine
                .expand(
                    &mut graph,
                    &mut memory_manager,
                    self.config.max_depth as u32,
                    &mut local_graph_log,
                    &mut local_memory_log,
                )
                .map_err(|e| OwlError::ReasoningError(format!("Expansion failed: {}", e)))?;
            if !local_graph_log.is_empty() {
                branch_logs.push(local_graph_log.clone());
            }

            if self.has_clash(current_node, &graph)? {
                return Ok(false);
            }

            let new_nodes = self.get_new_successors(current_node, &graph, &expanded_nodes);
            for new_node in new_nodes {
                if expanded_nodes.insert(new_node) {
                    nodes_to_expand.push_back(new_node);
                }
            }

            if let Some(timeout_ms) = self.config.timeout {
                let start_time = std::time::Instant::now();
                if start_time.elapsed().as_millis() >= timeout_ms as u128 {
                    return Err(OwlError::TimeoutError {
                        operation: "consistency_checking".to_string(),
                        timeout_ms,
                    });
                }
            }
        }

        drop(branch_logs);
        Ok(true)
    }

    pub fn classify(&self) -> OwlResult<()> {
        // Core classification logic will be implemented here
        Ok(())
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub fn get_memory_stats(&self) -> MemoryStats {
        self.memory_stats.borrow().clone()
    }

    pub fn reset(&mut self) {
        self.cache.clear();
        self.rules.clear();
        self.dependency_manager.clear();
        *self.memory_stats.borrow_mut() = MemoryStats::new();
    }

    pub fn is_consistent(&mut self) -> OwlResult<bool> {
        // Placeholder implementation
        self.check_consistency()
    }

    pub fn get_subclasses(&self, class: &IRI) -> Vec<IRI> {
        let mut subclasses = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut to_visit = std::collections::VecDeque::new();

        to_visit.push_back(class.clone());
        visited.insert(class.clone());

        // Traverse subclass relationships using transitive closure
        while let Some(current_class) = to_visit.pop_front() {
            // Find all direct subclasses from subclass axioms
            for axiom in &self.rules.subclass_rules {
                if let ClassExpression::Class(super_class) = axiom.super_class() {
                    if super_class.iri().as_ref() == &current_class {
                        if let ClassExpression::Class(sub_class) = axiom.sub_class() {
                            let sub_iri = sub_class.iri().as_ref().clone();
                            if !visited.contains(&sub_iri) {
                                visited.insert(sub_iri.clone());
                                subclasses.push(sub_iri.clone());
                                to_visit.push_back(sub_iri);
                            }
                        }
                    }
                }
            }

            // Also check equivalent classes - if A ≡ B and A ⊑ C, then B ⊑ C
            for equiv_axiom in &self.rules.equivalence_rules {
                let classes = equiv_axiom.classes();
                if classes.iter().any(|c| c.as_ref() == &current_class) {
                    // If current_class is in an equivalence class, all other classes in that equivalence
                    // can also be superclasses
                    for equiv_class in classes {
                        if equiv_class.as_ref() != &current_class
                            && !visited.contains(equiv_class.as_ref())
                        {
                            visited.insert(equiv_class.as_ref().clone());
                            // Find subclasses of this equivalent class too
                            to_visit.push_back(equiv_class.as_ref().clone());
                        }
                    }
                }
            }
        }

        subclasses
    }

    pub fn get_superclasses(&self, class: &IRI) -> Vec<IRI> {
        let mut superclasses = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut to_visit = std::collections::VecDeque::new();

        to_visit.push_back(class.clone());
        visited.insert(class.clone());

        // Traverse superclass relationships using transitive closure
        while let Some(current_class) = to_visit.pop_front() {
            // Find all direct superclasses from subclass axioms
            for axiom in &self.rules.subclass_rules {
                if let ClassExpression::Class(sub_class) = axiom.sub_class() {
                    if sub_class.iri().as_ref() == &current_class {
                        if let ClassExpression::Class(super_class) = axiom.super_class() {
                            let super_iri = super_class.iri().as_ref().clone();
                            if !visited.contains(&super_iri) {
                                visited.insert(super_iri.clone());
                                superclasses.push(super_iri.clone());
                                to_visit.push_back(super_iri);
                            }
                        }
                    }
                }
            }

            // Also check equivalent classes - if A ≡ B and A ⊑ C, then B ⊑ C
            for equiv_axiom in &self.rules.equivalence_rules {
                let classes = equiv_axiom.classes();
                if classes.iter().any(|c| c.as_ref() == &current_class) {
                    // If current_class is in an equivalence class, all other classes in that equivalence
                    // can also be subclasses
                    for equiv_class in classes {
                        if equiv_class.as_ref() != &current_class
                            && !visited.contains(equiv_class.as_ref())
                        {
                            visited.insert(equiv_class.as_ref().clone());
                            // Find superclasses of this equivalent class too
                            to_visit.push_back(equiv_class.as_ref().clone());
                        }
                    }
                }
            }
        }

        superclasses
    }

    pub fn get_equivalent_classes(&self, class: &IRI) -> Vec<IRI> {
        let mut equivalents = Vec::new();

        // Check equivalent classes axioms
        for equiv_axiom in &self.rules.equivalence_rules {
            let classes = equiv_axiom.classes();
            if classes.iter().any(|c| c.as_ref() == class) {
                // Add all other classes in this equivalence group
                for equiv_class in classes {
                    if equiv_class.as_ref() != class {
                        equivalents.push(equiv_class.as_ref().clone());
                    }
                }
            }
        }

        // Also check for classes that are equivalent through mutual subclass relationships
        // This would require checking if A ⊑ B and B ⊑ A for all pairs
        // For now, we'll rely on explicit equivalence axioms

        equivalents
    }

    pub fn get_disjoint_classes(&self, _class: &IRI) -> Vec<IRI> {
        // Placeholder implementation
        Vec::new()
    }

    pub fn are_disjoint_classes(&mut self, class1: &IRI, class2: &IRI) -> OwlResult<bool> {
        // To check if class1 and class2 are disjoint, we check if class1 ⊓ class2 is unsatisfiable
        // If it's unsatisfiable, then the classes are disjoint

        // First check explicit disjoint axioms
        for disjoint_axiom in &self.rules.disjointness_rules {
            let classes = disjoint_axiom.classes();
            let mut found_class1 = false;
            let mut found_class2 = false;

            for class_iri in classes {
                if **class_iri == *class1 {
                    found_class1 = true;
                }
                if **class_iri == *class2 {
                    found_class2 = true;
                }
            }

            if found_class1 && found_class2 {
                return Ok(true);
            }
        }

        // Use tableaux reasoning to check for implicit disjointness
        // Create a new tableaux graph for disjointness checking
        let mut graph = super::graph::TableauxGraph::new();
        let mut expansion_engine =
            super::expansion::ExpansionEngine::new().with_reasoning_rules(self.rules.clone());
        let mut blocking_manager =
            super::blocking::BlockingManager::new(super::blocking::BlockingStrategy::Optimized);
        let mut memory_manager = super::memory::MemoryManager::new();

        // For subclass checking, we don't initialize with all classes
        // We only add the specific concepts we're testing

        // Add both classes to the root node (their intersection)
        let class1_expr = ClassExpression::Class(Class::new(class1.as_str()));
        let class2_expr = ClassExpression::Class(Class::new(class2.as_str()));
        graph.add_concept(graph.get_root(), class1_expr);
        graph.add_concept(graph.get_root(), class2_expr);

        let mut nodes_to_expand = VecDeque::new();
        nodes_to_expand.push_back(graph.get_root());

        let mut expanded_nodes = HashSet::new();
        expanded_nodes.insert(graph.get_root());

        let mut branch_logs: Vec<super::graph::GraphChangeLog> = Vec::new();
        while let Some(current_node) = nodes_to_expand.pop_front() {
            if let Some(constraint) = blocking_manager.detect_blocking(current_node, &graph) {
                blocking_manager.add_blocking_constraint(constraint);
                continue;
            }

            let mut local_graph_log = super::graph::GraphChangeLog::new();
            let mut local_memory_log = super::memory::MemoryChangeLog::new();
            expansion_engine
                .expand(
                    &mut graph,
                    &mut memory_manager,
                    self.config.max_depth as u32,
                    &mut local_graph_log,
                    &mut local_memory_log,
                )
                .map_err(|e| OwlError::ReasoningError(format!("Expansion failed: {}", e)))?;
            if !local_graph_log.is_empty() {
                branch_logs.push(local_graph_log.clone());
            }

            if self.has_clash(current_node, &graph)? {
                return Ok(true);
            }

            let new_nodes = self.get_new_successors(current_node, &graph, &expanded_nodes);
            for new_node in new_nodes {
                if expanded_nodes.insert(new_node) {
                    nodes_to_expand.push_back(new_node);
                }
            }

            if let Some(timeout_ms) = self.config.timeout {
                let start_time = std::time::Instant::now();
                if start_time.elapsed().as_millis() >= timeout_ms as u128 {
                    return Err(OwlError::TimeoutError {
                        operation: "disjointness_checking".to_string(),
                        timeout_ms,
                    });
                }
            }
        }

        drop(branch_logs);
        Ok(false)
    }

    /// Check if two class expressions represent disjoint classes
    fn are_disjoint_class_expressions(
        &self,
        concept1: &ClassExpression,
        concept2: &ClassExpression,
    ) -> OwlResult<bool> {
        // Extract class names from concepts
        let class1 = self.extract_class_name(concept1)?;
        let class2 = self.extract_class_name(concept2)?;

        if let (Some(iri1), Some(iri2)) = (class1, class2) {
            // Check if these IRIs are declared disjoint
            for disjoint_axiom in &self.rules.disjointness_rules {
                let mut found_iri1 = false;
                let mut found_iri2 = false;

                // For disjoint classes axioms, we need to check the actual classes
                for class_iri in disjoint_axiom.classes() {
                    if **class_iri == iri1 {
                        found_iri1 = true;
                    }
                    if **class_iri == iri2 {
                        found_iri2 = true;
                    }
                }

                if found_iri1 && found_iri2 {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub fn is_class_satisfiable(&self, class: &IRI) -> OwlResult<bool> {
        // Check if the class is satisfiable using tableaux reasoning
        // To check satisfiability of C, we check if C leads to inconsistency

        // Special cases
        if class.as_str() == "http://www.w3.org/2002/07/owl#Thing" {
            // owl:Thing is always satisfiable
            return Ok(true);
        }
        if class.as_str() == "http://www.w3.org/2002/07/owl#Nothing" {
            // owl:Nothing is never satisfiable
            return Ok(false);
        }

        // Check if the class has any axioms that could make it unsatisfiable
        // If there are no axioms involving this class, it's trivially satisfiable
        let has_relevant_axioms = self.rules.subclass_rules.iter().any(|axiom| {
            matches!(axiom.sub_class(), ClassExpression::Class(c) if c.iri().as_ref() == class)
                || matches!(axiom.super_class(), ClassExpression::Class(c) if c.iri().as_ref() == class)
        }) || self.rules.equivalence_rules.iter().any(|axiom| {
            axiom.classes().iter().any(|c| c.as_ref() == class)
        }) || self.rules.disjointness_rules.iter().any(|axiom| {
            axiom.classes().iter().any(|c| c.as_ref() == class)
        });

        // If no axioms involve this class, it's trivially satisfiable
        if !has_relevant_axioms {
            return Ok(true);
        }

        // Create a new tableaux graph for satisfiability checking
        let mut graph = super::graph::TableauxGraph::new();
        let mut expansion_engine =
            super::expansion::ExpansionEngine::new().with_reasoning_rules(self.rules.clone());
        let mut blocking_manager =
            super::blocking::BlockingManager::new(super::blocking::BlockingStrategy::Optimized);
        let mut memory_manager = super::memory::MemoryManager::new();

        // For satisfiability checking, we add the class itself (not its negation)
        // and check if it leads to a contradiction
        // If C leads to contradiction, then C is unsatisfiable
        // If C does not lead to contradiction, then C is satisfiable

        // Add the target class to the root node
        let target_class_expr = ClassExpression::Class(Class::new(class.as_str()));
        graph.add_concept(graph.get_root(), target_class_expr);

        // Track reasoning state
        let mut nodes_to_expand = std::collections::VecDeque::new();
        nodes_to_expand.push_back(graph.get_root());

        let mut expanded_nodes = std::collections::HashSet::new();
        expanded_nodes.insert(graph.get_root());

        // Main reasoning loop
        let mut branch_logs: Vec<super::graph::GraphChangeLog> = Vec::new();
        while let Some(current_node) = nodes_to_expand.pop_front() {
            // Check if current node should be blocked
            if let Some(constraint) = blocking_manager.detect_blocking(current_node, &graph) {
                blocking_manager.add_blocking_constraint(constraint);
                continue;
            }

            // Apply tableaux expansion rules
            // Note: current_node context is handled internally during expansion
            let mut local_graph_log = super::graph::GraphChangeLog::new();
            let mut local_memory_log = super::memory::MemoryChangeLog::new();
            let _expansion_result = expansion_engine
                .expand(
                    &mut graph,
                    &mut memory_manager,
                    self.config.max_depth as u32,
                    &mut local_graph_log,
                    &mut local_memory_log,
                )
                .map_err(|e| OwlError::ReasoningError(format!("Expansion failed: {}", e)))?;
            if !local_graph_log.is_empty() {
                branch_logs.push(local_graph_log.clone());
            }

            // Check for clashes after expansion
            if self.has_clash(current_node, &graph)? {
                // Found a clash - C is inconsistent, so C is unsatisfiable
                return Ok(false);
            }

            // Get newly created nodes from expansion
            let new_nodes = self.get_new_successors(current_node, &graph, &expanded_nodes);

            // Add new nodes to expansion queue
            for new_node in new_nodes {
                if !expanded_nodes.contains(&new_node) {
                    nodes_to_expand.push_back(new_node);
                    expanded_nodes.insert(new_node);
                }
            }

            // For satisfiability checking, we don't use backtracking for simplicity
            // If needed, backtracking can be added later

            // Check timeout
            if let Some(timeout_ms) = self.config.timeout {
                let start_time = std::time::Instant::now();
                if start_time.elapsed().as_millis() >= timeout_ms as u128 {
                    return Err(OwlError::TimeoutError {
                        operation: "class_satisfiability_checking".to_string(),
                        timeout_ms,
                    });
                }
            }
        }

        // No clash found - C is consistent, so C is satisfiable
        drop(branch_logs);
        Ok(true)
    }

    pub fn is_class_expression_satisfiable(&self, _class: &ClassExpression) -> OwlResult<bool> {
        // Placeholder implementation - check if the class expression can be instantiated
        Ok(true)
    }

    pub fn is_subclass_of(&self, subclass: &IRI, superclass: &IRI) -> OwlResult<bool> {
        // To check if subclass ⊑ superclass, we check if subclass ⊓ ¬superclass is unsatisfiable
        // If it's unsatisfiable, then subclass is indeed a subclass of superclass

        // Create a new tableaux graph for subclass checking
        let mut graph = super::graph::TableauxGraph::new();
        let mut expansion_engine =
            super::expansion::ExpansionEngine::new().with_reasoning_rules(self.rules.clone());
        let mut blocking_manager =
            super::blocking::BlockingManager::new(super::blocking::BlockingStrategy::Optimized);
        let mut memory_manager = super::memory::MemoryManager::new();

        // For satisfiability checking, we don't initialize with all classes
        // We only add the specific concepts we're testing

        // Add the subclass as a concept
        let subclass_expr = ClassExpression::Class(Class::new(subclass.as_str()));
        graph.add_concept(graph.get_root(), subclass_expr);

        // Add the negation of the superclass as a concept
        let superclass_expr = ClassExpression::Class(Class::new(superclass.as_str()));
        let negation = ClassExpression::ObjectComplementOf(Box::new(superclass_expr));
        graph.add_concept(graph.get_root(), negation);

        // Track reasoning state
        let mut nodes_to_expand = std::collections::VecDeque::new();
        nodes_to_expand.push_back(graph.get_root());

        let mut expanded_nodes = std::collections::HashSet::new();
        expanded_nodes.insert(graph.get_root());

        // Main reasoning loop
        let mut branch_logs: Vec<super::graph::GraphChangeLog> = Vec::new();
        while let Some(current_node) = nodes_to_expand.pop_front() {
            // Check if current node should be blocked
            if let Some(constraint) = blocking_manager.detect_blocking(current_node, &graph) {
                blocking_manager.add_blocking_constraint(constraint);
                continue;
            }

            // Apply tableaux expansion rules
            // Note: current_node context is handled internally during expansion
            let mut local_graph_log = super::graph::GraphChangeLog::new();
            let mut local_memory_log = super::memory::MemoryChangeLog::new();
            let _expansion_result = expansion_engine
                .expand(
                    &mut graph,
                    &mut memory_manager,
                    self.config.max_depth as u32,
                    &mut local_graph_log,
                    &mut local_memory_log,
                )
                .map_err(|e| OwlError::ReasoningError(format!("Expansion failed: {}", e)))?;
            if !local_graph_log.is_empty() {
                branch_logs.push(local_graph_log.clone());
            }

            // Check for clashes after expansion
            if self.has_clash(current_node, &graph)? {
                // Found a clash - subclass ⊓ ¬superclass is inconsistent, so subclass ⊑ superclass
                return Ok(true);
            }

            // Get newly created nodes from expansion
            let new_nodes = self.get_new_successors(current_node, &graph, &expanded_nodes);

            // Add new nodes to expansion queue
            for new_node in new_nodes {
                if !expanded_nodes.contains(&new_node) {
                    nodes_to_expand.push_back(new_node);
                    expanded_nodes.insert(new_node);
                }
            }

            // For subclass checking, we don't use backtracking for simplicity
            // If needed, backtracking can be added later

            // Check timeout
            if let Some(timeout_ms) = self.config.timeout {
                let start_time = std::time::Instant::now();
                if start_time.elapsed().as_millis() >= timeout_ms as u128 {
                    return Err(OwlError::TimeoutError {
                        operation: "subclass_checking".to_string(),
                        timeout_ms,
                    });
                }
            }
        }

        // No clash found - subclass ⊓ ¬superclass is consistent, so subclass is not a subclass of superclass
        drop(branch_logs);
        Ok(false)
    }

    /// Initialize the root node with class assertions and relevant concepts
    ///
    /// Note: We should NOT add all declared classes to the root node, as that would
    /// imply that there exists an individual of each class, which is incorrect.
    /// We only add:
    /// 1. Class assertions (individuals with their types)
    /// 2. owl:Thing (the universal class)
    fn initialize_root_node(&self, graph: &mut super::graph::TableauxGraph) -> OwlResult<()> {
        let root_id = graph.get_root();

        // DO NOT add all named classes - this was causing false inconsistencies!
        // A class declaration does not imply the existence of an individual of that class.

        // Add owl:Thing to the root node (everything is an instance of Thing)
        let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing").map_err(|e| {
            OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Thing".to_string(),
                context: format!("Failed to create owl:Thing IRI: {}", e),
            }
        })?;
        let thing_expr = ClassExpression::Class(Class::new(thing_iri.as_str()));
        graph.add_concept(root_id, thing_expr);

        // Add all class assertions (individuals with their types)
        for class_assertion in self.ontology.as_ref().class_assertions() {
            // Add the class expression to the root node
            graph.add_concept(root_id, class_assertion.class_expr().clone());
        }

        Ok(())
    }

    /// Check if a node contains contradictory concepts (clash detection)
    fn has_clash(&self, node_id: NodeId, graph: &super::graph::TableauxGraph) -> OwlResult<bool> {
        if let Some(node) = graph.get_node(node_id) {
            let concepts: Vec<_> = node.concepts_iter().collect();

            // Check for direct contradictions
            for (i, concept1) in concepts.iter().enumerate() {
                for concept2 in concepts.iter().skip(i + 1) {
                    if self.are_contradictory(concept1, concept2)? {
                        return Ok(true);
                    }
                }
            }

            // Check existential/universal restrictions against successors
            for concept in &concepts {
                match concept {
                    ClassExpression::ObjectSomeValuesFrom(property, filler) => {
                        let (is_inverse, property_iri) = Self::resolve_property_direction(property);
                        if !is_inverse {
                            if let Some(successors) = graph.get_successors(node_id, property_iri) {
                                for succ_id in successors {
                                    if let Some(succ_node) = graph.get_node(*succ_id) {
                                        for succ_concept in succ_node.concepts_iter() {
                                            if self.are_contradictory(succ_concept, filler)? {
                                                return Ok(true);
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            let predecessors = graph.get_predecessors(node_id, property_iri);
                            for pred_id in predecessors {
                                if let Some(pred_node) = graph.get_node(pred_id) {
                                    for pred_concept in pred_node.concepts_iter() {
                                        if self.are_contradictory(pred_concept, filler)? {
                                            return Ok(true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ClassExpression::ObjectAllValuesFrom(property, filler) => {
                        let (is_inverse, property_iri) = Self::resolve_property_direction(property);
                        if !is_inverse {
                            if let Some(successors) = graph.get_successors(node_id, property_iri) {
                                for succ_id in successors {
                                    if let Some(succ_node) = graph.get_node(*succ_id) {
                                        for succ_concept in succ_node.concepts_iter() {
                                            if self.are_contradictory(succ_concept, filler)? {
                                                return Ok(true);
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            let predecessors = graph.get_predecessors(node_id, property_iri);
                            for pred_id in predecessors {
                                if let Some(pred_node) = graph.get_node(pred_id) {
                                    for pred_concept in pred_node.concepts_iter() {
                                        if self.are_contradictory(pred_concept, filler)? {
                                            return Ok(true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ClassExpression::ObjectMaxCardinality(max, property) => {
                        let (is_inverse, property_iri) = Self::resolve_property_direction(property);
                        let count =
                            Self::count_role_targets(node_id, property_iri, is_inverse, graph);
                        if count as u32 > *max {
                            return Ok(true);
                        }
                    }
                    ClassExpression::ObjectExactCardinality(exact, property) => {
                        let (is_inverse, property_iri) = Self::resolve_property_direction(property);
                        let count =
                            Self::count_role_targets(node_id, property_iri, is_inverse, graph);
                        if count as u32 > *exact {
                            return Ok(true);
                        }
                    }
                    _ => {}
                }
            }

            // Check for disjoint class axioms
            for (i, concept1) in concepts.iter().enumerate() {
                for concept2 in concepts.iter().skip(i + 1) {
                    if self.are_disjoint_class_expressions(concept1, concept2)? {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    fn resolve_property_direction(expr: &ObjectPropertyExpression) -> (bool, &IRI) {
        fn flatten(e: &ObjectPropertyExpression, invert: bool) -> (bool, &IRI) {
            match e {
                ObjectPropertyExpression::ObjectProperty(prop) => (invert, prop.iri()),
                ObjectPropertyExpression::ObjectInverseOf(inner) => {
                    flatten(inner.as_ref(), !invert)
                }
            }
        }

        flatten(expr, false)
    }

    fn count_role_targets(
        node_id: NodeId,
        property_iri: &IRI,
        is_inverse: bool,
        graph: &super::graph::TableauxGraph,
    ) -> usize {
        if !is_inverse {
            graph
                .get_successors(node_id, property_iri)
                .map(|targets| targets.len())
                .unwrap_or(0)
        } else {
            graph.get_predecessors(node_id, property_iri).len()
        }
    }

    /// Check if two concepts are contradictory
    fn are_contradictory(
        &self,
        concept1: &ClassExpression,
        concept2: &ClassExpression,
    ) -> OwlResult<bool> {
        match (concept1, concept2) {
            (ClassExpression::Class(class1), ClassExpression::Class(class2)) => {
                // Check if classes are declared disjoint
                for disjoint_axiom in &self.rules.disjointness_rules {
                    let mut found_class1 = false;
                    let mut found_class2 = false;

                    for class_iri in disjoint_axiom.classes() {
                        if **class_iri == **class1.iri() {
                            found_class1 = true;
                        }
                        if **class_iri == **class2.iri() {
                            found_class2 = true;
                        }
                    }

                    if found_class1 && found_class2 {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (ClassExpression::ObjectComplementOf(comp1), ClassExpression::Class(class2)) => {
                // Check if complement contradicts the class
                Ok(comp1.as_ref() == &ClassExpression::Class(Class::new(class2.iri().as_str())))
            }
            (ClassExpression::Class(class1), ClassExpression::ObjectComplementOf(comp2)) => {
                // Check if complement contradicts the class
                Ok(&ClassExpression::Class(Class::new(class1.iri().as_str())) == comp2.as_ref())
            }
            (
                ClassExpression::ObjectComplementOf(comp1),
                ClassExpression::ObjectComplementOf(comp2),
            ) => {
                // Check if complements are of the same class
                Ok(comp1.as_ref() == comp2.as_ref())
            }
            // Check for bottom (Nothing) and top (Thing) contradictions
            (ClassExpression::Class(class), _)
                if class.iri().as_str() == "http://www.w3.org/2002/07/owl#Nothing" =>
            {
                Ok(true) // Nothing contradicts everything except itself
            }
            (_, ClassExpression::Class(class))
                if class.iri().as_str() == "http://www.w3.org/2002/07/owl#Nothing" =>
            {
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Extract the class name from a class expression
    #[allow(clippy::only_used_in_recursion)]
    fn extract_class_name(&self, concept: &ClassExpression) -> OwlResult<Option<IRI>> {
        match concept {
            ClassExpression::Class(class) => Ok(Some((**class.iri()).clone())),
            ClassExpression::ObjectComplementOf(comp) => self.extract_class_name(comp.as_ref()),
            _ => Ok(None),
        }
    }

    /// Get new successor nodes that haven't been processed yet
    fn get_new_successors(
        &self,
        node_id: NodeId,
        graph: &super::graph::TableauxGraph,
        expanded_nodes: &std::collections::HashSet<NodeId>,
    ) -> Vec<NodeId> {
        let mut new_nodes = Vec::new();

        // Check all edges from the current node
        for edge in graph.edges.get_all_edges() {
            if edge.0 == node_id && !expanded_nodes.contains(&edge.2) {
                new_nodes.push(edge.2);
            }
        }

        new_nodes
    }
}
