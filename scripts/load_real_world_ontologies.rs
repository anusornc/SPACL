#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Load real-world ontologies and collect statistics

use std::path::Path;
use std::time::Instant;

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Real-World Ontology Loading Test                          ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");
    
    let ontologies = vec![
        ("LUBM", "tests/data/univ-bench.owl", "Small test"),
        ("PATO", "benchmarks/ontologies/other/pato.owl", "~3k classes"),
        ("DOID", "benchmarks/ontologies/other/doid.owl", "~15k classes"),
        // ("UBERON", "benchmarks/ontologies/other/uberon.owl", "~15k classes"),
        // ("GO_Basic", "benchmarks/ontologies/other/go-basic.owl", "~45k classes"),
        // Skip ChEBI - too large (773MB)
    ];
    
    for (name, path, desc) in ontologies {
        println!("\nTesting {} ({})...", name, desc);
        
        let path = Path::new(path);
        if !path.exists() {
            println!("  ✗ File not found: {}", path.display());
            continue;
        }
        
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!("  File size: {:.2} MB", file_size as f64 / 1024.0 / 1024.0);
        
        let start = Instant::now();
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let read_time = start.elapsed();
                println!("  Read time: {:?}", read_time);
                
                // Count basic patterns
                let class_count = content.matches("<owl:Class").count();
                let subclass_count = content.matches("rdfs:subClassOf").count();
                println!("  owl:Class occurrences: {}", class_count);
                println!("  subClassOf occurrences: {}", subclass_count);
            }
            Err(e) => {
                println!("  ✗ Error reading: {:?}", e);
            }
        }
    }
    
    println!("\n✓ Test complete");
}
