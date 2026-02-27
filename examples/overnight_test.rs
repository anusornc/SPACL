#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Overnight Real-World Ontology Benchmark
//!
//! Run with: cargo run --release --example overnight_test

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use owl2_reasoner::parser::{OntologyParser, RdfXmlParser};
use owl2_reasoner::{Ontology, OwlReasoner, SimpleReasoner, SpeculativeTableauxReasoner};

/// Ontology to test
struct TestCase {
    name: &'static str,
    path: &'static str,
    expected_classes: usize,
}

const TEST_CASES: &[TestCase] = &[
    TestCase {
        name: "LUBM",
        path: "tests/data/univ-bench.owl",
        expected_classes: 43,
    },
    TestCase {
        name: "PATO",
        path: "benchmarks/ontologies/other/pato.owl",
        expected_classes: 3000,
    },
    TestCase {
        name: "DOID",
        path: "benchmarks/ontologies/other/doid.owl",
        expected_classes: 15000,
    },
    TestCase {
        name: "UBERON",
        path: "benchmarks/ontologies/other/uberon.owl",
        expected_classes: 15000,
    },
    TestCase {
        name: "GO_Basic",
        path: "benchmarks/ontologies/other/go-basic.owl",
        expected_classes: 45000,
    },
];

fn log(msg: &str) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    println!("[{}] {}", timestamp, msg);
    std::io::stdout().flush().unwrap();
}

fn test_ontology(name: &str, path: &str) -> Result<BenchmarkResult, String> {
    log(&format!(
        "═══════════════════════════════════════════════════"
    ));
    log(&format!("Testing: {}", name));
    log(&format!("Path: {}", path));

    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return Err(format!("File not found: {}", path));
    }

    // Get file size
    let file_size = std::fs::metadata(path_obj).map(|m| m.len()).unwrap_or(0);
    let file_size_mb = file_size as f64 / 1024.0 / 1024.0;
    log(&format!("File size: {:.2} MB", file_size_mb));

    // Load file
    log("Step 1/5: Reading file from disk...");
    let start = Instant::now();
    let content =
        std::fs::read_to_string(path_obj).map_err(|e| format!("Failed to read file: {:?}", e))?;
    let read_time = start.elapsed();
    log(&format!("✓ File read complete: {:?}", read_time));

    // Parse ontology
    log("Step 2/5: Parsing RDF/XML ontology...");
    let start = Instant::now();
    let parser = RdfXmlParser::new();
    let ontology: Ontology = parser
        .parse_str(&content)
        .map_err(|e| format!("Failed to parse: {:?}", e))?;
    let parse_time = start.elapsed();

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    log(&format!("✓ Parse complete: {:?}", parse_time));
    log(&format!("  Classes: {}", class_count));
    log(&format!("  Axioms: {}", axiom_count));

    // Sequential reasoning
    log("Step 3/5: Running SEQUENTIAL reasoning...");
    log("  (This may take several minutes for large ontologies)");
    let start = Instant::now();
    let mut seq_reasoner = SimpleReasoner::new(ontology.clone());
    let seq_result = seq_reasoner.is_consistent();
    let seq_time = start.elapsed();

    match seq_result {
        Ok(consistent) => {
            log(&format!("✓ Sequential complete: {:?}", seq_time));
            log(&format!(
                "  Result: {}",
                if consistent {
                    "CONSISTENT"
                } else {
                    "INCONSISTENT"
                }
            ));
        }
        Err(e) => {
            log(&format!("✗ Sequential failed: {:?}", e));
        }
    }

    // SPACL reasoning
    log("Step 4/5: Running SPACL parallel reasoning...");
    log("  (This may take several minutes for large ontologies)");
    let start = Instant::now();
    let mut spacl_reasoner = SpeculativeTableauxReasoner::new(ontology.clone());
    let spacl_result = spacl_reasoner.is_consistent();
    let spacl_time = start.elapsed();

    match spacl_result {
        Ok(consistent) => {
            log(&format!("✓ SPACL complete: {:?}", spacl_time));
            log(&format!(
                "  Result: {}",
                if consistent {
                    "CONSISTENT"
                } else {
                    "INCONSISTENT"
                }
            ));
        }
        Err(e) => {
            log(&format!("✗ SPACL failed: {:?}", e));
        }
    }

    // Calculate speedup
    log("Step 5/5: Calculating results...");
    let speedup = if spacl_time.as_nanos() > 0 {
        seq_time.as_nanos() as f64 / spacl_time.as_nanos() as f64
    } else {
        0.0
    };

    log(&format!("✓ Speedup: {:.2}x", speedup));

    Ok(BenchmarkResult {
        name: name.to_string(),
        file_size_mb,
        classes: class_count,
        axioms: axiom_count,
        read_time_ms: read_time.as_millis() as u64,
        parse_time_ms: parse_time.as_millis() as u64,
        seq_time_ms: seq_time.as_millis() as u64,
        spacl_time_ms: spacl_time.as_millis() as u64,
        speedup,
    })
}

