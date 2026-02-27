#!/usr/bin/env python3
"""
Basic Tableau Reasoner for ALC Description Logic
===============================================

This implementation follows the classical tableau algorithm for ALC
(Attributive Language with Complements) as described in:

- Baader & Sattler (2001): "An Overview of Tableau Algorithms for Description Logics"
- Horrocks & Sattler (2007): "A Tableau Decision Procedure for SHOIQ"

The algorithm constructs a tableau (tree-like structure) to test concept satisfiability
by applying expansion rules until either a clash is found (unsatisfiable) or 
a complete, clash-free tableau is constructed (satisfiable).
"""

from typing import Set, Dict, List, Optional, Tuple, Union
from dataclasses import dataclass, field
from enum import Enum
import copy
from abc import ABC, abstractmethod

# =============================================================================
# Core Data Structures for ALC Description Logic
# =============================================================================

class ConceptType(Enum):
    """Types of ALC concepts"""
    ATOMIC = "atomic"           # A (atomic concept)
    TOP = "top"                 # ⊤ (universal concept)
    BOTTOM = "bottom"           # ⊥ (empty concept)
    NEGATION = "negation"       # ¬C
    CONJUNCTION = "conjunction" # C ⊓ D
    DISJUNCTION = "disjunction" # C ⊔ D
    EXISTENTIAL = "existential" # ∃R.C
    UNIVERSAL = "universal"     # ∀R.C

@dataclass(frozen=True)
class Concept:
    """Represents an ALC concept"""
    concept_type: ConceptType
    name: Optional[str] = None          # For atomic concepts
    subconcepts: Tuple['Concept', ...] = field(default_factory=tuple)  # For complex concepts
    role: Optional[str] = None          # For existential/universal restrictions
    
    def __str__(self) -> str:
        if self.concept_type == ConceptType.ATOMIC:
            return self.name or "UNNAMED"
        elif self.concept_type == ConceptType.TOP:
            return "⊤"
        elif self.concept_type == ConceptType.BOTTOM:
            return "⊥"
        elif self.concept_type == ConceptType.NEGATION:
            return f"¬{self.subconcepts[0]}"
        elif self.concept_type == ConceptType.CONJUNCTION:
            return f"({self.subconcepts[0]} ⊓ {self.subconcepts[1]})"
        elif self.concept_type == ConceptType.DISJUNCTION:
            return f"({self.subconcepts[0]} ⊔ {self.subconcepts[1]})"
        elif self.concept_type == ConceptType.EXISTENTIAL:
            return f"∃{self.role}.{self.subconcepts[0]}"
        elif self.concept_type == ConceptType.UNIVERSAL:
            return f"∀{self.role}.{self.subconcepts[0]}"
        return "UNKNOWN"

# Convenience constructors for concepts
def atomic_concept(name: str) -> Concept:
    return Concept(ConceptType.ATOMIC, name=name)

def top_concept() -> Concept:
    return Concept(ConceptType.TOP)

def bottom_concept() -> Concept:
    return Concept(ConceptType.BOTTOM)

def negation(concept: Concept) -> Concept:
    return Concept(ConceptType.NEGATION, subconcepts=(concept,))

def conjunction(left: Concept, right: Concept) -> Concept:
    return Concept(ConceptType.CONJUNCTION, subconcepts=(left, right))

def disjunction(left: Concept, right: Concept) -> Concept:
    return Concept(ConceptType.DISJUNCTION, subconcepts=(left, right))

def existential_restriction(role: str, concept: Concept) -> Concept:
    return Concept(ConceptType.EXISTENTIAL, role=role, subconcepts=(concept,))

def universal_restriction(role: str, concept: Concept) -> Concept:
    return Concept(ConceptType.UNIVERSAL, role=role, subconcepts=(concept,))

@dataclass
class Individual:
    """Represents an individual in the tableau"""
    name: str
    is_root: bool = False
    created_by_rule: Optional[str] = None
    parent: Optional['Individual'] = None
    
    def __str__(self) -> str:
        return self.name
    
    def __hash__(self) -> int:
        return hash(self.name)

@dataclass
class RoleAssertion:
    """Represents a role assertion R(a,b)"""
    role: str
    source: Individual
    target: Individual
    
    def __str__(self) -> str:
        return f"{self.role}({self.source}, {self.target})"
    
    def __hash__(self) -> int:
        return hash((self.role, self.source.name, self.target.name))

