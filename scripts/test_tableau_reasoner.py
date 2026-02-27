#!/usr/bin/env python3
"""
Comprehensive Test Suite for Tableau Reasoner
=============================================

This test suite validates the tableau reasoner implementation against
known satisfiable and unsatisfiable concepts from description logic literature.
"""

import time
from tableau_reasoner import *

class TableauTestSuite:
    """Comprehensive test suite for tableau reasoner"""
    
    def __init__(self):
        self.reasoner = TableauReasoner()
        self.test_results = []
        self.total_tests = 0
        self.passed_tests = 0
    
    def run_test(self, test_name: str, concept: Concept, expected_satisfiable: bool, description: str = ""):
        """Run a single test case"""
        self.total_tests += 1
        print(f"\nTest {self.total_tests}: {test_name}")
        print(f"Concept: {concept}")
        if description:
            print(f"Description: {description}")
        
        start_time = time.time()
        satisfiable, model = self.reasoner.is_satisfiable(concept)
        end_time = time.time()
        
        execution_time = (end_time - start_time) * 1000  # Convert to milliseconds
        stats = self.reasoner.get_statistics()
        
        # Check if result matches expectation
        passed = satisfiable == expected_satisfiable
        if passed:
            self.passed_tests += 1
            status = "✓ PASS"
        else:
            status = "✗ FAIL"
        
        result_str = "Satisfiable" if satisfiable else "Unsatisfiable"
        expected_str = "Satisfiable" if expected_satisfiable else "Unsatisfiable"
        
        print(f"Result: {result_str} (Expected: {expected_str}) - {status}")
        print(f"Time: {execution_time:.2f}ms")
        print(f"Statistics: Nodes={stats['nodes_created']}, Rules={stats['rules_applied']}, Depth={stats['max_depth']}")
        
        if satisfiable and model and len(model.concept_assertions) <= 10:
            print("Model (sample):")
            for assertion in list(model.concept_assertions)[:5]:
                print(f"  {assertion}")
            if len(model.role_assertions) > 0:
                for assertion in list(model.role_assertions)[:3]:
                    print(f"  {assertion}")
        
        self.test_results.append({
            'test_name': test_name,
            'concept': str(concept),
            'expected': expected_satisfiable,
            'actual': satisfiable,
            'passed': passed,
            'time_ms': execution_time,
            'statistics': stats
        })
        
        return passed
    
    def test_basic_concepts(self):
        """Test basic atomic and compound concepts"""
        print("=" * 60)
        print("BASIC CONCEPTS TESTS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        C = atomic_concept("C")
        
        # Atomic concepts
        self.run_test("Atomic Concept", A, True, "Single atomic concept should be satisfiable")
        
        # Top and Bottom
        self.run_test("Top Concept", top_concept(), True, "Universal concept ⊤ should be satisfiable")
        self.run_test("Bottom Concept", bottom_concept(), False, "Empty concept ⊥ should be unsatisfiable")
        
        # Simple conjunctions
        self.run_test("Simple Conjunction", conjunction(A, B), True, "A ⊓ B should be satisfiable")
        self.run_test("Triple Conjunction", conjunction(conjunction(A, B), C), True, "(A ⊓ B) ⊓ C should be satisfiable")
        
        # Simple disjunctions
        self.run_test("Simple Disjunction", disjunction(A, B), True, "A ⊔ B should be satisfiable")
        self.run_test("Disjunction with Bottom", disjunction(A, bottom_concept()), True, "A ⊔ ⊥ should be satisfiable")
        
        # Negations
        self.run_test("Simple Negation", negation(A), True, "¬A should be satisfiable")
        self.run_test("Double Negation", negation(negation(A)), True, "¬¬A should be satisfiable")
    
    def test_contradictions(self):
        """Test various forms of contradictions"""
        print("\n" + "=" * 60)
        print("CONTRADICTION TESTS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        
        # Direct contradictions
        self.run_test("Direct Contradiction", conjunction(A, negation(A)), False, "A ⊓ ¬A should be unsatisfiable")
        
        # Contradiction with Bottom
        self.run_test("Conjunction with Bottom", conjunction(A, bottom_concept()), False, "A ⊓ ⊥ should be unsatisfiable")
        
        # Complex contradictions
        complex_contradiction = conjunction(
            conjunction(A, B),
            negation(A)
        )
        self.run_test("Complex Contradiction", complex_contradiction, False, "(A ⊓ B) ⊓ ¬A should be unsatisfiable")
        
        # Nested contradictions
        nested_contradiction = conjunction(
            A,
            conjunction(B, negation(A))
        )
        self.run_test("Nested Contradiction", nested_contradiction, False, "A ⊓ (B ⊓ ¬A) should be unsatisfiable")
    
    def test_existential_restrictions(self):
        """Test existential restrictions"""
        print("\n" + "=" * 60)
        print("EXISTENTIAL RESTRICTION TESTS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        
        # Simple existential
        self.run_test("Simple Existential", existential_restriction("R", A), True, "∃R.A should be satisfiable")
        
        # Existential with conjunction
        exists_conj = existential_restriction("R", conjunction(A, B))
        self.run_test("Existential Conjunction", exists_conj, True, "∃R.(A ⊓ B) should be satisfiable")
        
        # Existential with contradiction
        exists_contradiction = existential_restriction("R", conjunction(A, negation(A)))
        self.run_test("Existential Contradiction", exists_contradiction, False, "∃R.(A ⊓ ¬A) should be unsatisfiable")
        
        # Multiple existentials
        multi_exists = conjunction(
            existential_restriction("R", A),
            existential_restriction("S", B)
        )
        self.run_test("Multiple Existentials", multi_exists, True, "∃R.A ⊓ ∃S.B should be satisfiable")
        
        # Nested existentials
        nested_exists = existential_restriction("R", existential_restriction("S", A))
        self.run_test("Nested Existentials", nested_exists, True, "∃R.∃S.A should be satisfiable")
    
    def test_universal_restrictions(self):
        """Test universal restrictions"""
        print("\n" + "=" * 60)
        print("UNIVERSAL RESTRICTION TESTS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        
        # Simple universal (should be satisfiable - no R-successors required)
        self.run_test("Simple Universal", universal_restriction("R", A), True, "∀R.A should be satisfiable")
        
        # Universal with existential (forces R-successor)
        univ_with_exists = conjunction(
            existential_restriction("R", top_concept()),
            universal_restriction("R", A)
        )
        self.run_test("Universal with Existential", univ_with_exists, True, "∃R.⊤ ⊓ ∀R.A should be satisfiable")
        
        # Universal contradiction
        univ_contradiction = conjunction(
            existential_restriction("R", A),
            universal_restriction("R", negation(A))
        )
        self.run_test("Universal Contradiction", univ_contradiction, False, "∃R.A ⊓ ∀R.¬A should be unsatisfiable")
        
        # Complex universal reasoning
        complex_univ = conjunction(
            existential_restriction("R", A),
            universal_restriction("R", conjunction(A, B))
        )
        self.run_test("Complex Universal", complex_univ, True, "∃R.A ⊓ ∀R.(A ⊓ B) should be satisfiable")
    
    def test_complex_combinations(self):
        """Test complex combinations of constructors"""
        print("\n" + "=" * 60)
        print("COMPLEX COMBINATION TESTS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        C = atomic_concept("C")
        
        # Disjunction of existentials
        disj_exists = disjunction(
            existential_restriction("R", A),
            existential_restriction("S", B)
        )
        self.run_test("Disjunction of Existentials", disj_exists, True, "∃R.A ⊔ ∃S.B should be satisfiable")
        
        # Existential of disjunction
        exists_disj = existential_restriction("R", disjunction(A, B))
        self.run_test("Existential of Disjunction", exists_disj, True, "∃R.(A ⊔ B) should be satisfiable")
        
        # Complex nesting
        complex_nested = conjunction(
            disjunction(A, existential_restriction("R", B)),
            universal_restriction("R", conjunction(B, C))
        )
        self.run_test("Complex Nesting", complex_nested, True, "(A ⊔ ∃R.B) ⊓ ∀R.(B ⊓ C) should be satisfiable")
        
        # Very complex concept
        very_complex = conjunction(
            conjunction(
                disjunction(A, negation(B)),
                existential_restriction("R", conjunction(B, C))
            ),
            universal_restriction("R", disjunction(negation(B), C))
        )
        self.run_test("Very Complex", very_complex, True, "Complex nested concept should be satisfiable")
    
    def test_known_unsatisfiable_patterns(self):
        """Test known unsatisfiable patterns from DL literature"""
        print("\n" + "=" * 60)
        print("KNOWN UNSATISFIABLE PATTERNS")
        print("=" * 60)
        
        A = atomic_concept("A")
        B = atomic_concept("B")
        
        # Pattern 1: ∃R.A ⊓ ∀R.¬A
        pattern1 = conjunction(
            existential_restriction("R", A),
            universal_restriction("R", negation(A))
        )
        self.run_test("Pattern 1", pattern1, False, "∃R.A ⊓ ∀R.¬A should be unsatisfiable")
        
        # Pattern 2: ∃R.(A ⊓ B) ⊓ ∀R.(¬A ⊔ ¬B)
        pattern2 = conjunction(
            existential_restriction("R", conjunction(A, B)),
            universal_restriction("R", disjunction(negation(A), negation(B)))
        )
        self.run_test("Pattern 2", pattern2, False, "∃R.(A ⊓ B) ⊓ ∀R.(¬A ⊔ ¬B) should be unsatisfiable")
        
        # Pattern 3: A ⊓ ∃R.B ⊓ ∀R.(¬A ⊓ ¬B)
        pattern3 = conjunction(
            conjunction(A, existential_restriction("R", B)),
            universal_restriction("R", conjunction(negation(A), negation(B)))
        )
        self.run_test("Pattern 3", pattern3, False, "A ⊓ ∃R.B ⊓ ∀R.(¬A ⊓ ¬B) should be unsatisfiable")
    
    def test_performance_scalability(self):
        """Test performance with increasingly complex concepts"""
        print("\n" + "=" * 60)
        print("PERFORMANCE AND SCALABILITY TESTS")
        print("=" * 60)
        
        # Test with increasing number of conjuncts
        concepts = [atomic_concept(f"A{i}") for i in range(10)]
        
        # Build increasingly large conjunctions
        for size in [2, 4, 6, 8]:
            large_conj = concepts[0]
            for i in range(1, size):
                large_conj = conjunction(large_conj, concepts[i])
            
            self.run_test(f"Large Conjunction ({size} terms)", large_conj, True, 
                         f"Conjunction of {size} atomic concepts")
        
        # Test with nested existentials
        nested = atomic_concept("A")
        for depth in range(1, 5):
            nested = existential_restriction(f"R{depth}", nested)
            self.run_test(f"Nested Existentials (depth {depth})", nested, True,
                         f"Existential nesting of depth {depth}")
    
    def run_all_tests(self):
        """Run all test suites"""
        print("TABLEAU REASONER COMPREHENSIVE TEST SUITE")
        print("=" * 70)
        print("Testing implementation against known DL satisfiability problems")
        
        start_time = time.time()
        
        self.test_basic_concepts()
        self.test_contradictions()
        self.test_existential_restrictions()
        self.test_universal_restrictions()
        self.test_complex_combinations()
        self.test_known_unsatisfiable_patterns()
        self.test_performance_scalability()
        
        end_time = time.time()
        total_time = end_time - start_time
        
        # Print summary
        print("\n" + "=" * 70)
        print("TEST SUITE SUMMARY")
        print("=" * 70)
        print(f"Total Tests: {self.total_tests}")
        print(f"Passed: {self.passed_tests}")
        print(f"Failed: {self.total_tests - self.passed_tests}")
        print(f"Success Rate: {(self.passed_tests / self.total_tests * 100):.1f}%")
        print(f"Total Time: {total_time:.2f} seconds")
        
        if self.passed_tests == self.total_tests:
            print("\n🎉 ALL TESTS PASSED! The tableau reasoner is working correctly.")
        else:
            print(f"\n⚠️  {self.total_tests - self.passed_tests} tests failed. Review implementation.")
        
        # Performance summary
        total_nodes = sum(result['statistics']['nodes_created'] for result in self.test_results)
        total_rules = sum(result['statistics']['rules_applied'] for result in self.test_results)
        avg_time = sum(result['time_ms'] for result in self.test_results) / len(self.test_results)
        
        print(f"\nPerformance Summary:")
        print(f"Total Nodes Created: {total_nodes}")
        print(f"Total Rules Applied: {total_rules}")
        print(f"Average Test Time: {avg_time:.2f}ms")
        
        return self.passed_tests == self.total_tests

def main():
    """Run the comprehensive test suite"""
    test_suite = TableauTestSuite()
    success = test_suite.run_all_tests()
    
    if success:
        print("\n✅ Tableau reasoner implementation validated successfully!")
        print("The reasoner correctly handles ALC description logic reasoning.")
    else:
        print("\n❌ Some tests failed. The implementation needs review.")
    
    return success

if __name__ == "__main__":
    main()
