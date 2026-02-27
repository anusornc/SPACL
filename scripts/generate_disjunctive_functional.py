#!/usr/bin/env python3
"""
Generate disjunctive ontologies in OWL Functional Syntax (faster parsing).
"""

import sys
import random

def generate_functional_ontology(num_classes, output_file):
    """Generate OWL Functional Syntax ontology with disjunctions."""
    
    print(f"Generating {num_classes} classes in OWL Functional Syntax...")
    
    with open(output_file, 'w') as f:
        # Header
        f.write('Prefix(:=<http://example.org/disjunctive#>)\n')
        f.write('Prefix(owl:=<http://www.w3.org/2002/07/owl#>)\n')
        f.write('Prefix(rdf:=<http://www.w3.org/1999/02/22-rdf-syntax-ns#>)\n')
        f.write('Prefix(xml:=<http://www.w3.org/XML/1998/namespace>)\n')
        f.write('Prefix(xsd:=<http://www.w3.org/2001/XMLSchema#>)\n')
        f.write('Prefix(rdfs:=<http://www.w3.org/2000/01/rdf-schema#>)\n\n')
        
        f.write('Ontology(<http://example.org/disjunctive>\n\n')
        
        # Declare classes
        print("  Declaring classes...")
        for i in range(num_classes):
            f.write(f'  Declaration(Class(:Class{i:05d}))\n')
        
        f.write('\n')
        
        # Subclass axioms (hierarchy)
        print("  Adding hierarchy...")
        hierarchy = num_classes // 2
        for i in range(hierarchy):
            parent = (i + 1) % num_classes
            f.write(f'  SubClassOf(:Class{i:05d} :Class{parent:05d})\n')
        
        f.write('\n')
        
        # Disjunctions (unions) - enable parallel reasoning
        print("  Adding disjunctions...")
        random.seed(42)
        disjunctions = num_classes // 5
        
        for i in range(disjunctions):
            class_name = f"Class{i:05d}"
            op1 = f"Class{random.randint(0, num_classes-1):05d}"
            op2 = f"Class{random.randint(0, num_classes-1):05d}"
            # A ⊑ (B ⊔ C)
            f.write(f'  SubClassOf(:{class_name} ObjectUnionOf(:{op1} :{op2}))\n')
        
        f.write('\n')
        
        # Intersections
        print("  Adding intersections...")
        intersections = num_classes // 10
        
        for i in range(intersections):
            class_name = f"Class{(i + disjunctions) % num_classes:05d}"
            op1 = f"Class{random.randint(0, num_classes-1):05d}"
            op2 = f"Class{random.randint(0, num_classes-1):05d}"
            # A ⊑ (B ⊓ C)
            f.write(f'  SubClassOf(:{class_name} ObjectIntersectionOf(:{op1} :{op2}))\n')
        
        f.write(')\n')
    
    print(f"✓ Generated: {output_file}")
    print(f"  - Classes: {num_classes}")
    print(f"  - Hierarchy: {hierarchy}")
    print(f"  - Disjunctions: {disjunctions}")
    print(f"  - Intersections: {intersections}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python3 generate_disjunctive_functional.py <num_classes> <output_file>")
        sys.exit(1)
    
    num_classes = int(sys.argv[1])
    output_file = sys.argv[2]
    generate_functional_ontology(num_classes, output_file)
