#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
//! Test GRAIL with Real-World Ontologies
//!
//! This tests GRAIL performance on actual biomedical ontologies:
//! - PATO: Phenotype And Trait Ontology (13K classes)
//! - DOID: Disease Ontology (15K classes)
//! - UBERON: Anatomy Ontology (45K classes)
//! - GO_Basic: Gene Ontology (51K classes)

use owl2_reasoner::{HierarchicalClassificationEngine, Ontology, OwlReasoner, ParserFactory};
use std::time::Instant;

fn main() {
    println!("========================================");
    println!("GRAIL Real-World Ontology Test");
    println!("========================================\n");

    let ontologies = vec![
        ("LUBM", "tests/data/univ-bench.owl"),
        ("PATO", "benchmarks/ontologies/other/pato.owl"),
        ("DOID", "benchmarks/ontologies/other/doid.owl"),
        ("UBERON", "benchmarks/ontologies/other/uberon.owl"),
        ("GO_Basic", "benchmarks/ontologies/other/go-basic.owl"),
    ];

    for (name, path) in ontologies {
        println!("\n{}", "=".repeat(70));
        println!("Testing: {}", name);
        println!("{}", "=".repeat(70));

        // Check file exists
        if !std::path::Path::new(path).exists() {
            println!("  ⚠️  File not found: {}", path);
            continue;
        }

        // Load ontology
        println!("  Loading...");
        let start = Instant::now();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                println!("  ❌ Error loading: {:?}", e);
                continue;
            }
        };

        let parser = match ParserFactory::auto_detect(&content) {
            Some(p) => p,
            None => {
                println!("  ❌ No parser found");
                continue;
            }
        };

        let ontology = match parser.parse_str(&content) {
            Ok(o) => o,
            Err(e) => {
                println!("  ❌ Parse error: {:?}", e);
                continue;
            }
        };
        let load_time = start.elapsed();

        let class_count = ontology.classes().len();
        let axiom_count = ontology.axioms().len();
        let file_size_mb = std::fs::metadata(path)
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0);

        println!(
            "  ✓ Loaded: {} classes, {} axioms ({:.1} MB) in {:?}",
            class_count, axiom_count, file_size_mb, load_time
        );

        // Check if hierarchical can handle it
        let start = Instant::now();
        let can_handle = HierarchicalClassificationEngine::can_handle(&ontology);
        let check_time = start.elapsed();
        println!(
            "  Can use hierarchical: {} (checked in {:?})",
            can_handle, check_time
        );

        if !can_handle {
            println!("  ⚠️  Skipping (not hierarchical enough)");
            continue;
        }

        // Count simple vs complex axioms for info
        let mut simple_count = 0;
        let mut complex_count = 0;
        for axiom in ontology.subclass_axioms() {
            match (axiom.sub_class(), axiom.super_class()) {
                (
                    owl2_reasoner::logic::axioms::ClassExpression::Class(_),
                    owl2_reasoner::logic::axioms::ClassExpression::Class(_),
                ) => {
                    simple_count += 1;
                }
                _ => complex_count += 1,
            }
        }
        println!(
            "  Simple subclass axioms: {}/{} ({:.1}%)",
            simple_count,
            simple_count + complex_count,
            if simple_count + complex_count > 0 {
                100.0 * simple_count as f64 / (simple_count + complex_count) as f64
            } else {
                0.0
            }
        );

        // Classify with GRAIL
        println!("  Classifying with GRAIL...");
        let start = Instant::now();
        let mut engine = HierarchicalClassificationEngine::new(ontology.clone());
        let result = match engine.classify() {
            Ok(r) => r,
            Err(e) => {
                println!("  ❌ Classification error: {:?}", e);
                continue;
            }
        };
        let classify_time = start.elapsed();

        println!("  ✓ Classified in {:?}", classify_time);
        println!("  Classes processed: {}", result.stats.classes_processed);
        println!(
            "  Relationships discovered: {}",
            result.stats.relationships_discovered
        );

        // Calculate throughput
        let classes_per_sec = if classify_time.as_secs_f64() > 0.0 {
            class_count as f64 / classify_time.as_secs_f64()
        } else {
            0.0
        };
        println!("  Throughput: {:.0} classes/sec", classes_per_sec);

        // Test queries
        if class_count > 0 {
            let start = Instant::now();
            let classes: Vec<_> = ontology.classes().iter().cloned().collect();
            let thing_iri = owl2_reasoner::core::iri::IRI::new(
                "http://www.w3.org/2002/07/owl#Thing".to_string(),
            )
            .unwrap();

            // Query 100 random subclass relationships
            let query_count = 100.min(classes.len());
            for i in 0..query_count {
                let _ = engine.is_subclass_of(classes[i].iri(), &thing_iri);
            }
            let query_time = start.elapsed();

            println!("  Query time ({} checks): {:?}", query_count, query_time);
            if query_count > 0 {
                println!("  Avg per query: {:?}", query_time / query_count as u32);
            }
        }

        // Memory estimate
        let mem_estimate_mb = if class_count > 0 {
            let relationships = result.stats.relationships_discovered;
            // Rough estimate: 8 bytes per relationship pointer + overhead
            let bytes = relationships * 32; // Conservative estimate
            bytes as f64 / 1_048_576.0
        } else {
            0.0
        };
        println!("  Estimated hierarchy memory: {:.1} MB", mem_estimate_mb);

        // Compare with old approach if we have historical data
        if name == "GO_Basic" && class_count > 50000 {
            println!("  🎯 GO_Basic with GRAIL: Previously timeout with old approach!");
        }
    }

    println!("\n{}", "=".repeat(70));
    println!("Real-World Test Complete");
    println!("{}", "=".repeat(70));
}
