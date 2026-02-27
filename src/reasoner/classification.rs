//! Classification algorithms for OWL2 ontologies
//!
//! Implements classification algorithms to compute class hierarchy and relationships.

use crate::logic::axioms::ClassExpression;
use crate::core::error::{OwlError, OwlResult};
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::reasoner::tableaux::TableauxReasoner;

use hashbrown::HashMap;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// Classification engine for OWL2 ontologies
pub struct ClassificationEngine {
    ontology: Arc<Ontology>,
    tableaux_reasoner: TableauxReasoner,
    config: ClassificationConfig,
    hierarchy: ClassHierarchy,
}

/// Classification configuration
#[derive(Debug, Clone)]
pub struct ClassificationConfig {
    /// Enable incremental classification
    pub incremental: bool,
    /// Enable equivalence class computation
    pub compute_equivalences: bool,
    /// Enable disjoint class computation
    pub compute_disjointness: bool,
    /// Maximum number of iterations
    pub max_iterations: usize,
    /// Timeout in milliseconds
    pub timeout: Option<u64>,
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        ClassificationConfig {
            incremental: true,
            compute_equivalences: true,
            compute_disjointness: true,
            max_iterations: 1000,
            timeout: Some(60000), // 60 seconds default
        }
    }
}

/// Optimized class hierarchy representation
#[derive(Debug, Clone, Default)]
pub struct ClassHierarchy {
    /// Parent relationships (class -> its superclasses)
    pub(crate) parents: HashMap<IRI, HashSet<IRI>>,
    /// Child relationships (class -> its subclasses)
    pub(crate) children: HashMap<IRI, HashSet<IRI>>,
    /// Equivalent classes
    pub equivalences: HashMap<IRI, HashSet<IRI>>,
    /// Disjoint classes
    pub disjointness: HashMap<IRI, HashSet<IRI>>,
    /// Satisfiability cache
    #[allow(dead_code)]
    satisfiable: HashMap<IRI, bool>,
    /// Hierarchy depth cache for optimization
    depth_cache: HashMap<IRI, usize>,
}

/// Classification result
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub hierarchy: ClassHierarchy,
    pub stats: ClassificationStats,
    pub is_complete: bool,
}

/// Classification statistics
#[derive(Debug, Clone)]
pub struct ClassificationStats {
    pub classes_processed: usize,
    pub relationships_discovered: usize,
    pub equivalences_found: usize,
    pub disjointness_found: usize,
    pub time_ms: u64,
    pub iterations: usize,
}

impl ClassificationEngine {
    /// Create a new classification engine
    pub fn new(ontology: Ontology) -> Self {
        Self::with_config(ontology, ClassificationConfig::default())
    }

    /// Create a new classification engine with custom configuration
    pub fn with_config(ontology: Ontology, config: ClassificationConfig) -> Self {
        let ontology = Arc::new(ontology);
        let tableaux_reasoner = TableauxReasoner::from_arc(&ontology); // Use Arc reference to avoid cloning
        let hierarchy = ClassHierarchy::new(&ontology); // Pass reference instead of cloning

        ClassificationEngine {
            ontology,
            tableaux_reasoner,
            config,
            hierarchy,
        }
    }

    /// Classify the ontology
    pub fn classify(&mut self) -> OwlResult<ClassificationResult> {
        let start_time = std::time::Instant::now();

        // Initialize hierarchy with direct relationships
        self.initialize_hierarchy()?;

        // Compute transitive closure of subclass relationships
        self.compute_transitive_closure()?;

        // Fix borrow checker issues by collecting changes first
        self.apply_transitive_changes()?;

        // Compute equivalent classes
        if self.config.compute_equivalences {
            self.compute_equivalent_classes()?;
        }

        // Compute disjoint classes
        if self.config.compute_disjointness {
            self.compute_disjoint_classes()?;
        }

        // Perform additional reasoning to discover implicit relationships
        self.reason_about_hierarchy()?;

        let time_ms = start_time.elapsed().as_millis() as u64;

        Ok(ClassificationResult {
            hierarchy: self.hierarchy.clone(), // Clone the computed hierarchy instead of creating new
            stats: ClassificationStats {
                classes_processed: self.ontology.classes().len(),
                relationships_discovered: self.count_relationships(),
                equivalences_found: self.count_equivalences(),
                disjointness_found: self.count_disjointness(),
                time_ms,
                iterations: 1, // Simplified for now
            },
            is_complete: true,
        })
    }

