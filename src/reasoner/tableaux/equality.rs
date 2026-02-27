//! # Equality Reasoning Module
//!
//! Provides comprehensive equality reasoning for the tableaux algorithm.
//! This module manages node equality/inequality relationships, detects clashes,
//! and handles node merging for functional and inverse functional properties.
//!
//! ## Key Components
//!
//! - **[`EqualityTracker`]** - Manages equality and inequality relationships between nodes
//! - **[`EqualityReasoner`]** - Performs equality reasoning and clash detection
//! - **Node Merging** - Merges equivalent nodes while preserving all properties
//! - **Dependency Tracking** - Tracks dependencies for equality conflicts
//!
//! ## Features
//!
//! - Union-Find data structure for efficient equality management
//! - Inequality constraint tracking and validation
//! - Node merging with dependency tracking
//! - Functional and inverse functional property clash detection
//! - Comprehensive equality reasoning for complex scenarios

use super::core::NodeId;
use super::graph::{GraphChangeLog, TableauxGraph};
use crate::core::iri::IRI;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Tracks equality and inequality relationships between nodes in the tableaux graph
///
/// Uses a Union-Find (Disjoint Set Union) data structure for efficient equality management
/// and maintains separate inequality constraints for clash detection.
#[derive(Debug, Clone)]
pub struct EqualityTracker {
    /// Union-Find structure for equality relationships
    parent: HashMap<NodeId, NodeId>,
    /// Rank for union by rank optimization
    rank: HashMap<NodeId, u32>,
    /// Inequality constraints between representatives
    inequalities: HashSet<(NodeId, NodeId)>,
    /// Mapping from representative to all nodes in the set
    sets: HashMap<NodeId, HashSet<NodeId>>,
    /// Dependencies for each equality merge
    dependencies: HashMap<(NodeId, NodeId), Vec<Arc<IRI>>>,
}

