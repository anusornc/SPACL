#!/usr/bin/env python3
"""Generate large test ontologies for benchmarking"""
import sys
from pathlib import Path

def generate_hierarchy(n, output_path):
    """Generate a class hierarchy ontology with n classes"""
    lines = [
        "Prefix(:=<http://example.org/>)",
        "Prefix(owl:=<http://www.w3.org/2002/07/owl#>)",
        "Prefix(rdf:=<http://www.w3.org/1999/02/22-rdf-syntax-ns#>)",
        "Prefix(xml:=<http://www.w3.org/XML/1998/namespace>)",
        "Prefix(xsd:=<http://www.w3.org/2001/XMLSchema#>)",
        "Prefix(rdfs:=<http://www.w3.org/2000/01/rdf-schema#>)",
        "",
        f"Ontology(<http://example.org/hierarchy{n}>",
        "",
        "Declaration(Class(:Root))",
    ]
    
    # Declare all classes
    for i in range(n):
        lines.append(f"Declaration(Class(:C{i}))")
    
    lines.append("")
    
    # Create subclass relationships (chain)
    lines.append("SubClassOf(:C0 :Root)")
    for i in range(1, n):
        lines.append(f"SubClassOf(:C{i} :C{i-1})")
    
    lines.append(")")
    lines.append("")
    
    with open(output_path, 'w') as f:
        f.write('\n'.join(lines))
    
    print(f"Generated {n}-class hierarchy: {output_path}")

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <num_classes> <output_file>")
        sys.exit(1)
    
    n = int(sys.argv[1])
    output = sys.argv[2]
    generate_hierarchy(n, output)