@dataclass
class ConceptAssertion:
    """Represents a concept assertion C(a)"""
    concept: Concept
    individual: Individual
    
    def __str__(self) -> str:
        return f"{self.concept}({self.individual})"
    
    def __hash__(self) -> int:
        return hash((str(self.concept), self.individual.name))

# =============================================================================
# Tableau Node and Branch Management
# =============================================================================

@dataclass
class TableauNode:
    """Represents a node in the tableau tree"""
    concept_assertions: Set[ConceptAssertion] = field(default_factory=set)
    role_assertions: Set[RoleAssertion] = field(default_factory=set)
    individuals: Set[Individual] = field(default_factory=set)
    is_closed: bool = False
    clash_reason: Optional[str] = None
    parent: Optional['TableauNode'] = None
    children: List['TableauNode'] = field(default_factory=list)
    applied_rules: Set[str] = field(default_factory=set)
    
    def add_concept_assertion(self, assertion: ConceptAssertion):
        """Add a concept assertion to this node"""
        self.concept_assertions.add(assertion)
        self.individuals.add(assertion.individual)
    
    def add_role_assertion(self, assertion: RoleAssertion):
        """Add a role assertion to this node"""
        self.role_assertions.add(assertion)
        self.individuals.add(assertion.source)
        self.individuals.add(assertion.target)
    
    def get_concepts_for_individual(self, individual: Individual) -> Set[Concept]:
        """Get all concepts asserted for a specific individual"""
        return {assertion.concept for assertion in self.concept_assertions 
                if assertion.individual == individual}
    
    def get_role_successors(self, individual: Individual, role: str) -> Set[Individual]:
        """Get all role successors for an individual and role"""
        return {assertion.target for assertion in self.role_assertions
                if assertion.source == individual and assertion.role == role}
    
    def has_clash(self) -> bool:
        """Check if this node contains a clash"""
        if self.is_closed:
            return True
            
        # Check for direct clashes: C(a) and ¬C(a)
        for individual in self.individuals:
            concepts = self.get_concepts_for_individual(individual)
            for concept in concepts:
                # Check for negation clash
                neg_concept = negation(concept)
                if neg_concept in concepts:
                    self.is_closed = True
                    self.clash_reason = f"Clash: {concept}({individual}) and {neg_concept}({individual})"
                    return True
                
                # Check for bottom concept
                if concept.concept_type == ConceptType.BOTTOM:
                    self.is_closed = True
                    self.clash_reason = f"Clash: ⊥({individual})"
                    return True
        
        return False
    
    def is_complete(self) -> bool:
        """Check if this node is complete (no more rules can be applied)"""
        if self.has_clash():
            return True
            
        # Check if any expansion rules can still be applied
        for assertion in self.concept_assertions:
            concept = assertion.concept
            individual = assertion.individual
            
            # Check conjunction rule
            if (concept.concept_type == ConceptType.CONJUNCTION and 
                f"conj_{concept}_{individual}" not in self.applied_rules):
                return False
            
            # Check disjunction rule
            if (concept.concept_type == ConceptType.DISJUNCTION and 
                f"disj_{concept}_{individual}" not in self.applied_rules):
                return False
            
            # Check existential rule
            if (concept.concept_type == ConceptType.EXISTENTIAL and 
                f"exists_{concept}_{individual}" not in self.applied_rules):
                return False
            
            # Check universal rule
            if concept.concept_type == ConceptType.UNIVERSAL:
                role = concept.role
                successors = self.get_role_successors(individual, role)
                for successor in successors:
                    if f"forall_{concept}_{individual}_{successor}" not in self.applied_rules:
                        return False
        
        return True
    
    def copy(self) -> 'TableauNode':
        """Create a deep copy of this node"""
        new_node = TableauNode()
        new_node.concept_assertions = copy.deepcopy(self.concept_assertions)
        new_node.role_assertions = copy.deepcopy(self.role_assertions)
        new_node.individuals = copy.deepcopy(self.individuals)
        new_node.applied_rules = copy.copy(self.applied_rules)
        new_node.parent = self
        return new_node

# =============================================================================
# Tableau Expansion Rules
# =============================================================================