impl EqualityTracker {
    /// Create a new equality tracker
    pub fn new() -> Self {
        Self {
            parent: HashMap::new(),
            rank: HashMap::new(),
            inequalities: HashSet::new(),
            sets: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    /// Add a new node to the equality tracker
    pub fn add_node(&mut self, node_id: NodeId) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.parent.entry(node_id) {
            e.insert(node_id);
            self.rank.insert(node_id, 0);
            self.sets.insert(node_id, HashSet::from([node_id]));
        }
    }

    /// Find the representative (root) of a node's equivalence class
    pub fn find(&mut self, node_id: NodeId) -> NodeId {
        if let Some(&parent) = self.parent.get(&node_id) {
            if parent != node_id {
                // Path compression
                let root = self.find(parent);
                self.parent.insert(node_id, root);
                root
            } else {
                parent
            }
        } else {
            node_id
        }
    }

    /// Check if two nodes are known to be equal
    pub fn are_equal(&mut self, node1: NodeId, node2: NodeId) -> bool {
        self.find(node1) == self.find(node2)
    }

    /// Check if two nodes are known to be different
    pub fn are_different(&mut self, node1: NodeId, node2: NodeId) -> bool {
        if self.are_equal(node1, node2) {
            return false; // They're equal, so they can't be different
        }

        let rep1 = self.find(node1);
        let rep2 = self.find(node2);

        self.inequalities.contains(&(rep1, rep2)) || self.inequalities.contains(&(rep2, rep1))
    }

    /// Add an inequality constraint between two nodes
    pub fn add_inequality(&mut self, node1: NodeId, node2: NodeId) -> Result<(), String> {
        self.add_node(node1);
        self.add_node(node2);

        let rep1 = self.find(node1);
        let rep2 = self.find(node2);

        if rep1 == rep2 {
            return Err(format!(
                "Inequality clash: nodes {:?} and {:?} are both equal and different",
                node1, node2
            ));
        }

        self.inequalities.insert((rep1, rep2));
        self.inequalities.insert((rep2, rep1));
        Ok(())
    }

    /// Merge two nodes as equal
    pub fn merge(
        &mut self,
        node1: NodeId,
        node2: NodeId,
        dependency: Option<Arc<IRI>>,
    ) -> Result<NodeId, String> {
        self.add_node(node1);
        self.add_node(node2);

        let rep1 = self.find(node1);
        let rep2 = self.find(node2);

        if rep1 == rep2 {
            return Ok(rep1); // Already in the same set
        }

        // Check for inequality clash
        if self.inequalities.contains(&(rep1, rep2)) || self.inequalities.contains(&(rep2, rep1)) {
            return Err(format!(
                "Equality clash: cannot merge {:?} and {:?} due to inequality constraint",
                rep1, rep2
            ));
        }

        // Union by rank
        let (new_root, old_root) = match (self.rank.get(&rep1), self.rank.get(&rep2)) {
            (Some(&rank1), Some(&rank2)) => {
                if rank1 < rank2 {
                    (rep2, rep1)
                } else {
                    (rep1, rep2)
                }
            }
            _ => (rep1, rep2),
        };

        // Perform union
        self.parent.insert(old_root, new_root);

        // Update rank if needed
        if let (Some(&rank1), Some(&rank2)) = (self.rank.get(&rep1), self.rank.get(&rep2)) {
            if rank1 == rank2 {
                self.rank.insert(new_root, rank1 + 1);
            }
        }

        // Merge sets
        if let Some(old_set) = self.sets.remove(&old_root) {
            if let Some(new_set) = self.sets.get_mut(&new_root) {
                new_set.extend(old_set);
            } else {
                self.sets.insert(new_root, old_set);
            }
        }

        // Update inequalities
        let mut new_inequalities = HashSet::new();
        for &(a, b) in &self.inequalities {
            let new_a = if a == old_root { new_root } else { a };
            let new_b = if b == old_root { new_root } else { b };
            if new_a != new_b {
                new_inequalities.insert((new_a, new_b));
            }
        }
        self.inequalities = new_inequalities;

        // Add dependency
        if let Some(dep) = dependency {
            self.dependencies
                .entry((node1, node2))
                .or_default()
                .push(dep);
        }

        Ok(new_root)
    }

    /// Get all nodes in the same equivalence class as the given node
    pub fn get_equivalence_class(&mut self, node_id: NodeId) -> HashSet<NodeId> {
        let rep = self.find(node_id);
        self.sets.get(&rep).cloned().unwrap_or_default()
    }

    /// Get all inequalities involving the given node
    pub fn get_inequalities(&mut self, node_id: NodeId) -> HashSet<NodeId> {
        let rep = self.find(node_id);
        let mut result = HashSet::new();

        for &(a, b) in &self.inequalities {
            if a == rep {
                result.insert(b);
            } else if b == rep {
                result.insert(a);
            }
        }

        result
    }

    /// Check if merging would cause a clash
    pub fn can_merge(&mut self, node1: NodeId, node2: NodeId) -> bool {
        if self.are_equal(node1, node2) {
            return true;
        }

        let rep1 = self.find(node1);
        let rep2 = self.find(node2);

        !self.inequalities.contains(&(rep1, rep2)) && !self.inequalities.contains(&(rep2, rep1))
    }

    /// Get the size of an equivalence class
    pub fn equivalence_class_size(&mut self, node_id: NodeId) -> usize {
        let rep = self.find(node_id);
        self.sets.get(&rep).map_or(0, |set| set.len())
    }

    /// Check if there are any pending inequalities for a node
    pub fn has_inequalities(&mut self, node_id: NodeId) -> bool {
        let rep = self.find(node_id);
        self.inequalities.iter().any(|&(a, b)| a == rep || b == rep)
    }

    /// Get dependencies for a merge
    pub fn get_dependencies(&self, node1: NodeId, node2: NodeId) -> &[Arc<IRI>] {
        self.dependencies
            .get(&(node1, node2))
            .map_or(&[], |deps| deps.as_slice())
    }

    /// Clear all equality and inequality relationships
    pub fn clear(&mut self) {
        self.parent.clear();
        self.rank.clear();
        self.inequalities.clear();
        self.sets.clear();
        self.dependencies.clear();
    }

    /// Get statistics about the equality tracker
    pub fn get_stats(&self) -> EqualityStats {
        EqualityStats {
            total_nodes: self.parent.len(),
            equivalence_classes: self.sets.len(),
            inequalities: self.inequalities.len(),
            dependencies: self.dependencies.len(),
        }
    }
}

/// Statistics about the equality tracker
#[derive(Debug, Clone)]
pub struct EqualityStats {
    pub total_nodes: usize,
    pub equivalence_classes: usize,
    pub inequalities: usize,
    pub dependencies: usize,
}

/// Performs equality reasoning and clash detection
#[derive(Debug)]
pub struct EqualityReasoner {
    equality_tracker: EqualityTracker,
}

impl EqualityReasoner {
    /// Create a new equality reasoner
    pub fn new() -> Self {
        Self {
            equality_tracker: EqualityTracker::new(),
        }
    }