#[derive(Debug)]
struct BenchmarkResult {
    name: String,
    file_size_mb: f64,
    classes: usize,
    axioms: usize,
    read_time_ms: u64,
    parse_time_ms: u64,
    seq_time_ms: u64,
    spacl_time_ms: u64,
    speedup: f64,
}

fn print_summary(results: &[BenchmarkResult]) {
    log("");
    log("╔════════════════════════════════════════════════════════════════╗");
    log("║                    BENCHMARK SUMMARY                           ║");
    log("╚════════════════════════════════════════════════════════════════╝");
    log("");

    println!(
        "{:<12} {:>8} {:>10} {:>12} {:>12} {:>10}",
        "Ontology", "Size(MB)", "Classes", "Seq(ms)", "SPACL(ms)", "Speedup"
    );
    println!("{}", "─".repeat(80));

    for r in results {
        println!(
            "{:<12} {:>8.1} {:>10} {:>12} {:>12} {:>9.2}x",
            r.name, r.file_size_mb, r.classes, r.seq_time_ms, r.spacl_time_ms, r.speedup
        );
    }

    log("");

    // Save JSON results
    let json = serde_json::json!({
        "timestamp": chrono::Local::now().to_rfc3339(),
        "results": results.iter().map(|r| serde_json::json!({
            "name": r.name,
            "file_size_mb": r.file_size_mb,
            "classes": r.classes,
            "axioms": r.axioms,
            "read_time_ms": r.read_time_ms,
            "parse_time_ms": r.parse_time_ms,
            "seq_time_ms": r.seq_time_ms,
            "spacl_time_ms": r.spacl_time_ms,
            "speedup": r.speedup,
        })).collect::<Vec<_>>(),
    });

    let json_str = serde_json::to_string_pretty(&json).unwrap();
    let output_file = "results/overnight_results.json";

    std::fs::create_dir_all("results").ok();
    std::fs::write(output_file, json_str).expect("Failed to write results");

    log(&format!("Results saved to: {}", output_file));
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Overnight Real-World Ontology Benchmark                    ║");
    println!("║     SPACL vs Sequential Performance Test                       ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!(
        "Started at: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    println!("This will take several hours. Progress will be logged.");
    println!();

    let mut results = Vec::new();
    let total = TEST_CASES.len();

    for (i, test) in TEST_CASES.iter().enumerate() {
        log(&format!(""));
        log(&format!(
            "=========================================================="
        ));
        log(&format!("PROGRESS: {}/{} ontologies", i + 1, total));
        log(&format!(
            "=========================================================="
        ));

        match test_ontology(test.name, test.path) {
            Ok(result) => results.push(result),
            Err(e) => {
                log(&format!("✗ FAILED: {}", e));
            }
        }
    }

    print_summary(&results);

    log("");
    log("Benchmark complete!");
    log(&format!(
        "Finished at: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
}
