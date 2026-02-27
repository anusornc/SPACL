//! Compact Hierarchy Representation for Large Ontologies
//!
//! This module provides a memory-efficient, cache-friendly representation
//! of class hierarchies using compact indices and bitsets instead of
//! HashMaps with IRI keys.
//!
//! ## Performance Characteristics
//! - Memory: O(n) bits per hierarchy level (vs O(n^2) pointers)
//! - Cache: Sequential access, 64 classes processed per cache line
//! - Operations: SIMD-friendly bit operations

use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::ClassExpression;
use crate::reasoner::classification::{ClassHierarchy, ClassificationResult, ClassificationStats};

use hashbrown::HashMap;
use std::sync::Arc;

/// Fixed-size bitset for compact class sets
/// Uses u64 chunks for SIMD-friendly operations
#[derive(Clone, Debug)]
pub struct CompactBitSet {
    chunks: Vec<u64>,
    size: usize, // Number of bits (classes)
}

impl CompactBitSet {
    /// Create a new bitset for `size` classes
    pub fn new(size: usize) -> Self {
        let num_chunks = (size + 63) / 64;
        Self {
            chunks: vec![0u64; num_chunks],
            size,
        }
    }

    /// Set bit at index
    #[inline(always)]
    pub fn set(&mut self, index: usize) {
        debug_assert!(index < self.size);
        let chunk = index / 64;
        let bit = index % 64;
        self.chunks[chunk] |= 1u64 << bit;
    }

    /// Check if bit is set
    #[inline(always)]
    pub fn contains(&self, index: usize) -> bool {
        if index >= self.size {
            return false;
        }
        let chunk = index / 64;
        let bit = index % 64;
        (self.chunks[chunk] >> bit) & 1 != 0
    }

    /// Union in-place: self |= other
    #[inline]
    pub fn union_with(&mut self, other: &CompactBitSet) {
        debug_assert_eq!(self.chunks.len(), other.chunks.len());
        for (s, o) in self.chunks.iter_mut().zip(other.chunks.iter()) {
            *s |= *o;
        }
    }

    /// Get all set indices as a vector
    pub fn to_vec(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for (chunk_idx, chunk) in self.chunks.iter().enumerate() {
            let mut bits = *chunk;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                result.push(chunk_idx * 64 + bit);
                bits &= bits - 1; // Clear lowest bit
            }
        }
        result
    }

    /// Count set bits
    pub fn count(&self) -> usize {
        self.chunks.iter().map(|c| c.count_ones() as usize).sum()
    }

    /// Clear all bits
    pub fn clear(&mut self) {
        for chunk in &mut self.chunks {
            *chunk = 0;
        }
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.chunks.iter().all(|c| *c == 0)
    }
}

/// Compact class index (32-bit for memory efficiency)
pub type ClassIdx = u32;

/// Memory-efficient hierarchy representation
pub struct CompactHierarchy {
    /// Map IRI to compact index
    iri_to_idx: HashMap<IRI, ClassIdx>,
    /// Map index back to IRI
    idx_to_iri: Vec<IRI>,
    /// Direct parents for each class (compact bitsets)
    direct_parents: Vec<CompactBitSet>,
    /// All ancestors (transitive closure) - computed on demand or pre-computed
    ancestors: Vec<CompactBitSet>,
    /// Direct children for each class
    direct_children: Vec<CompactBitSet>,
    /// Number of classes
    num_classes: usize,
    /// Index of owl:Thing
    thing_idx: ClassIdx,
    /// Index of owl:Nothing
    nothing_idx: ClassIdx,
}

