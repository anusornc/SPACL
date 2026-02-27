#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test disjunctive ontologies with progress output

use std::path::Path;
use std::time::Instant;
use owl2_reasoner::{Ontology, SimpleReasoner, SpeculativeTableauxReasoner, OwlReasoner};
use owl2_reasoner::parser::{RdfXmlParser, OntologyParser};

fn log(msg: &str) {
    println!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg);
}

fn test_ontology(name: &str, path: &str) {
    log(&format!("═══════════════════════════════════════════════════"));
    log(&format!("Testing: {}", name));
    
    let path = Path::new(path);
    if !path.exists() {
        log(&format!("✗ File not found: {}", path.display()));
        return;
    }
    
    // Load and parse
    log("Loading and parsing...");
    let start = Instant::now();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log(&format!("✗ Error reading: {:?}", e));
            return;
        }
    };
    
    let parser = RdfXmlParser::new();
    let ont: Ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            log(&format!("✗ Error parsing: {:?}", e));
            return;
        }
    };
    let parse_time = start.elapsed();
    
    let class_count = ont.classes().len();
    let axiom_count = ont.axioms().len();
    
    log(&format!("✓ Parsed: {} classes, {} axioms in {:?}", 
        class_count, axiom_count, parse_time));
    
    // Sequential reasoning
    log("Running SEQUENTIAL reasoning...");
    let start = Instant::now();
    let mut seq = SimpleReasoner::new(ont.clone());
    let seq_result = seq.is_consistent();
    let seq_time = start.elapsed();
    log(&format!("✓ Sequential: {:?} - {:?}", seq_time, seq_result));
    
    // SPACL reasoning
    log("Running SPACL reasoning...");
    let start = Instant::now();
    let mut spacl = SpeculativeTableauxReasoner::new(ont.clone());
    let spacl_result = spacl.is_consistent();
    let spacl_time = start.elapsed();
    log(&format!("✓ SPACL: {:?} - {:?}", spacl_time, spacl_result));
    
    // Speedup
    if spacl_time.as_nanos() > 0 {
        let speedup = seq_time.as_nanos() as f64 / spacl_time.as_nanos() as f64;
        log(&format!("⚡ Speedup: {:.2}x", speedup));
        
        if speedup > 1.0 {
            log("✅ SPACL IS FASTER!");
        } else {
            log("⚠️  SPACL is slower (overhead > benefit)");
        }
    }
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Disjunctive Ontology Test                                  ║");
    println!("║     (These should show SPACL speedup > 1.0x)                   ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    
    test_ontology("Disjunctive_10K", "benchmarks/ontologies/disjunctive/disjunctive_10k.owl");
    test_ontology("Disjunctive_30K", "benchmarks/ontologies/disjunctive/disjunctive_30k.owl");
    test_ontology("Disjunctive_50K", "benchmarks/ontologies/disjunctive/disjunctive_50k.owl");
    
    log("");
    log("Test complete!");
}