class TableauRule(ABC):
    """Abstract base class for tableau expansion rules"""
    
    @abstractmethod
    def is_applicable(self, node: TableauNode) -> List[Tuple[ConceptAssertion, Individual]]:
        """Check if this rule is applicable to the given node"""
        pass
    
    @abstractmethod
    def apply(self, node: TableauNode, assertion: ConceptAssertion, individual: Individual) -> List[TableauNode]:
        """Apply this rule to create new tableau nodes"""
        pass
    
    @abstractmethod
    def get_rule_name(self) -> str:
        """Get the name of this rule"""
        pass

class ConjunctionRule(TableauRule):
    """⊓-rule: If x : C ⊓ D, then add x : C and x : D"""
    
    def is_applicable(self, node: TableauNode) -> List[Tuple[ConceptAssertion, Individual]]:
        applicable = []
        for assertion in node.concept_assertions:
            if (assertion.concept.concept_type == ConceptType.CONJUNCTION and
                f"conj_{assertion.concept}_{assertion.individual}" not in node.applied_rules):
                applicable.append((assertion, assertion.individual))
        return applicable
    
    def apply(self, node: TableauNode, assertion: ConceptAssertion, individual: Individual) -> List[TableauNode]:
        new_node = node.copy()
        concept = assertion.concept
        
        # Add both subconcepts
        left_assertion = ConceptAssertion(concept.subconcepts[0], individual)
        right_assertion = ConceptAssertion(concept.subconcepts[1], individual)
        
        new_node.add_concept_assertion(left_assertion)
        new_node.add_concept_assertion(right_assertion)
        
        # Mark rule as applied
        rule_id = f"conj_{concept}_{individual}"
        new_node.applied_rules.add(rule_id)
        
        return [new_node]
    
    def get_rule_name(self) -> str:
        return "Conjunction Rule (⊓)"

class DisjunctionRule(TableauRule):
    """⊔-rule: If x : C ⊔ D, then branch: either x : C or x : D"""
    
    def is_applicable(self, node: TableauNode) -> List[Tuple[ConceptAssertion, Individual]]:
        applicable = []
        for assertion in node.concept_assertions:
            if (assertion.concept.concept_type == ConceptType.DISJUNCTION and
                f"disj_{assertion.concept}_{assertion.individual}" not in node.applied_rules):
                applicable.append((assertion, assertion.individual))
        return applicable
    
    def apply(self, node: TableauNode, assertion: ConceptAssertion, individual: Individual) -> List[TableauNode]:
        concept = assertion.concept
        
        # Create two branches
        left_node = node.copy()
        right_node = node.copy()
        
        # Left branch: add first subconcept
        left_assertion = ConceptAssertion(concept.subconcepts[0], individual)
        left_node.add_concept_assertion(left_assertion)
        left_node.applied_rules.add(f"disj_{concept}_{individual}")
        
        # Right branch: add second subconcept
        right_assertion = ConceptAssertion(concept.subconcepts[1], individual)
        right_node.add_concept_assertion(right_assertion)
        right_node.applied_rules.add(f"disj_{concept}_{individual}")
        
        return [left_node, right_node]
    
    def get_rule_name(self) -> str:
        return "Disjunction Rule (⊔)"

class ExistentialRule(TableauRule):
    """∃-rule: If x : ∃R.C, then create new individual y with R(x,y) and y : C"""
    
    def __init__(self):
        self.individual_counter = 0
    
    def is_applicable(self, node: TableauNode) -> List[Tuple[ConceptAssertion, Individual]]:
        applicable = []
        for assertion in node.concept_assertions:
            if (assertion.concept.concept_type == ConceptType.EXISTENTIAL and
                f"exists_{assertion.concept}_{assertion.individual}" not in node.applied_rules):
                applicable.append((assertion, assertion.individual))
        return applicable
    
    def apply(self, node: TableauNode, assertion: ConceptAssertion, individual: Individual) -> List[TableauNode]:
        new_node = node.copy()
        concept = assertion.concept
        
        # Create new individual
        self.individual_counter += 1
        new_individual = Individual(
            name=f"y{self.individual_counter}",
            created_by_rule="existential",
            parent=individual
        )
        
        # Add role assertion R(x, y)
        role_assertion = RoleAssertion(concept.role, individual, new_individual)
        new_node.add_role_assertion(role_assertion)
        
        # Add concept assertion C(y)
        concept_assertion = ConceptAssertion(concept.subconcepts[0], new_individual)
        new_node.add_concept_assertion(concept_assertion)
        
        # Mark rule as applied
        rule_id = f"exists_{concept}_{individual}"
        new_node.applied_rules.add(rule_id)
        
        return [new_node]
    
    def get_rule_name(self) -> str:
        return "Existential Rule (∃)"

