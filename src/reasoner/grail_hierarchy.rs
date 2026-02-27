//! GRAIL: Scalable Reachability Index for Large Graphs
//!
//! Implementation based on "GRAIL: Scalable Reachability Index for Large Graphs"
//! (Yildirim, Chaoji, Zaki - VLDB 2010)
//!
//! Key insight: Instead of materializing all ancestors (O(n²)), store randomized
//! interval labels that allow O(1) reachability tests with high probability.
//!
//! For a DAG, we assign each node v an interval [min_post(v), max_post(v)] where:
//! - min_post(v) = minimum postorder ID in v's subtree
//! - max_post(v) = maximum postorder ID in v's subtree
//!
//! Reachability test: u can reach v iff interval(u) contains interval(v)
//!
//! This works perfectly for trees. For DAGs, we use multiple randomized DFS
//! traversals to reduce false positives (when interval contains but no path exists).

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::ClassExpression;
use crate::reasoner::classification::{ClassHierarchy, ClassificationResult, ClassificationStats};

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::sync::Arc;

/// Class index for compact representation
pub type ClassIdx = u32;

/// GRAIL interval label: [min, max] postorder IDs
#[derive(Clone, Copy, Debug, Default)]
pub struct GrailInterval {
    pub min: u32,
    pub max: u32,
}

impl GrailInterval {
    /// Check if self contains other (self.min <= other.min && other.max <= self.max)
    #[inline(always)]
    pub fn contains(&self, other: &GrailInterval) -> bool {
        self.min <= other.min && other.max <= self.max
    }

    /// Check if intervals are disjoint
    #[inline(always)]
    pub fn is_disjoint(&self, other: &GrailInterval) -> bool {
        self.max < other.min || other.max < self.min
    }
}

/// GRAIL index with multiple traversals for DAGs
pub struct GrailIndex {
    /// Number of randomized traversals (higher = more accurate, slower build)
    num_traversals: usize,
    /// Interval labels for each class: intervals[traversal][class]
    intervals: Vec<Vec<GrailInterval>>,
    /// Direct parents (transitive reduction)
    direct_parents: Vec<SmallVec<[ClassIdx; 4]>>,
    /// Direct children (for forward traversal)
    direct_children: Vec<SmallVec<[ClassIdx; 4]>>,
    /// Whether the hierarchy is a tree (no node has >1 parent)
    is_tree: bool,
}

impl GrailIndex {
    /// Create new GRAIL index
    pub fn new(num_classes: usize, num_traversals: usize) -> Self {
        Self {
            num_traversals,
            intervals: vec![vec![GrailInterval::default(); num_classes]; num_traversals],
            direct_parents: vec![SmallVec::new(); num_classes],
            direct_children: vec![SmallVec::new(); num_classes],
            is_tree: true,
        }
    }

    /// Add edge from -> to (child -> parent in hierarchy terms)
    pub fn add_edge(&mut self, from: ClassIdx, to: ClassIdx) {
        self.direct_parents[from as usize].push(to);
        self.direct_children[to as usize].push(from);
        if self.direct_parents[from as usize].len() > 1 {
            self.is_tree = false;
        }
    }

    /// Build index with multiple randomized DFS traversals
    pub fn build(&mut self) {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let num_classes = self.direct_parents.len();
        let mut rng = thread_rng();

        for t in 0..self.num_traversals {
            // Randomize traversal order for each pass
            let mut order: Vec<ClassIdx> = (0..num_classes as ClassIdx).collect();
            order.shuffle(&mut rng);

            // Postorder counter
            let mut post_counter: u32 = 0;

            // DFS from roots (nodes with no parents)
            let mut roots: Vec<ClassIdx> = (0..num_classes as ClassIdx)
                .filter(|&i| self.direct_parents[i as usize].is_empty())
                .collect();
            roots.shuffle(&mut rng);

            let mut visited = vec![false; num_classes];

            for root in roots {
                self.dfs_build(root, &mut visited, &mut post_counter, t);
            }
        }
    }

