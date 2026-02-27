#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test GRAIL on large ontologies with progress monitoring
//!
//! This version prints progress updates so we can monitor long-running operations

use owl2_reasoner::{HierarchicalClassificationEngine, ParserFactory};
use std::io::Write;
use std::time::{Duration, Instant};

fn test_ontology(name: &str, path: &str) {
    println!("\n{}", "=".repeat(70));
    println!("Testing: {}", name);
    println!("{}", "=".repeat(70));

    if !std::path::Path::new(path).exists() {
        println!("  ❌ File not found: {}", path);
        return;
    }

    // Get file size
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("  File size: {:.2} MB", file_size as f64 / 1_048_576.0);

    // Load with progress
    print!("  Loading... ");
    std::io::stdout().flush().unwrap();
    let start = Instant::now();
    let load_result = load_with_progress(path);
    let load_time = start.elapsed();

    let ontology = match load_result {
        Some(o) => {
            println!(" ✓ Done in {:?}", load_time);
            o
        }
        None => {
            println!(" ❌ Failed");
            return;
        }
    };

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();
    println!("  Classes: {}, Axioms: {}", class_count, axiom_count);

    // Check hierarchical
    print!("  Checking if hierarchical... ");
    std::io::stdout().flush().unwrap();
    let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
    println!("{}", if can_handle { "✓ Yes" } else { "✗ No" });

    if !can_handle {
        return;
    }

    // Classify with GRAIL
    println!("  Classifying with GRAIL...");
    let start = Instant::now();
    let mut engine = HierarchicalClassificationEngine::new(ontology);

    // Print progress every few seconds during classification
    let classify_start = Instant::now();
    let classify_result = engine.classify();
    let classify_time = classify_start.elapsed();

    match classify_result {
        Ok(result) => {
            let total_time = start.elapsed();
            println!("  ✓ Classification complete!");
            println!("    GRAIL build + classify: {:?}", classify_time);
            println!("    Total time: {:?}", total_time);
            println!("    Classes processed: {}", result.stats.classes_processed);
            println!(
                "    Relationships: {}",
                result.stats.relationships_discovered
            );

            let throughput = if classify_time.as_secs_f64() > 0.0 {
                class_count as f64 / classify_time.as_secs_f64()
            } else {
                0.0
            };
            println!("    Throughput: {:.0} classes/sec", throughput);
        }
        Err(e) => {
            println!("  ❌ Classification error: {:?}", e);
        }
    }

    println!("  ✓ Test complete for {}", name);
}

fn load_with_progress(path: &str) -> Option<owl2_reasoner::Ontology> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let parser = ParserFactory::auto_detect(&content)?;

    // For large files, this takes a while
    // Print a message every 30 seconds to show we're still alive
    let start = Instant::now();
    let last_print = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last_print_clone = last_print.clone();

    // Spawn a progress monitor thread
    let monitor_handle = std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(10));
            let elapsed = start.elapsed().as_secs();
            let last = last_print_clone.load(std::sync::atomic::Ordering::Relaxed);

            if elapsed > last + 30 {
                print!(" ({}s)... ", elapsed);
                std::io::stdout().flush().unwrap();
                last_print_clone.store(elapsed, std::sync::atomic::Ordering::Relaxed);
            }

            // Exit after 30 minutes (1800 seconds)
            if elapsed > 1800 {
                break;
            }
        }
    });

    let result = parser.parse_str(&content).ok();

    // Let monitor thread finish
    drop(monitor_handle);

    result
}

fn main() {
    println!("========================================");
    println!("GRAIL Real-World Test with Progress");
    println!("========================================");
    println!("This test will show progress every 30 seconds during long operations.");
    println!("Press Ctrl+C to cancel.\n");

    let start_time = Instant::now();

    // Test ontologies from smallest to largest
    test_ontology("LUBM", "tests/data/univ-bench.owl");
    test_ontology("PATO", "benchmarks/ontologies/other/pato.owl");
    test_ontology("DOID", "benchmarks/ontologies/other/doid.owl");

    // Large ontologies - these take a while to load
    println!("\n⚠️  Large ontologies ahead - this may take 10-30 minutes per ontology");
    println!("Progress will be shown every 30 seconds.\n");

    test_ontology("UBERON", "benchmarks/ontologies/other/uberon.owl");
    test_ontology("GO_Basic", "benchmarks/ontologies/other/go-basic.owl");

    let total_elapsed = start_time.elapsed();

    println!("\n{}", "=".repeat(70));
    println!("All Tests Complete!");
    println!("Total time: {:?}", total_elapsed);
    println!("{}", "=".repeat(70));
}