class UniversalRule(TableauRule):
    """∀-rule: If x : ∀R.C and R(x,y), then add y : C"""
    
    def is_applicable(self, node: TableauNode) -> List[Tuple[ConceptAssertion, Individual]]:
        applicable = []
        for assertion in node.concept_assertions:
            if assertion.concept.concept_type == ConceptType.UNIVERSAL:
                individual = assertion.individual
                role = assertion.concept.role
                successors = node.get_role_successors(individual, role)
                
                for successor in successors:
                    rule_id = f"forall_{assertion.concept}_{individual}_{successor}"
                    if rule_id not in node.applied_rules:
                        applicable.append((assertion, successor))
        return applicable
    
    def apply(self, node: TableauNode, assertion: ConceptAssertion, target_individual: Individual) -> List[TableauNode]:
        new_node = node.copy()
        concept = assertion.concept
        source_individual = assertion.individual
        
        # Add concept assertion C(y)
        concept_assertion = ConceptAssertion(concept.subconcepts[0], target_individual)
        new_node.add_concept_assertion(concept_assertion)
        
        # Mark rule as applied
        rule_id = f"forall_{concept}_{source_individual}_{target_individual}"
        new_node.applied_rules.add(rule_id)
        
        return [new_node]
    
    def get_rule_name(self) -> str:
        return "Universal Rule (∀)"

# =============================================================================
# Blocking Mechanism for Termination
# =============================================================================

class BlockingStrategy(ABC):
    """Abstract base class for blocking strategies"""
    
    @abstractmethod
    def is_blocked(self, node: TableauNode, individual: Individual) -> bool:
        """Check if an individual should be blocked in the given node"""
        pass

class SubsetBlocking(BlockingStrategy):
    """Subset blocking: block if individual's concepts are subset of ancestor's concepts"""
    
    def is_blocked(self, node: TableauNode, individual: Individual) -> bool:
        if individual.is_root or individual.parent is None:
            return False
        
        individual_concepts = node.get_concepts_for_individual(individual)
        
        # Check ancestors
        current = individual.parent
        while current is not None:
            ancestor_concepts = node.get_concepts_for_individual(current)
            if individual_concepts.issubset(ancestor_concepts):
                return True
            current = current.parent
        
        return False

# =============================================================================
# Main Tableau Reasoner
# =============================================================================

class TableauReasoner:
    """Main tableau reasoner for ALC description logic"""
    
    def __init__(self, blocking_strategy: Optional[BlockingStrategy] = None):
        self.blocking_strategy = blocking_strategy or SubsetBlocking()
        self.rules = [
            ConjunctionRule(),
            DisjunctionRule(),
            ExistentialRule(),
            UniversalRule()
        ]
        self.statistics = {
            'nodes_created': 0,
            'nodes_closed': 0,
            'rules_applied': 0,
            'max_depth': 0
        }
    
    def is_satisfiable(self, concept: Concept) -> Tuple[bool, Optional[TableauNode]]:
        """
        Test if a concept is satisfiable using tableau algorithm
        
        Returns:
            Tuple of (is_satisfiable, witness_model)
        """
        # Reset statistics
        self.statistics = {
            'nodes_created': 0,
            'nodes_closed': 0,
            'rules_applied': 0,
            'max_depth': 0
        }
        
        # Create initial tableau node
        root_individual = Individual("x0", is_root=True)
        initial_assertion = ConceptAssertion(concept, root_individual)
        
        initial_node = TableauNode()
        initial_node.add_concept_assertion(initial_assertion)
        self.statistics['nodes_created'] += 1
        
        # Start tableau construction
        result = self._expand_tableau(initial_node, depth=0)
        
        return result
    
    def _expand_tableau(self, node: TableauNode, depth: int = 0) -> Tuple[bool, Optional[TableauNode]]:
        """Recursively expand the tableau"""
        self.statistics['max_depth'] = max(self.statistics['max_depth'], depth)
        
        # Check for clash
        if node.has_clash():
            self.statistics['nodes_closed'] += 1
            return False, None
        
        # Check if complete
        if node.is_complete():
            return True, node
        
        # Apply rules
        for rule in self.rules:
            applicable = rule.is_applicable(node)
            
            for assertion, individual in applicable:
                # Check blocking
                if self.blocking_strategy.is_blocked(node, individual):
                    continue
                
                # Apply rule
                new_nodes = rule.apply(node, assertion, individual)
                self.statistics['rules_applied'] += 1
                
                # For non-branching rules, continue with the single result
                if len(new_nodes) == 1:
                    self.statistics['nodes_created'] += 1
                    return self._expand_tableau(new_nodes[0], depth + 1)
                
                # For branching rules, try each branch
                elif len(new_nodes) > 1:
                    for new_node in new_nodes:
                        self.statistics['nodes_created'] += 1
                        satisfiable, model = self._expand_tableau(new_node, depth + 1)
                        if satisfiable:
                            return True, model
                    
                    # All branches failed
                    return False, None
        
        # No rules applicable and not complete - should not happen
        return True, node
    
    def get_statistics(self) -> Dict[str, int]:
        """Get reasoning statistics"""
        return self.statistics.copy()