    /// DFS to compute postorder intervals
    fn dfs_build(
        &mut self,
        node: ClassIdx,
        visited: &mut [bool],
        post_counter: &mut u32,
        traversal: usize,
    ) {
        if visited[node as usize] {
            return;
        }
        visited[node as usize] = true;

        // Initialize min to large value
        let mut min_post = u32::MAX;
        let mut max_post = 0u32;

        // Visit children first (for postorder)
        let children: Vec<ClassIdx> = self.direct_children[node as usize].iter().cloned().collect();
        for child in children {
            if !visited[child as usize] {
                self.dfs_build(child, visited, post_counter, traversal);
            }
            // Update min/max from child intervals
            let child_interval = &self.intervals[traversal][child as usize];
            min_post = min_post.min(child_interval.min);
            max_post = max_post.max(child_interval.max);
        }

        // Assign postorder number
        let post_id = *post_counter;
        *post_counter += 1;

        // Interval is [min(child intervals, post_id), max(child intervals, post_id)]
        self.intervals[traversal][node as usize] = GrailInterval {
            min: min_post.min(post_id),
            max: max_post.max(post_id),
        };
    }

    /// Query if from can reach to (from ⊑ to in hierarchy)
    /// Returns: (definitely_yes, definitely_no)
    /// - (true, false): Definitely reachable (ancestor interval contains descendant)
    /// - (false, true): Definitely not reachable (intervals disjoint)
    /// - (false, false): Uncertain (need BFS verification)
    pub fn can_reach(&self, from: ClassIdx, to: ClassIdx) -> (bool, bool) {
        let mut definitely_yes = true;

        for t in 0..self.num_traversals {
            let from_interval = &self.intervals[t][from as usize];
            let to_interval = &self.intervals[t][to as usize];

            // If any traversal shows disjoint, definitely not reachable
            if from_interval.is_disjoint(to_interval) {
                return (false, true);
            }

            // Intervals are built top-down (parent -> child), but reachability
            // is checked bottom-up (subclass -> superclass). So we need to
            // verify that the ancestor interval contains the descendant.
            if !to_interval.contains(from_interval) {
                definitely_yes = false;
            }
        }

        (definitely_yes, false)
    }

    /// Definitive reachability query (using BFS fallback when uncertain)
    pub fn is_reachable(&self, from: ClassIdx, to: ClassIdx) -> bool {
        // Quick checks
        if from == to {
            return true;
        }

        let (def_yes, def_no) = self.can_reach(from, to);

        if def_yes {
            // For trees, interval containment is exact.
            // For DAGs, verify to avoid false positives.
            if self.is_tree {
                return true;
            }
        }
        if def_no {
            return false;
        }

        // Uncertain: fall back to BFS
        self.bfs_reachable(from, to)
    }

    /// BFS for verification when GRAIL is uncertain
    fn bfs_reachable(&self, from: ClassIdx, to: ClassIdx) -> bool {
        use std::collections::VecDeque;

        let mut visited = FxHashSet::default();
        let mut queue = VecDeque::new();
        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return true;
            }

            for &parent in &self.direct_parents[current as usize] {
                if !visited.contains(&parent) {
                    visited.insert(parent);
                    queue.push_back(parent);
                }
            }
        }

        false
    }

    /// Get all ancestors (for compatibility with ClassHierarchy)
    pub fn get_all_ancestors(&self, node: ClassIdx) -> FxHashSet<ClassIdx> {
        let mut ancestors = FxHashSet::default();
        self.collect_ancestors(node, &mut ancestors);
        ancestors
    }

    fn collect_ancestors(&self, node: ClassIdx, ancestors: &mut FxHashSet<ClassIdx>) {
        for &parent in &self.direct_parents[node as usize] {
            if ancestors.insert(parent) {
                self.collect_ancestors(parent, ancestors);
            }
        }
    }
}

/// On-demand reachability with memoization
pub struct OnDemandReachability {
    /// Direct parents only
    direct_parents: Vec<SmallVec<[ClassIdx; 4]>>,
    /// Memoized positive results (known reachabilities)
    positive_cache: FxHashMap<(ClassIdx, ClassIdx), ()>,
    /// Memoized negative results
    negative_cache: FxHashMap<(ClassIdx, ClassIdx), ()>,
}

impl OnDemandReachability {
    pub fn new(num_classes: usize) -> Self {
        Self {
            direct_parents: vec![SmallVec::new(); num_classes],
            positive_cache: FxHashMap::default(),
            negative_cache: FxHashMap::default(),
        }
    }

    pub fn add_edge(&mut self, from: ClassIdx, to: ClassIdx) {
        self.direct_parents[from as usize].push(to);
    }

    /// Check reachability with memoization
    pub fn is_reachable(&mut self, from: ClassIdx, to: ClassIdx) -> bool {
        // Check cache
        if self.positive_cache.contains_key(&(from, to)) {
            return true;
        }
        if self.negative_cache.contains_key(&(from, to)) {
            return false;
        }

        // Compute
        let result = self.bfs_reachable(from, to);

        // Cache result
        if result {
            self.positive_cache.insert((from, to), ());
        } else {
            self.negative_cache.insert((from, to), ());
        }

        result
    }