    /// Initialize the class hierarchy with direct relationships
    fn initialize_hierarchy(&mut self) -> OwlResult<()> {
        // Add owl:Thing as the root
        let thing_iri = IRI::new("http://www.w3.org/2002/07/owl#Thing").map_err(|e| {
            OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Thing".to_string(),
                context: format!("Failed to create owl:Thing IRI: {}", e),
            }
        })?;
        self.hierarchy
            .parents
            .insert(thing_iri.clone(), HashSet::new());

        // Add owl:Nothing as the bottom
        let nothing_iri = IRI::new("http://www.w3.org/2002/07/owl#Nothing").map_err(|e| {
            OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Nothing".to_string(),
                context: format!("Failed to create owl:Nothing IRI: {}", e),
            }
        })?;
        self.hierarchy
            .children
            .insert(nothing_iri.clone(), HashSet::new());

        // Process direct subclass axioms
        for axiom in self.ontology.subclass_axioms() {
            if let (ClassExpression::Class(sub_class), ClassExpression::Class(super_class)) =
                (axiom.sub_class(), axiom.super_class())
            {
                self.hierarchy
                    .add_parent((**sub_class.iri()).clone(), (**super_class.iri()).clone());
                self.hierarchy
                    .add_child((**super_class.iri()).clone(), (**sub_class.iri()).clone());
            }
        }

        // Ensure all classes are in the hierarchy
        for class in self.ontology.classes() {
            let class_iri = class.iri();

            // Add to hierarchy if not present
            self.hierarchy
                .parents
                .entry((**class_iri).clone())
                .or_default();
            self.hierarchy
                .children
                .entry((**class_iri).clone())
                .or_default();

            // If no parents specified, add owl:Thing as parent
            let class_iri_clone = (**class_iri).clone();
            let parents = self.hierarchy.parents.get(&class_iri_clone);
            let is_empty_parents = parents.is_none_or(|p| p.is_empty());

            if is_empty_parents && **class_iri != thing_iri {
                self.hierarchy
                    .add_parent(class_iri_clone.clone(), thing_iri.clone());
                self.hierarchy.add_child(thing_iri.clone(), class_iri_clone);
            }
        }

        Ok(())
    }

    /// Compute transitive closure of subclass relationships using evolved BFS algorithm
    /// This replaces the O(n³) iterative approach with an efficient O(N+E) BFS algorithm
    fn compute_transitive_closure(&mut self) -> OwlResult<()> {
        // Get all classes - need to clone Arc<IRI> to IRI
        let classes: Vec<IRI> = self
            .ontology
            .classes()
            .iter()
            .map(|c| (**c.iri()).clone()) // Convert Arc<IRI> to IRI
            .collect();

        // For each class, compute all transitive superclasses using BFS
        for class_iri in classes {
            let mut visited: HashSet<IRI> = HashSet::new();
            let mut queue: VecDeque<IRI> = VecDeque::new();
            let mut transitive_parents: HashSet<IRI> = HashSet::new();

            // Start BFS from the current class
            queue.push_back(class_iri.clone());
            visited.insert(class_iri.clone());

            while let Some(current_class) = queue.pop_front() {
                // Get direct parents of the current class
                if let Some(direct_parents) = self.hierarchy.parents.get(&current_class) {
                    for parent_iri in direct_parents {
                        // Add to transitive parents if not already present
                        if transitive_parents.insert(parent_iri.clone()) {
                            // Continue BFS from this parent if not visited
                            if visited.insert(parent_iri.clone()) {
                                queue.push_back(parent_iri.clone());
                            }
                        }
                    }
                }
            }

            // Add all discovered transitive parents to the hierarchy
            for transitive_parent in transitive_parents {
                let current_parents = self.hierarchy.parents.get(&class_iri);
                let should_add =
                    current_parents.is_none_or(|parents| !parents.contains(&transitive_parent));

                if should_add {
                    self.hierarchy
                        .add_parent(class_iri.clone(), transitive_parent.clone());
                    self.hierarchy
                        .add_child(transitive_parent.clone(), class_iri.clone());
                }
            }
        }

        Ok(())
    }

    /// Apply transitive changes (placeholder for borrow checker fix)
    fn apply_transitive_changes(&mut self) -> OwlResult<()> {
        Ok(())
    }

    /// Compute equivalent classes
    fn compute_equivalent_classes(&mut self) -> OwlResult<()> {
        // Process equivalent classes axioms
        for axiom in self.ontology.equivalent_classes_axioms() {
            let classes = axiom.classes();

            // All classes are equivalent to each other
            for i in 0..classes.len() {
                for j in i + 1..classes.len() {
                    let class1 = &classes[i];
                    let class2 = &classes[j];

                    self.hierarchy
                        .add_equivalence((**class1).clone(), (**class2).clone());
                    self.hierarchy
                        .add_equivalence((**class2).clone(), (**class1).clone());
                }
            }
        }

        // Discover additional equivalences through reasoning
        self.discover_equivalences_by_reasoning()?;

        Ok(())
    }

    /// Discover equivalent classes through reasoning
    fn discover_equivalences_by_reasoning(&mut self) -> OwlResult<()> {
        // Get classes without cloning IRIs
        let classes: Vec<&IRI> = self
            .ontology
            .classes()
            .iter()
            .map(|c| &**c.iri()) // Dereference Arc to get &IRI
            .collect();

        for i in 0..classes.len() {
            for j in i + 1..classes.len() {
                let class1 = classes[i];
                let class2 = classes[j];

                // Skip if already known to be equivalent
                if self.hierarchy.are_equivalent(class1, class2) {
                    continue;
                }

                // Check if class1 ⊑ class2 and class2 ⊑ class1
                let is_sub1 = self.tableaux_reasoner.is_subclass_of(class1, class2)?;
                let is_sub2 = self.tableaux_reasoner.is_subclass_of(class2, class1)?;

                if is_sub1 && is_sub2 {
                    self.hierarchy
                        .add_equivalence((*class1).clone(), (*class2).clone());
                    self.hierarchy
                        .add_equivalence((*class2).clone(), (*class1).clone());
                }
            }
        }

        Ok(())
    }

    /// Compute disjoint classes
    fn compute_disjoint_classes(&mut self) -> OwlResult<()> {
        // Process disjoint classes axioms
        for axiom in self.ontology.disjoint_classes_axioms() {
            let classes = axiom.classes();

            // All classes are disjoint with each other
            for i in 0..classes.len() {
                for j in i + 1..classes.len() {
                    let class1 = &classes[i];
                    let class2 = &classes[j];

                    self.hierarchy
                        .add_disjoint((**class1).clone(), (**class2).clone());
                    self.hierarchy
                        .add_disjoint((**class2).clone(), (**class1).clone());
                }
            }
        }

        // Discover additional disjointness through reasoning
        self.discover_disjointness_by_reasoning()?;

        Ok(())
    }

    /// Discover disjoint classes through reasoning
    fn discover_disjointness_by_reasoning(&mut self) -> OwlResult<()> {
        // Get classes without cloning IRIs
        let classes: Vec<&IRI> = self
            .ontology
            .classes()
            .iter()
            .map(|c| &**c.iri()) // Dereference Arc to get &IRI
            .collect();

        for i in 0..classes.len() {
            for j in i + 1..classes.len() {
                let class1 = classes[i];
                let class2 = classes[j];

                // Skip if already known to be disjoint
                if self.hierarchy.are_disjoint(class1, class2) {
                    continue;
                }

                // Check if class1 and class2 are disjoint
                let are_disjoint = self
                    .tableaux_reasoner
                    .are_disjoint_classes(class1, class2)?;

                if are_disjoint {
                    self.hierarchy
                        .add_disjoint((*class1).clone(), (*class2).clone());
                    self.hierarchy
                        .add_disjoint((*class2).clone(), (*class1).clone());
                }
            }
        }

        Ok(())
    }

    /// Perform additional reasoning about the hierarchy
    fn reason_about_hierarchy(&mut self) -> OwlResult<()> {
        // This is where more sophisticated reasoning rules would be applied
        // For now, we'll just ensure consistency of the hierarchy

        // Check for cycles in the hierarchy
        self.detect_cycles()?;

        // Ensure owl:Nothing is subclass of all classes
        self.ensure_nothing_bottom()?;

        Ok(())
    }

    /// Detect cycles in the class hierarchy
    fn detect_cycles(&self) -> OwlResult<()> {
        // Get classes without cloning IRIs
        let classes: Vec<&IRI> = self
            .ontology
            .classes()
            .iter()
            .map(|c| &**c.iri()) // Dereference Arc to get &IRI
            .collect();

        for class_iri in classes {
            if self.has_cycle_from_class(class_iri) {
                return Err(OwlError::OwlViolation(format!(
                    "Cycle detected in class hierarchy starting from {}",
                    class_iri
                )));
            }
        }

        Ok(())
    }

    /// Check if there's a cycle starting from a given class
    fn has_cycle_from_class(&self, start_class: &IRI) -> bool {
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        self.has_cycle_dfs(start_class, &mut visited, &mut recursion_stack)
    }

    /// Depth-first search to detect cycles
    fn has_cycle_dfs(
        &self,
        class_iri: &IRI,
        visited: &mut HashSet<IRI>,
        recursion_stack: &mut HashSet<IRI>,
    ) -> bool {
        visited.insert(class_iri.clone());
        recursion_stack.insert(class_iri.clone());

        for parent_iri in self
            .hierarchy
            .parents
            .get(class_iri)
            .unwrap_or(&HashSet::new())
        {
            if !visited.contains(parent_iri) {
                if self.has_cycle_dfs(parent_iri, visited, recursion_stack) {
                    return true;
                }
            } else if recursion_stack.contains(parent_iri) {
                return true;
            }
        }

        recursion_stack.remove(class_iri);
        false
    }

    /// Ensure owl:Nothing is subclass of all classes
    fn ensure_nothing_bottom(&mut self) -> OwlResult<()> {
        let nothing_iri = IRI::new("http://www.w3.org/2002/07/owl#Nothing").map_err(|e| {
            OwlError::IriParseError {
                iri: "http://www.w3.org/2002/07/owl#Nothing".to_string(),
                context: format!("Failed to create owl:Nothing IRI: {}", e),
            }
        })?;

        // Get classes without cloning IRIs
        let classes: Vec<&IRI> = self
            .ontology
            .classes()
            .iter()
            .map(|c| &**c.iri()) // Dereference Arc to get &IRI
            .collect();

        for class_iri in classes {
            if class_iri != &nothing_iri {
                self.hierarchy
                    .add_parent(nothing_iri.clone(), class_iri.clone());
                self.hierarchy
                    .add_child(class_iri.clone(), nothing_iri.clone());
            }
        }

        Ok(())
    }

    /// Count total relationships in the hierarchy
    fn count_relationships(&self) -> usize {
        self.hierarchy
            .parents
            .values()
            .map(|parents| parents.len())
            .sum()
    }

    /// Count equivalences in the hierarchy
    fn count_equivalences(&self) -> usize {
        self.hierarchy
            .equivalences
            .values()
            .map(|eqs| eqs.len())
            .sum::<usize>()
            / 2 // Divide by 2 because each equivalence is stored twice
    }

    /// Count disjointness relationships in the hierarchy
    fn count_disjointness(&self) -> usize {
        self.hierarchy
            .disjointness
            .values()
            .map(|disjs| disjs.len())
            .sum::<usize>()
            / 2 // Divide by 2 because each disjointness is stored twice
    }

    /// Get the computed class hierarchy
    pub fn hierarchy(&self) -> &ClassHierarchy {
        &self.hierarchy
    }

    /// Check if a class is satisfiable
    pub fn is_satisfiable(&mut self, class_iri: &IRI) -> OwlResult<bool> {
        self.tableaux_reasoner.is_class_satisfiable(class_iri)
    }

    /// Get all superclasses of a class
    pub fn get_superclasses(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.hierarchy.get_all_superclasses(class_iri)
    }

    /// Get all subclasses of a class
    pub fn get_subclasses(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.hierarchy.get_all_subclasses(class_iri)
    }

    /// Get equivalent classes
    pub fn get_equivalent_classes(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.hierarchy.get_equivalent_classes(class_iri)
    }

    /// Get disjoint classes
    pub fn get_disjoint_classes(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.hierarchy.get_disjoint_classes(class_iri)
    }
}