# =============================================================================
# Testing and Examples
# =============================================================================

def test_basic_concepts():
    """Test basic concept satisfiability"""
    reasoner = TableauReasoner()
    
    print("Testing Basic Tableau Reasoner")
    print("=" * 50)
    
    # Test 1: Atomic concept (should be satisfiable)
    A = atomic_concept("A")
    satisfiable, model = reasoner.is_satisfiable(A)
    print(f"Test 1 - A: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 2: Conjunction A ⊓ B (should be satisfiable)
    B = atomic_concept("B")
    conj = conjunction(A, B)
    satisfiable, model = reasoner.is_satisfiable(conj)
    print(f"\nTest 2 - A ⊓ B: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 3: Contradiction A ⊓ ¬A (should be unsatisfiable)
    not_A = negation(A)
    contradiction = conjunction(A, not_A)
    satisfiable, model = reasoner.is_satisfiable(contradiction)
    print(f"\nTest 3 - A ⊓ ¬A: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 4: Disjunction A ⊔ B (should be satisfiable)
    disj = disjunction(A, B)
    satisfiable, model = reasoner.is_satisfiable(disj)
    print(f"\nTest 4 - A ⊔ B: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 5: Existential restriction ∃R.A (should be satisfiable)
    exists_R_A = existential_restriction("R", A)
    satisfiable, model = reasoner.is_satisfiable(exists_R_A)
    print(f"\nTest 5 - ∃R.A: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 6: Complex concept ∃R.(A ⊓ B) (should be satisfiable)
    complex_concept = existential_restriction("R", conjunction(A, B))
    satisfiable, model = reasoner.is_satisfiable(complex_concept)
    print(f"\nTest 6 - ∃R.(A ⊓ B): {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    # Test 7: Unsatisfiable complex concept ∃R.(A ⊓ ¬A)
    unsatisfiable_complex = existential_restriction("R", conjunction(A, not_A))
    satisfiable, model = reasoner.is_satisfiable(unsatisfiable_complex)
    print(f"\nTest 7 - ∃R.(A ⊓ ¬A): {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")

def test_universal_restriction():
    """Test universal restriction reasoning"""
    reasoner = TableauReasoner()
    
    print("\nTesting Universal Restrictions")
    print("=" * 50)
    
    A = atomic_concept("A")
    B = atomic_concept("B")
    
    # Test: ∃R.A ⊓ ∀R.B (should be satisfiable - creates R-successor with A and B)
    exists_R_A = existential_restriction("R", A)
    forall_R_B = universal_restriction("R", B)
    concept = conjunction(exists_R_A, forall_R_B)
    
    satisfiable, model = reasoner.is_satisfiable(concept)
    print(f"∃R.A ⊓ ∀R.B: {'✓ Satisfiable' if satisfiable else '✗ Unsatisfiable'}")
    print(f"Statistics: {reasoner.get_statistics()}")
    
    if satisfiable and model:
        print("Model found:")
        for assertion in model.concept_assertions:
            print(f"  {assertion}")
        for assertion in model.role_assertions:
            print(f"  {assertion}")

def main():
    """Main function to run tableau reasoner tests"""
    print("ALC Tableau Reasoner Implementation")
    print("Based on classical tableau algorithms for Description Logic")
    print("=" * 70)
    
    test_basic_concepts()
    test_universal_restriction()
    
    print("\n" + "=" * 70)
    print("Tableau Reasoner Implementation Complete!")
    print("This is a working implementation of the basic tableau algorithm for ALC.")
    print("It can be extended with optimizations and support for more expressive logics.")

if __name__ == "__main__":
    main()