    /// Detect functional property clashes
    pub fn detect_functional_property_clash(
        &mut self,
        property: &IRI,
        source: NodeId,
        targets: &[NodeId],
    ) -> Option<FunctionalPropertyClash> {
        if targets.len() <= 1 {
            return None;
        }

        // Check if any two targets are known to be different
        for i in 0..targets.len() {
            for j in (i + 1)..targets.len() {
                if self.equality_tracker.are_different(targets[i], targets[j]) {
                    return Some(FunctionalPropertyClash {
                        property: property.clone(),
                        source,
                        conflicting_targets: vec![targets[i], targets[j]],
                        clash_type: FunctionalClashType::DifferentValues,
                    });
                }
            }
        }

        // If we can't prove they're different, we need to check if we can merge them
        let mut should_merge = Vec::new();
        for i in 1..targets.len() {
            if self.equality_tracker.can_merge(targets[0], targets[i]) {
                should_merge.push(targets[i]);
            }
        }

        if !should_merge.is_empty() {
            Some(FunctionalPropertyClash {
                property: property.clone(),
                source,
                conflicting_targets: should_merge,
                clash_type: FunctionalClashType::NeedsMerge,
            })
        } else {
            None
        }
    }

    /// Detect inverse functional property clashes
    pub fn detect_inverse_functional_property_clash(
        &mut self,
        property: &IRI,
        target: NodeId,
        sources: &[NodeId],
    ) -> Option<InverseFunctionalPropertyClash> {
        if sources.len() <= 1 {
            return None;
        }

        // Check if any two sources are known to be different
        for i in 0..sources.len() {
            for j in (i + 1)..sources.len() {
                if self.equality_tracker.are_different(sources[i], sources[j]) {
                    return Some(InverseFunctionalPropertyClash {
                        property: property.clone(),
                        target,
                        conflicting_sources: vec![sources[i], sources[j]],
                        clash_type: InverseFunctionalClashType::DifferentSources,
                    });
                }
            }
        }

        // If we can't prove they're different, check if we can merge them
        let mut should_merge = Vec::new();
        for i in 1..sources.len() {
            if self.equality_tracker.can_merge(sources[0], sources[i]) {
                should_merge.push(sources[i]);
            }
        }

        if !should_merge.is_empty() {
            Some(InverseFunctionalPropertyClash {
                property: property.clone(),
                target,
                conflicting_sources: should_merge,
                clash_type: InverseFunctionalClashType::NeedsMerge,
            })
        } else {
            None
        }
    }

    /// Merge nodes in the tableaux graph
    pub fn merge_nodes(
        &mut self,
        graph: &mut TableauxGraph,
        node1: NodeId,
        node2: NodeId,
        change_log: &mut GraphChangeLog,
    ) -> Result<NodeId, String> {
        let merged_rep = self.equality_tracker.merge(node1, node2, None)?;

        // Get all nodes in the equivalence class
        let equivalence_class = self.equality_tracker.get_equivalence_class(merged_rep);

        // Choose the representative node (prefer nodes with actual data)
        let representative = self.choose_representative(graph, &equivalence_class)?;

        // Merge all other nodes into the representative
        for &node_id in &equivalence_class {
            if node_id != representative {
                self.merge_into_node(graph, node_id, representative, change_log)?;
            }
        }

        Ok(representative)
    }

    /// Choose the best representative node from an equivalence class
    fn choose_representative(
        &self,
        graph: &TableauxGraph,
        equivalence_class: &HashSet<NodeId>,
    ) -> Result<NodeId, String> {
        if equivalence_class.is_empty() {
            return Err("Empty equivalence class".to_string());
        }

        // Prefer nodes with actual data (non-empty concepts)
        for &node_id in equivalence_class {
            if let Some(node) = graph.get_node(node_id) {
                if node.concepts_iter().next().is_some() {
                    return Ok(node_id);
                }
            }
        }

        // Fall back to the first node
        equivalence_class
            .iter()
            .next()
            .copied()
            .ok_or_else(|| "No representative found".to_string())
    }

