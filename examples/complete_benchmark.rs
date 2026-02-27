#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Complete Real-World Ontology Benchmark
//!
//! This benchmark runs to completion and saves incremental results.

use owl2_reasoner::{
    HierarchicalClassificationEngine, Ontology, OwlReasoner, ParserFactory, SimpleReasoner,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

fn main() {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let log_file = format!("results/benchmark_{}.log", timestamp);

    println!("=== Complete Real-World Ontology Benchmark ===");
    println!("Results will be saved to: {}", log_file);
    println!();

    // Create results directory
    let _ = std::fs::create_dir_all("results");

    log(
        &log_file,
        "=== Complete Real-World Ontology Benchmark ===\n",
    );
    log(&log_file, &format!("Started at: {}\n", timestamp));
    log(&log_file, "\n");

    // Test ontologies in order of size (smallest first)
    let ontologies = vec![
        ("LUBM", "tests/data/univ-bench.owl"),
        ("PATO", "benchmarks/ontologies/other/pato.owl"),
        ("DOID", "benchmarks/ontologies/other/doid.owl"),
        ("UBERON", "benchmarks/ontologies/other/uberon.owl"),
        ("GO_Basic", "benchmarks/ontologies/other/go-basic.owl"),
    ];

    for (name, path) in ontologies {
        println!("\n{}", "=".repeat(60));
        println!("Testing: {}", name);
        println!("{}", "=".repeat(60));

        log(&log_file, &format!("\n{}\n", "=".repeat(60)));
        log(&log_file, &format!("Testing: {}\n", name));
        log(&log_file, &format!("{}\n", "=".repeat(60)));

        match benchmark_ontology(name, path, &log_file) {
            Ok(_) => {
                println!("✓ {} completed", name);
                log(&log_file, &format!("✓ {} completed\n", name));
            }
            Err(e) => {
                println!("✗ {} failed: {:?}", name, e);
                log(&log_file, &format!("✗ {} failed: {:?}\n", name, e));
            }
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("BENCHMARK COMPLETE");
    println!("{}", "=".repeat(60));
    println!("Results saved to: {}", log_file);

    log(&log_file, "\n{'=':=<60}\n");
    log(&log_file, "BENCHMARK COMPLETE\n");
    log(&log_file, &format!("Results saved to: {}\n", log_file));
}

fn log(file: &str, msg: &str) {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .unwrap();
    let _ = f.write_all(msg.as_bytes());
    let _ = f.flush();
}

fn benchmark_ontology(
    name: &str,
    path: &str,
    log_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Load ontology
    println!("[1/5] Loading ontology...");
    let start = Instant::now();

    let content = std::fs::read_to_string(path)?;
    let parser = ParserFactory::auto_detect(&content).ok_or("No parser found")?;
    let ontology = parser.parse_str(&content)?;
    let load_time = start.elapsed();

    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();

    println!("  Loaded: {} classes, {} axioms", class_count, axiom_count);
    println!("  Load time: {:?}", load_time);

    log(log_file, &format!("  Classes: {}\n", class_count));
    log(log_file, &format!("  Axioms: {}\n", axiom_count));
    log(log_file, &format!("  Load time: {:?}\n", load_time));

    // Step 2: Check strategy
    println!("[2/5] Analyzing ontology characteristics...");
    let can_hierarchical = HierarchicalClassificationEngine::can_handle(&ontology);
    println!("  Can use hierarchical: {}", can_hierarchical);
    log(
        log_file,
        &format!("  Can use hierarchical: {}\n", can_hierarchical),
    );

    // Count simple vs complex axioms
    let mut simple_count = 0;
    let mut total_subclass = 0;
    for axiom in ontology.subclass_axioms() {
        total_subclass += 1;
        match (axiom.sub_class(), axiom.super_class()) {
            (
                owl2_reasoner::ClassExpression::Class(_),
                owl2_reasoner::ClassExpression::Class(_),
            ) => {
                simple_count += 1;
            }
            _ => {}
        }
    }
    let simple_ratio = if total_subclass > 0 {
        simple_count as f64 / total_subclass as f64 * 100.0
    } else {
        0.0
    };

    println!(
        "  Simple subclass axioms: {}/{} ({:.1}%)",
        simple_count, total_subclass, simple_ratio
    );
    log(
        log_file,
        &format!(
            "  Simple subclass axioms: {}/{} ({:.1}%)\n",
            simple_count, total_subclass, simple_ratio
        ),
    );

    // Step 3: Benchmark hierarchical (if applicable)
    let hierarchical_time = if can_hierarchical {
        println!("[3/5] Running hierarchical classification...");

        // Run multiple iterations for stable timing
        let iterations = if class_count < 100 { 100 } else { 10 };
        let mut times = Vec::new();

        for i in 0..iterations {
            let start = Instant::now();
            let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
            let _ = engine.classify();
            let elapsed = start.elapsed();
            times.push(elapsed);

            if i == 0 {
                println!("  First run: {:?}", elapsed);
                log(
                    log_file,
                    &format!("  First hierarchical run: {:?}\n", elapsed),
                );
            }
        }

        // Calculate average (exclude first run as warm-up)
        let avg_time: std::time::Duration =
            times.iter().skip(1).sum::<std::time::Duration>() / (times.len() - 1).max(1) as u32;

        println!("  Average hierarchical time: {:?}", avg_time);
        log(
            log_file,
            &format!("  Average hierarchical time: {:?}\n", avg_time),
        );

        Some(avg_time)
    } else {
        println!("[3/5] Skipping hierarchical (not applicable)");
        log(log_file, "  Skipping hierarchical (not applicable)\n");
        None
    };

    // Step 4: Benchmark simple reasoner
    println!("[4/5] Running simple reasoner...");

    let simple_iterations = if class_count < 1000 { 10 } else { 3 };
    let mut simple_times = Vec::new();

    for i in 0..simple_iterations {
        let start = Instant::now();
        let mut reasoner = SimpleReasoner::new(ontology.clone());
        let _ = reasoner.is_consistent();
        let elapsed = start.elapsed();
        simple_times.push(elapsed);

        if i == 0 {
            println!("  First run: {:?}", elapsed);
            log(log_file, &format!("  First simple run: {:?}\n", elapsed));
        }
    }

    let avg_simple: std::time::Duration = simple_times.iter().skip(1).sum::<std::time::Duration>()
        / (simple_times.len() - 1).max(1) as u32;

    println!("  Average simple time: {:?}", avg_simple);
    log(
        log_file,
        &format!("  Average simple time: {:?}\n", avg_simple),
    );

    // Step 5: Calculate speedup
    println!("[5/5] Calculating results...");

    if let Some(hier_time) = hierarchical_time {
        let speedup = avg_simple.as_secs_f64() / hier_time.as_secs_f64();
        let percentage = if hier_time < avg_simple {
            (speedup * 100.0) as i32
        } else {
            (-1.0 / speedup * 100.0) as i32
        };

        println!("\n  RESULTS:");
        println!("  - Hierarchical: {:?}", hier_time);
        println!("  - Simple: {:?}", avg_simple);
        if hier_time < avg_simple {
            println!("  - Speedup: {:.1}x ({}% faster)", speedup, percentage);
        } else {
            println!("  - Simple is faster by {:.1}x", 1.0 / speedup);
        }

        log(log_file, "\n  RESULTS:\n");
        log(log_file, &format!("  - Hierarchical: {:?}\n", hier_time));
        log(log_file, &format!("  - Simple: {:?}\n", avg_simple));
        log(log_file, &format!("  - Speedup: {:.1}x\n", speedup));
    } else {
        println!("\n  RESULTS:");
        println!("  - Simple: {:?}", avg_simple);
        println!("  - Strategy: Simple (hierarchical not applicable)");

        log(log_file, "\n  RESULTS:\n");
        log(log_file, &format!("  - Simple: {:?}\n", avg_simple));
        log(
            log_file,
            "  - Strategy: Simple (hierarchical not applicable)\n",
        );
    }

    Ok(())
}