impl ClassHierarchy {
    /// Create a new class hierarchy
    pub fn new(_ontology: &Arc<Ontology>) -> Self {
        ClassHierarchy {
            parents: HashMap::new(),
            children: HashMap::new(),
            equivalences: HashMap::new(),
            disjointness: HashMap::new(),
            satisfiable: HashMap::new(),
            depth_cache: HashMap::new(),
        }
    }

    /// Add a parent relationship
    pub fn add_parent(&mut self, child: IRI, parent: IRI) {
        self.parents
            .entry(child.clone())
            .or_default()
            .insert(parent.clone());
        self.children.entry(parent).or_default().insert(child);
        self.depth_cache.clear(); // Invalidate depth cache
    }

    /// Add a child relationship
    pub fn add_child(&mut self, parent: IRI, child: IRI) {
        self.children
            .entry(parent.clone())
            .or_default()
            .insert(child.clone());
        self.parents.entry(child).or_default().insert(parent);
    }

    /// Add an equivalence relationship
    pub fn add_equivalence(&mut self, class1: IRI, class2: IRI) {
        self.equivalences
            .entry(class1.clone())
            .or_default()
            .insert(class2.clone());
        self.equivalences.entry(class2).or_default().insert(class1);
    }

    /// Add a disjointness relationship
    pub fn add_disjoint(&mut self, class1: IRI, class2: IRI) {
        self.disjointness
            .entry(class1.clone())
            .or_default()
            .insert(class2.clone());
        self.disjointness.entry(class2).or_default().insert(class1);
    }