impl CompactHierarchy {
    /// Create a new compact hierarchy from an ontology
    pub fn new(ontology: &Ontology) -> OwlResult<Self> {
        let mut iri_to_idx: HashMap<IRI, ClassIdx> = HashMap::new();
        let mut idx_to_iri: Vec<IRI> = Vec::new();

        // Reserve index 0 for owl:Thing, 1 for owl:Nothing
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

        // Assign indices to all classes
        for class in ontology.classes() {
            let iri = (**class.iri()).clone();
            if !iri_to_idx.contains_key(&iri) {
                let idx = idx_to_iri.len() as ClassIdx;
                iri_to_idx.insert(iri.clone(), idx);
                idx_to_iri.push(iri);
            }
        }

        let num_classes = idx_to_iri.len();

        // Initialize empty bitsets
        let direct_parents: Vec<CompactBitSet> = (0..num_classes)
            .map(|_| CompactBitSet::new(num_classes))
            .collect();
        
        let direct_children: Vec<CompactBitSet> = (0..num_classes)
            .map(|_| CompactBitSet::new(num_classes))
            .collect();

        let ancestors: Vec<CompactBitSet> = (0..num_classes)
            .map(|_| CompactBitSet::new(num_classes))
            .collect();

        Ok(Self {
            iri_to_idx,
            idx_to_iri,
            direct_parents,
            ancestors,
            direct_children,
            num_classes,
            thing_idx: 0,
            nothing_idx: 1,
        })
    }

    /// Add a direct parent relationship
    pub fn add_parent(&mut self, child: ClassIdx, parent: ClassIdx) {
        self.direct_parents[child as usize].set(parent as usize);
        self.direct_children[parent as usize].set(child as usize);
    }

    /// Build hierarchy from ontology axioms
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
                    self.add_parent(sub_idx, super_idx);
                }
            }
        }

        // Ensure all classes have owl:Thing as ancestor if no parents
        for idx in 0..self.num_classes {
            if idx != self.thing_idx as usize && 
               idx != self.nothing_idx as usize &&
               self.direct_parents[idx].is_empty() {
                self.add_parent(idx as ClassIdx, self.thing_idx);
            }
        }

        // Ensure owl:Nothing is child of all
        for idx in 0..self.num_classes {
            if idx != self.nothing_idx as usize {
                self.add_parent(self.nothing_idx, idx as ClassIdx);
            }
        }

        Ok(())
    }

    /// Compute transitive closure using iterative deepening
    /// This is O(n * depth) with very fast bit operations
    pub fn compute_transitive_closure(&mut self) {
        // Initialize ancestors with direct parents
        for idx in 0..self.num_classes {
            self.ancestors[idx] = self.direct_parents[idx].clone();
        }

        // Iteratively add ancestors of ancestors until fixpoint
        // This converges in O(depth) iterations
        let mut changed = true;
        let mut iterations = 0;
        let max_iterations = self.num_classes; // Safety bound

        while changed && iterations < max_iterations {
            changed = false;
            iterations += 1;

            for idx in 0..self.num_classes {
                let parents_vec = self.direct_parents[idx].to_vec();
                
                for &parent_idx in &parents_vec {
                    let parent_ancestors = self.ancestors[parent_idx].clone();
                    
                    // Check if we're adding new ancestors
                    let old_count = self.ancestors[idx].count();
                    self.ancestors[idx].union_with(&parent_ancestors);
                    let new_count = self.ancestors[idx].count();
                    
                    if new_count > old_count {
                        changed = true;
                    }
                }
            }
        }
    }

    /// Compute transitive closure using topological sort (BFS from roots)
    /// More efficient for tree-like structures
    pub fn compute_transitive_closure_topological(&mut self) {
        use std::collections::VecDeque;

        // Initialize
        for idx in 0..self.num_classes {
            self.ancestors[idx] = self.direct_parents[idx].clone();
        }

        // Find roots (classes with no parents except Thing)
        let mut roots: Vec<ClassIdx> = Vec::new();
        for idx in 0..self.num_classes {
            let parents = &self.direct_parents[idx];
            if parents.is_empty() || 
               (parents.count() == 1 && parents.contains(self.thing_idx as usize)) {
                roots.push(idx as ClassIdx);
            }
        }

        // BFS from roots
        let mut queue: VecDeque<ClassIdx> = VecDeque::from(roots);
        let mut processed = vec![false; self.num_classes];

        while let Some(current) = queue.pop_front() {
            if processed[current as usize] {
                continue;
            }
            processed[current as usize] = true;

            // Get children
            let children = self.direct_children[current as usize].to_vec();
            
            for child in children {
                // Child inherits all ancestors from current
                let current_ancestors = self.ancestors[current as usize].clone();
                self.ancestors[child as usize].union_with(&current_ancestors);
                
                if !processed[child as usize] {
                    queue.push_back(child as ClassIdx);
                }
            }
        }
    }

    /// Convert back to standard ClassHierarchy for compatibility
    pub fn to_class_hierarchy(&self) -> ClassHierarchy {
        use std::sync::Arc;
        let ontology = Arc::new(Ontology::new());
        let mut hierarchy = ClassHierarchy::new(&ontology);

        for idx in 0..self.num_classes {
            let iri = self.idx_to_iri[idx].clone();
            
            // Convert ancestors (parents) to IRI set
            let parent_indices = self.ancestors[idx].to_vec();
            for parent_idx in parent_indices {
                let parent_iri = self.idx_to_iri[parent_idx].clone();
                hierarchy.add_parent(iri.clone(), parent_iri);
            }
        }

        hierarchy
    }

    /// Get memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        let bitset_size = self.num_classes * ((self.num_classes + 63) / 64) * 8;
        let iri_map_size = self.iri_to_idx.len() * 32; // Rough estimate
        let iri_vec_size = self.idx_to_iri.len() * 32;
        
        bitset_size * 3 + iri_map_size + iri_vec_size // 3 bitsets: parents, children, ancestors
    }
}