    /// Merge source node into target node
    fn merge_into_node(
        &mut self,
        graph: &mut TableauxGraph,
        source: NodeId,
        target: NodeId,
        change_log: &mut GraphChangeLog,
    ) -> Result<(), String> {
        // Collect all data from source node first
        let source_concepts: Vec<_> = if let Some(source_node) = graph.get_node(source) {
            source_node.concepts_iter().cloned().collect()
        } else {
            return Ok(());
        };

        // Collect edges to transfer
        let mut outgoing_edges = Vec::new();
        let mut incoming_edges = Vec::new();

        for (from, property, to) in graph.edges.get_all_edges() {
            if *from == source {
                outgoing_edges.push((property.clone(), *to));
            }
            if *to == source {
                incoming_edges.push((*from, property.clone()));
            }
        }

        // Now transfer everything to target
        for concept in source_concepts {
            graph.add_concept_logged(target, concept, change_log);
        }

        for (property, to) in outgoing_edges {
            graph.add_edge_logged(target, &property, to, change_log);
        }

        for (from, property) in incoming_edges {
            graph.add_edge_logged(from, &property, target, change_log);
        }

        // Mark the source node as merged
        if let Some(source_node_mut) = graph.get_node_mut(source) {
            source_node_mut.mark_merged();
        }

        Ok(())
    }

    /// Add same individual axiom
    pub fn add_same_individual_axiom(
        &mut self,
        graph: &mut TableauxGraph,
        individuals: &[Arc<IRI>],
        change_log: &mut GraphChangeLog,
    ) -> Result<Vec<NodeId>, String> {
        if individuals.len() < 2 {
            return Ok(Vec::new());
        }

        // Find nodes for all individuals
        let mut nodes = Vec::new();
        for individual in individuals {
            if let Some(node_id) = self.find_individual_node(graph, individual) {
                nodes.push(node_id);
            } else {
                // Create a new node for this individual
                let node_id = graph.add_node();
                // Add a label to identify this individual
                graph.add_label_logged(node_id, individual.to_string(), change_log);
                nodes.push(node_id);
            }
        }

        // Merge all nodes
        let mut representative = nodes[0];
        for &node in &nodes[1..] {
            representative = self.merge_nodes(graph, representative, node, change_log)?;
        }

        Ok(nodes)
    }

    /// Add different individuals axiom
    pub fn add_different_individuals_axiom(
        &mut self,
        individuals: &[Arc<IRI>],
        node_ids: &[NodeId],
    ) -> Result<(), String> {
        if individuals.len() < 2 || node_ids.len() != individuals.len() {
            return Ok(());
        }

        // Add inequality constraints between all pairs
        for i in 0..node_ids.len() {
            for j in (i + 1)..node_ids.len() {
                self.equality_tracker
                    .add_inequality(node_ids[i], node_ids[j])?;
            }
        }

        Ok(())
    }

    /// Find node for an individual by IRI
    pub fn find_individual_node(&self, graph: &TableauxGraph, individual: &IRI) -> Option<NodeId> {
        // Search for a node with a label matching the individual IRI
        for (node_id, _) in graph.nodes_iter() {
            if let Some(node) = graph.get_node(node_id) {
                for label in node.labels_iter() {
                    if label.contains(&individual.to_string()) {
                        return Some(node_id);
                    }
                }
            }
        }
        None
    }

    /// Get a reference to the equality tracker
    pub fn equality_tracker(&self) -> &EqualityTracker {
        &self.equality_tracker
    }