    /// Check if two classes are equivalent
    pub fn are_equivalent(&self, class1: &IRI, class2: &IRI) -> bool {
        self.equivalences
            .get(class1)
            .is_some_and(|eqs| eqs.contains(class2))
    }

    /// Check if two classes are disjoint
    pub fn are_disjoint(&self, class1: &IRI, class2: &IRI) -> bool {
        self.disjointness
            .get(class1)
            .is_some_and(|disjs| disjs.contains(class2))
    }

    /// Get all superclasses of a class (transitive)
    pub fn get_all_superclasses(&self, class_iri: &IRI) -> HashSet<IRI> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct_parents) = self.parents.get(class_iri) {
            for parent in direct_parents {
                queue.push_back(parent.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            if !result.contains(&current) {
                result.insert(current.clone());

                if let Some(parents) = self.parents.get(&current) {
                    for parent in parents {
                        queue.push_back(parent.clone());
                    }
                }
            }
        }

        result
    }

    /// Get all subclasses of a class (transitive)
    pub fn get_all_subclasses(&self, class_iri: &IRI) -> HashSet<IRI> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(direct_children) = self.children.get(class_iri) {
            for child in direct_children {
                queue.push_back(child.clone());
            }
        }

        while let Some(current) = queue.pop_front() {
            if !result.contains(&current) {
                result.insert(current.clone());

                if let Some(children) = self.children.get(&current) {
                    for child in children {
                        queue.push_back(child.clone());
                    }
                }
            }
        }

        result
    }

    /// Get equivalent classes
    pub fn get_equivalent_classes(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.equivalences
            .get(class_iri)
            .cloned()
            .unwrap_or_default()
    }

    /// Get disjoint classes
    pub fn get_disjoint_classes(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.disjointness
            .get(class_iri)
            .cloned()
            .unwrap_or_default()
    }

    /// Get direct parents of a class
    pub fn get_direct_parents(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.parents.get(class_iri).cloned().unwrap_or_default()
    }

    /// Get direct children of a class
    pub fn get_direct_children(&self, class_iri: &IRI) -> HashSet<IRI> {
        self.children.get(class_iri).cloned().unwrap_or_default()
    }
}