    fn bfs_reachable(&self, from: ClassIdx, to: ClassIdx) -> bool {
        use std::collections::VecDeque;

        let mut visited = FxHashSet::default();
        let mut queue = VecDeque::new();
        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return true;
            }

            for &parent in &self.direct_parents[current as usize] {
                if !visited.contains(&parent) {
                    visited.insert(parent);
                    queue.push_back(parent);
                }
            }
        }

        false
    }
}

/// Hybrid hierarchy: uses GRAIL for fast queries, on-demand as fallback
pub struct HybridHierarchy {
    /// IRI pool
    iri_to_idx: FxHashMap<IRI, ClassIdx>,
    idx_to_iri: Vec<IRI>,
    /// GRAIL index
    grail: GrailIndex,
    /// Number of classes
    num_classes: usize,
    /// Thing/Nothing indices
    thing_idx: ClassIdx,
    nothing_idx: ClassIdx,
}

impl HybridHierarchy {
    pub fn new(ontology: &Ontology) -> OwlResult<Self> {
        let mut iri_to_idx: FxHashMap<IRI, ClassIdx> = FxHashMap::default();
        let mut idx_to_iri: Vec<IRI> = Vec::new();

        // Reserve Thing/Nothing
        let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing".to_string())
            .map_err(|e| OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Thing".to_string(),
                context: format!("Failed to create IRI: {}", e),
            })?;

        let nothing_iri = IRI::new("http://www.w3.org/2002/07/owl#Nothing".to_string())
            .map_err(|e| OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Nothing".to_string(),
                context: format!("Failed to create IRI: {}", e),
            })?;

        iri_to_idx.insert(thing_iri.clone(), 0);
        iri_to_idx.insert(nothing_iri.clone(), 1);
        idx_to_iri.push(thing_iri.clone());
        idx_to_iri.push(nothing_iri.clone());

        // Add all classes
        for class in ontology.classes() {
            let iri = (**class.iri()).clone();
            if !iri_to_idx.contains_key(&iri) {
                let idx = idx_to_iri.len() as ClassIdx;
                iri_to_idx.insert(iri.clone(), idx);
                idx_to_iri.push(iri);
            }
        }

        let num_classes = idx_to_iri.len();

        Ok(Self {
            iri_to_idx,
            idx_to_iri,
            grail: GrailIndex::new(num_classes, 3), // 3 traversals for DAGs
            num_classes,
            thing_idx: 0,
            nothing_idx: 1,
        })
    }

    pub fn build_from_ontology(&mut self, ontology: &Ontology) -> OwlResult<()> {
        // Process subclass axioms
        for axiom in ontology.subclass_axioms() {
            if let (
                ClassExpression::Class(sub_class),
                ClassExpression::Class(super_class),
            ) = (axiom.sub_class(), axiom.super_class()) {
                let sub_iri = (**sub_class.iri()).clone();
                let super_iri = (**super_class.iri()).clone();

                if let (Some(&sub_idx), Some(&super_idx)) = (
                    self.iri_to_idx.get(&sub_iri),
                    self.iri_to_idx.get(&super_iri),
                ) {
                    self.grail.add_edge(sub_idx, super_idx);
                }
            }
        }

        // Ensure Thing as root
        for idx in 0..self.num_classes {
            let idx = idx as ClassIdx;
            if idx != self.thing_idx &&
               idx != self.nothing_idx &&
               self.grail.direct_parents[idx as usize].is_empty() {
                self.grail.add_edge(idx, self.thing_idx);
            }
        }

        // Build GRAIL index
        let start = std::time::Instant::now();
        self.grail.build();
        if std::env::var("OWL2_REASONER_GRAIL_LOG").is_ok() {
            println!("GRAIL index built in {:?}", start.elapsed());
        }

        Ok(())
    }

    pub fn is_subclass_of(&self, sub: &IRI, sup: &IRI) -> bool {
        let sub_idx = self.iri_to_idx.get(sub).copied().unwrap_or(0);
        let sup_idx = self.iri_to_idx.get(sup).copied().unwrap_or(0);
        self.grail.is_reachable(sub_idx, sup_idx)
    }

    pub fn to_class_hierarchy(&self) -> ClassHierarchy {
        use std::sync::Arc;
        let ontology = Arc::new(Ontology::new());
        let mut hierarchy = ClassHierarchy::new(&ontology);

        // Materialize all ancestors for compatibility
        for idx in 0..self.num_classes {
            let iri = self.idx_to_iri[idx].clone();
            let ancestors = self.grail.get_all_ancestors(idx as ClassIdx);

            for ancestor_idx in ancestors {
                let ancestor_iri = self.idx_to_iri[ancestor_idx as usize].clone();
                hierarchy.add_parent(iri.clone(), ancestor_iri);
            }
        }

        hierarchy
    }

    pub fn get_stats(&self) -> (usize, usize) {
        let total_edges: usize = self.grail.direct_parents.iter().map(|v| v.len()).sum();
        (self.num_classes, total_edges)
    }
}

