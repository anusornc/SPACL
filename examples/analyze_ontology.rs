#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Example: Analyze ontology characteristics
//!
//! This example shows how to use the OntologyCharacteristics analyzer
//! to determine the optimal reasoning strategy for an ontology.

use owl2_reasoner::{Ontology, OntologyCharacteristics, ParserFactory, ReasoningStrategy};
use std::env;
use std::path::Path;

fn main() {
    // Get ontology path from command line or use default
    let args: Vec<String> = env::args().collect();
    let ontology_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("benchmarks/ontologies/other/go-basic.owl");

    println!("========================================");
    println!("  Ontology Structure Analysis Example");
    println!("========================================\n");

    // Load ontology
    let path = Path::new(ontology_path);
    if !path.exists() {
        eprintln!("Error: Ontology file not found: {}", ontology_path);
        eprintln!("Usage: cargo run --example analyze_ontology -- <path-to-ontology>");
        std::process::exit(1);
    }

    println!("Loading ontology: {}\n", ontology_path);

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    // Parse ontology
    let parser = match ParserFactory::auto_detect(&content) {
        Some(p) => p,
        None => {
            eprintln!("Error: Could not detect ontology format");
            std::process::exit(1);
        }
    };

    let ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error parsing ontology: {:?}", e);
            std::process::exit(1);
        }
    };

    println!("✓ Ontology loaded successfully\n");

    // Analyze characteristics
    println!("Analyzing ontology structure...\n");
    let chars = OntologyCharacteristics::analyze(&ontology);

    // Print results
    println!("========================================");
    println!("  Analysis Results");
    println!("========================================\n");

    println!("📊 Basic Statistics:");
    println!("  • Classes:          {}", chars.class_count);
    println!("  • Object Properties: {}", chars.property_count);
    println!("  • Individuals:      {}", chars.individual_count);
    println!();

    println!("🔍 Complexity Analysis:");
    println!("  • Disjunctions:     {}", chars.disjunction_count);
    println!(
        "  • Complex Expressions: {}",
        chars.complex_expression_count
    );
    println!(
        "  • Disjointness Axioms: {}",
        chars.disjointness_axiom_count
    );
    println!("  • Equivalence Axioms:  {}", chars.equivalence_axiom_count);
    println!("  • Max Expression Depth: {}", chars.max_expression_depth);
    println!();

    println!("🏗️  Hierarchy Structure:");
    println!("  • Estimated Depth:  {}", chars.hierarchy_depth);
    println!(
        "  • Tree-like:        {}",
        if chars.is_tree_like {
            "✓ Yes"
        } else {
            "✗ No"
        }
    );
    println!();

    println!("📈 Complexity Assessment:");
    println!("  • Complexity Score: {:.2}/1.0", chars.complexity_score);
    println!("  • Description:      {}", chars.complexity_description());
    println!();

    println!("🎯 Recommendations:");
    println!(
        "  • Fast Path Eligible: {}",
        if chars.can_use_fast_path() {
            "✓ Yes"
        } else {
            "✗ No"
        }
    );
    println!(
        "  • Is Small:         {}",
        if chars.is_small() {
            "✓ Yes"
        } else {
            "✗ No"
        }
    );
    println!("  • Strategy:         {:?}", chars.recommended_strategy);
    println!(
        "  • Est. Time:        ~{}ms",
        chars.estimated_classification_time_ms()
    );
    println!();

    println!("========================================");
    println!("  Summary");
    println!("========================================");

    match chars.recommended_strategy {
        ReasoningStrategy::Hierarchical => {
            println!("✓ This ontology can use the FAST hierarchical path!");
            println!("  Expected classification time: < 1 second");
        }
        ReasoningStrategy::BatchIncremental => {
            println!("⚠ This ontology requires batch incremental processing.");
            println!("  Processing will occur in chunks with progress reporting.");
        }
        ReasoningStrategy::SpeculativeParallel => {
            println!("⚡ This ontology has complex disjunctions.");
            println!("  Speculative parallelism (SPACL) is recommended.");
        }
        ReasoningStrategy::Sequential => {
            println!("ℹ Sequential processing recommended.");
            println!("  Suitable for debugging or very small ontologies.");
        }
    }

    println!();
}
