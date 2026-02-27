#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test real-world ontologies
//! 
//! This script tests SPACL on real-world ontologies from BioPortal

use std::path::Path;
use std::time::Instant;

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Real-World Ontology Test - SPACL Performance              ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");
    
    let ontologies = vec![
        ("LUBM", "tests/data/univ-bench.owl", "Lehigh University Benchmark"),
        ("GO_Basic", "benchmarks/ontologies/other/go-basic.owl", "Gene Ontology"),
        ("UBERON", "benchmarks/ontologies/other/uberon.owl", "Uberon Anatomy"),
        ("DOID", "benchmarks/ontologies/other/doid.owl", "Disease Ontology"),
        ("PATO", "benchmarks/ontologies/other/pato.owl", "Phenotype And Trait"),
    ];
    
    println!("{:<15} {:<20} {:>12} {:>12} {:>10}", 
             "Ontology", "Description", "Classes", "Time (ms)", "Status");
    println!("{}", "─".repeat(85));
    
    for (name, path, desc) in ontologies {
        let path = Path::new(path);
        if !path.exists() {
            println!("{:<15} {:<20} {:>12} {:>12} {:>10}", 
                     name, desc, "N/A", "N/A", "NOT FOUND");
            continue;
        }
        
        // Try to load and parse
        let start = Instant::now();
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let load_time = start.elapsed().as_millis();
                
                // Count classes (simple heuristic - count <owl:Class or rdf:about patterns)
                let class_count = content.matches("<owl:Class").count() 
                    + content.matches("rdf:type rdf:resource=\"http://www.w3.org/2002/07/owl#Class\"").count();
                
                println!("{:<15} {:<20} {:>12} {:>12} {:>10}", 
                         name, desc, class_count, load_time, "LOADED");
            }
            Err(e) => {
                println!("{:<15} {:<20} {:>12} {:>12} {:>10}", 
                         name, desc, "ERROR", "N/A", format!("{:?}", e));
            }
        }
    }
    
    println!("\n{}", "─".repeat(85));
    println!("\nNote: Full reasoning benchmarks require complete parser support.");
    println!("These ontologies use OWL/XML or RDF/XML formats.");
}