/// GRAIL-based classification engine
pub struct GrailClassificationEngine {
    ontology: Arc<Ontology>,
    hierarchy: HybridHierarchy,
}

impl GrailClassificationEngine {
    pub fn new(ontology: Ontology) -> OwlResult<Self> {
        Self::from_arc(Arc::new(ontology))
    }

    pub fn from_arc(ontology: Arc<Ontology>) -> OwlResult<Self> {
        let hierarchy = HybridHierarchy::new(&ontology)?;

        Ok(Self {
            ontology,
            hierarchy,
        })
    }

    pub fn classify(&mut self) -> OwlResult<ClassificationResult> {
        let start = std::time::Instant::now();

        self.hierarchy.build_from_ontology(&self.ontology)?;

        let elapsed = start.elapsed();
        let (num_classes, num_edges) = self.hierarchy.get_stats();

        let stats = ClassificationStats {
            classes_processed: num_classes,
            relationships_discovered: num_edges,
            equivalences_found: 0,
            disjointness_found: 0,
            time_ms: elapsed.as_millis() as u64,
            iterations: 1,
        };

        // Note: We're NOT materializing the full ClassHierarchy here
        // because that would be O(n²). Instead, we return empty hierarchy
        // and provide query methods.
        let hierarchy = ClassHierarchy::new(&self.ontology);

        Ok(ClassificationResult {
            hierarchy,
            stats,
            is_complete: true,
        })
    }

    pub fn is_subclass_of(&self, sub: &IRI, sup: &IRI) -> bool {
        self.hierarchy.is_subclass_of(sub, sup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grail_interval() {
        let i1 = GrailInterval { min: 1, max: 10 };
        let i2 = GrailInterval { min: 2, max: 5 };
        let i3 = GrailInterval { min: 15, max: 20 };

        assert!(i1.contains(&i2));
        assert!(!i2.contains(&i1));
        assert!(i1.is_disjoint(&i3));
    }

    #[test]
    fn test_grail_chain() {
        // Chain: 0 -> 1 -> 2 -> 3 (0 is root)
        let mut grail = GrailIndex::new(4, 3);
        grail.add_edge(1, 0);
        grail.add_edge(2, 1);
        grail.add_edge(3, 2);
        grail.build();

        // All should be reachable from their descendants
        assert!(grail.is_reachable(3, 0)); // 3 can reach 0
        assert!(grail.is_reachable(3, 1)); // 3 can reach 1
        assert!(grail.is_reachable(3, 2)); // 3 can reach 2
        assert!(grail.is_reachable(3, 3)); // 3 can reach itself

        assert!(!grail.is_reachable(0, 3)); // 0 cannot reach 3
        assert!(!grail.is_reachable(1, 3)); // 1 cannot reach 3
    }

    #[test]
    fn test_grail_diamond() {
        // Diamond: 0 at bottom, 1 and 2 in middle, 3 at top
        //    3
        //   / \
        //  1   2
        //   \ /
        //    0
        let mut grail = GrailIndex::new(4, 5); // More traversals for DAG
        grail.add_edge(0, 1);
        grail.add_edge(0, 2);
        grail.add_edge(1, 3);
        grail.add_edge(2, 3);
        grail.build();

        // 0 can reach everyone
        assert!(grail.is_reachable(0, 3));
        assert!(grail.is_reachable(0, 1));
        assert!(grail.is_reachable(0, 2));

        // 1 and 2 can reach 3
        assert!(grail.is_reachable(1, 3));
        assert!(grail.is_reachable(2, 3));

        // 1 and 2 cannot reach each other
        assert!(!grail.is_reachable(1, 2));
        assert!(!grail.is_reachable(2, 1));

        // 3 cannot reach anyone (except itself)
        assert!(!grail.is_reachable(3, 0));
        assert!(!grail.is_reachable(3, 1));
        assert!(!grail.is_reachable(3, 2));
    }
}