    /// Get a mutable reference to the equality tracker
    pub fn equality_tracker_mut(&mut self) -> &mut EqualityTracker {
        &mut self.equality_tracker
    }
}

/// Represents a functional property clash
#[derive(Debug, Clone)]
pub struct FunctionalPropertyClash {
    pub property: IRI,
    pub source: NodeId,
    pub conflicting_targets: Vec<NodeId>,
    pub clash_type: FunctionalClashType,
}

/// Types of functional property clashes
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionalClashType {
    /// Two different values for a functional property
    DifferentValues,
    /// Multiple values that need to be merged
    NeedsMerge,
}

/// Represents an inverse functional property clash
#[derive(Debug, Clone)]
pub struct InverseFunctionalPropertyClash {
    pub property: IRI,
    pub target: NodeId,
    pub conflicting_sources: Vec<NodeId>,
    pub clash_type: InverseFunctionalClashType,
}

/// Types of inverse functional property clashes
#[derive(Debug, Clone, PartialEq)]
pub enum InverseFunctionalClashType {
    /// Two different sources for an inverse functional property
    DifferentSources,
    /// Multiple sources that need to be merged
    NeedsMerge,
}

impl Default for EqualityTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for EqualityReasoner {
    fn default() -> Self {
        Self::new()
    }
}

// Include comprehensive tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equality_tracker_basic_operations() {
        let mut tracker = EqualityTracker::new();

        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);
        let node3 = NodeId::new(3);

        tracker.add_node(node1);
        tracker.add_node(node2);
        tracker.add_node(node3);

        assert!(!tracker.are_equal(node1, node2));
        assert!(!tracker.are_equal(node1, node3));

        // Merge node1 and node2
        let rep = tracker.merge(node1, node2, None).unwrap();
        assert!(tracker.are_equal(node1, node2));
        assert_eq!(tracker.find(node1), tracker.find(node2));

        // Add inequality
        tracker.add_inequality(rep, node3).unwrap();
        assert!(tracker.are_different(rep, node3));

        // Try to merge with inequality - should fail
        assert!(tracker.merge(rep, node3, None).is_err());
    }

    #[test]
    fn test_functional_property_clash_detection() {
        let mut reasoner = EqualityReasoner::new();
        let property = IRI::new("http://example.org/hasParent").unwrap();
        let source = NodeId::new(1);
        let target1 = NodeId::new(2);
        let target2 = NodeId::new(3);

        // Add inequality between targets
        reasoner
            .equality_tracker_mut()
            .add_inequality(target1, target2)
            .unwrap();

        let clash =
            reasoner.detect_functional_property_clash(&property, source, &[target1, target2]);
        assert!(clash.is_some());

        let clash = clash.unwrap();
        assert_eq!(clash.clash_type, FunctionalClashType::DifferentValues);
    }

    #[test]
    fn test_inverse_functional_property_clash_detection() {
        let mut reasoner = EqualityReasoner::new();
        let property = IRI::new("http://example.org/hasSSN").unwrap();
        let target = NodeId::new(1);
        let source1 = NodeId::new(2);
        let source2 = NodeId::new(3);

        // Add inequality between sources
        reasoner
            .equality_tracker_mut()
            .add_inequality(source1, source2)
            .unwrap();

        let clash = reasoner.detect_inverse_functional_property_clash(
            &property,
            target,
            &[source1, source2],
        );
        assert!(clash.is_some());

        let clash = clash.unwrap();
        assert_eq!(
            clash.clash_type,
            InverseFunctionalClashType::DifferentSources
        );
    }

    #[test]
    fn test_node_merging_with_graph() {
        let mut graph = TableauxGraph::new();
        let mut reasoner = EqualityReasoner::new();
        let mut change_log = GraphChangeLog::new();

        // Create two nodes
        let node1 = graph.add_node();
        let node2 = graph.add_node();

        // Add concepts to node1
        let class1 = crate::core::entities::Class::new("http://example.org/Person");
        let concept1 = crate::logic::axioms::class_expressions::ClassExpression::Class(class1);
        graph.add_concept_logged(node1, concept1.clone(), &mut change_log);

        // Add concepts to node2
        let class2 = crate::core::entities::Class::new("http://example.org/Adult");
        let concept2 = crate::logic::axioms::class_expressions::ClassExpression::Class(class2);
        graph.add_concept_logged(node2, concept2.clone(), &mut change_log);

        // Merge nodes
        let representative = reasoner
            .merge_nodes(&mut graph, node1, node2, &mut change_log)
            .unwrap();

        // Verify merge
        assert!(graph.get_node(representative).is_some());
        assert!(
            graph
                .get_node(representative)
                .unwrap()
                .concepts_iter()
                .count()
                == 2
        );
    }
}
