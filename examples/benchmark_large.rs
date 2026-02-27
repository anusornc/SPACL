#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Large ontology benchmark for overnight testing
use owl2_reasoner::{SimpleReasoner, SpeculativeConfig, SpeculativeTableauxReasoner};
use std::env;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: benchmark_large <ontology.owl>");
        std::process::exit(1);
    }

    let path = &args[1];
    println!("Loading: {}", path);

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            std::process::exit(1);
        }
    };

    let parser = match owl2_reasoner::ParserFactory::auto_detect(&content) {
        Some(p) => p,
        None => {
            eprintln!("Could not detect parser");
            std::process::exit(1);
        }
    };

    let ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Parse error: {:?}", e);
            std::process::exit(1);
        }
    };

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    println!("========================================");
    println!("Ontology: {}", path);
    println!("Classes: {}", class_count);
    println!("Axioms: {}", axiom_count);
    println!("========================================");

    // Determine threshold based on size
    let threshold = if axiom_count < 100 {
        1000 // Never parallel for tiny ontologies
    } else if axiom_count < 1000 {
        500
    } else {
        100
    };

    // Sequential benchmark
    println!("\n--- Sequential ---");
    let start = Instant::now();
    let seq = SimpleReasoner::new(ontology.clone());
    let seq_result = match seq.is_consistent() {
        Ok(r) => r,
        Err(e) => {
            println!("Sequential error: {:?}", e);
            false
        }
    };
    let seq_time = start.elapsed();
    println!("Result: {} in {:?}", seq_result, seq_time);

    // SPACL benchmark (adaptive)
    println!("\n--- SPACL (threshold={}) ---", threshold);
    let start = Instant::now();
    let mut config = SpeculativeConfig::default();
    config.parallel_threshold = threshold;
    let mut spacl = SpeculativeTableauxReasoner::with_config(ontology.clone(), config);
    let spacl_result = match spacl.is_consistent() {
        Ok(r) => r,
        Err(e) => {
            println!("SPACL error: {:?}", e);
            false
        }
    };
    let spacl_time = start.elapsed();
    let stats = spacl.get_stats();

    println!("Result: {} in {:?}", spacl_result, spacl_time);
    println!("Branches created: {}", stats.branches_created);
    println!("Branches pruned: {}", stats.branches_pruned);
    println!("Nogoods learned: {}", stats.nogoods_learned);
    println!("Nogood hits: {}", stats.nogood_hits);

    if seq_result == spacl_result {
        println!("✓ Results match!");
    } else {
        println!("✗ RESULT MISMATCH!");
    }

    // Calculate speedup
    let seq_ms = seq_time.as_millis() as f64;
    let spacl_ms = spacl_time.as_millis() as f64;

    if spacl_ms > 0.0 {
        let speedup = seq_ms / spacl_ms;
        let overhead = if speedup < 1.0 {
            format!("{:.1}x overhead", 1.0 / speedup)
        } else {
            format!("{:.2}x speedup", speedup)
        };
        println!("\nPerformance: {}", overhead);

        // JSON output for parsing
        println!("\n{{");
        println!("  \"ontology\": \"{}\",", path);
        println!("  \"classes\": {},", class_count);
        println!("  \"axioms\": {},", axiom_count);
        println!("  \"threshold\": {},", threshold);
        println!("  \"sequential_ms\": {:.2},", seq_ms);
        println!("  \"spacl_ms\": {:.2},", spacl_ms);
        println!("  \"speedup\": {:.3},", speedup);
        println!("  \"branches_created\": {},", stats.branches_created);
        println!("  \"branches_pruned\": {},", stats.branches_pruned);
        println!("  \"nogoods_learned\": {}", stats.nogoods_learned);
        println!("}}");
    }

    println!("\n========================================");
}
