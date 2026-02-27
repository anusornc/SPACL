#!/usr/bin/env python3
"""
Generate large synthetic OWL ontologies with disjunctions for benchmarking.

These ontologies are designed to test SPACL's parallel performance on
disjunctive reasoning (A ⊔ B), where speculative parallelism provides benefits.

Usage:
    python3 generate_disjunctive_ontology.py <size> <output_file>
    
Example:
    python3 generate_disjunctive_ontology.py 50000 output/ontology_50k.owl
"""

import sys
import random
from datetime import datetime

def generate_disjunctive_ontology(num_classes, output_file):
    """
    Generate an OWL ontology with:
    - Linear hierarchy: C0 ⊑ C1 ⊑ C2 ⊑ ... ⊑ Cn
    - Disjunctions: Each class has alternatives (A ⊔ B)
    - Complex expressions: Mix of unions, intersections, complements
    """
    
    print(f"Generating disjunctive ontology with {num_classes} classes...")
    
    with open(output_file, 'w') as f:
        # XML header
        f.write('<?xml version="1.0"?>\n')
        f.write('<rdf:RDF xmlns="http://example.org/disjunctive#"\n')
        f.write('     xml:base="http://example.org/disjunctive"\n')
        f.write('     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"\n')
        f.write('     xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"\n')
        f.write('     xmlns:owl="http://www.w3.org/2002/07/owl#">\n')
        f.write('\n')
        f.write('<owl:Ontology rdf:about="">\n')
        f.write(f'  <rdfs:comment>Disjunctive benchmark ontology with {num_classes} classes</rdfs:comment>\n')
        f.write(f'  <rdfs:label>Disjunctive_{num_classes}</rdfs:label>\n')
        f.write('</owl:Ontology>\n')
        f.write('\n')
        
        # Generate base classes
        print("  Generating base classes...")
        for i in range(num_classes):
            class_name = f"Class{i:05d}"
            f.write(f'<owl:Class rdf:ID="{class_name}" />\n')
        
        f.write('\n')
        
        # Generate subclass axioms (linear hierarchy for 50%)
        print("  Generating hierarchy axioms...")
        hierarchy_count = num_classes // 2
        for i in range(hierarchy_count):
            subclass = f"Class{i:05d}"
            superclass = f"Class{(i + 1) % num_classes:05d}"
            f.write(f'<owl:Class rdf:about="#{subclass}">\n')
            f.write(f'  <rdfs:subClassOf rdf:resource="#{superclass}" />\n')
            f.write('</owl:Class>\n')
        
        f.write('\n')
        
        # Generate disjunctions (unions) - these enable parallel reasoning
        print("  Generating disjunctions (unions)...")
        disjunction_count = num_classes // 5  # 20% have disjunctions
        random.seed(42)  # Reproducible
        
        for i in range(disjunction_count):
            class_name = f"Class{i:05d}"
            # Create union: A ⊑ (B ⊔ C)
            operand1 = f"Class{random.randint(0, num_classes-1):05d}"
            operand2 = f"Class{random.randint(0, num_classes-1):05d}"
            
            f.write(f'<owl:Class rdf:about="#{class_name}">\n')
            f.write('  <rdfs:subClassOf>\n')
            f.write('    <owl:Class>\n')
            f.write('      <owl:unionOf rdf:parseType="Collection">\n')
            f.write(f'        <owl:Class rdf:about="#{operand1}" />\n')
            f.write(f'        <owl:Class rdf:about="#{operand2}" />\n')
            f.write('      </owl:unionOf>\n')
            f.write('    </owl:Class>\n')
            f.write('  </rdfs:subClassOf>\n')
            f.write('</owl:Class>\n')
        
        f.write('\n')
        
        # Generate intersections
        print("  Generating intersections...")
        intersection_count = num_classes // 10  # 10% have intersections
        
        for i in range(intersection_count):
            class_name = f"Class{(i + disjunction_count):05d}"
            operand1 = f"Class{random.randint(0, num_classes-1):05d}"
            operand2 = f"Class{random.randint(0, num_classes-1):05d}"
            
            f.write(f'<owl:Class rdf:about="#{class_name}">\n')
            f.write('  <rdfs:subClassOf>\n')
            f.write('    <owl:Class>\n')
            f.write('      <owl:intersectionOf rdf:parseType="Collection">\n')
            f.write(f'        <owl:Class rdf:about="#{operand1}" />\n')
            f.write(f'        <owl:Class rdf:about="#{operand2}" />\n')
            f.write('      </owl:intersectionOf>\n')
            f.write('    </owl:Class>\n')
            f.write('  </rdfs:subClassOf>\n')
            f.write('</owl:Class>\n')
        
        f.write('\n')
        f.write('</rdf:RDF>\n')
    
    print(f"✓ Generated: {output_file}")
    print(f"  - Classes: {num_classes}")
    print(f"  - Hierarchy axioms: {hierarchy_count}")
    print(f"  - Disjunctions (unions): {disjunction_count}")
    print(f"  - Intersections: {intersection_count}")
    
    return output_file

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python3 generate_disjunctive_ontology.py <num_classes> <output_file>")
        print("Example: python3 generate_disjunctive_ontology.py 50000 disjunctive_50k.owl")
        sys.exit(1)
    
    num_classes = int(sys.argv[1])
    output_file = sys.argv[2]
    
    generate_disjunctive_ontology(num_classes, output_file)