/// Fast hierarchical classification using compact representation
pub struct CompactClassificationEngine {
    ontology: Arc<Ontology>,
    hierarchy: CompactHierarchy,
}

impl CompactClassificationEngine {
    pub fn new(ontology: Ontology) -> OwlResult<Self> {
        let ontology = Arc::new(ontology);
        let hierarchy = CompactHierarchy::new(&ontology)?;
        
        Ok(Self {
            ontology,
            hierarchy,
        })
    }

    pub fn classify(&mut self) -> OwlResult<ClassificationResult> {
        let start = std::time::Instant::now();

        // Build direct relationships
        self.hierarchy.build_from_ontology(&self.ontology)?;

        // Compute transitive closure
        self.hierarchy.compute_transitive_closure_topological();

        let elapsed = start.elapsed();
        
        // Convert to standard hierarchy
        let hierarchy = self.hierarchy.to_class_hierarchy();

        let stats = ClassificationStats {
            classes_processed: self.hierarchy.num_classes,
            relationships_discovered: self.hierarchy.ancestors.iter()
                .map(|a| a.count()).sum(),
            equivalences_found: 0,
            disjointness_found: 0,
            time_ms: elapsed.as_millis() as u64,
            iterations: 1,
        };

        Ok(ClassificationResult {
            hierarchy,
            stats,
            is_complete: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_bitset() {
        let mut bs = CompactBitSet::new(100);
        
        bs.set(5);
        bs.set(10);
        bs.set(63);
        bs.set(64);
        
        assert!(bs.contains(5));
        assert!(bs.contains(10));
        assert!(bs.contains(63));
        assert!(bs.contains(64));
        assert!(!bs.contains(0));
        assert!(!bs.contains(99));
        
        assert_eq!(bs.count(), 4);
        
        let vec = bs.to_vec();
        assert_eq!(vec, vec![5, 10, 63, 64]);
    }

    #[test]
    fn test_bitset_union() {
        let mut bs1 = CompactBitSet::new(100);
        let mut bs2 = CompactBitSet::new(100);
        
        bs1.set(1);
        bs1.set(2);
        bs2.set(2);
        bs2.set(3);
        
        bs1.union_with(&bs2);
        
        assert!(bs1.contains(1));
        assert!(bs1.contains(2));
        assert!(bs1.contains(3));
        assert_eq!(bs1.count(), 3);
    }
}
