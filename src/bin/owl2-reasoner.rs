//! OWL2 Reasoner CLI - Demo Application
//!
//! A command-line tool for reasoning with OWL2 ontologies.
//!
//! Usage:
//!   cargo run --bin owl2-reasoner -- <command> [options] <ontology_file>
//!
//! Commands:
//!   check       Check ontology consistency
//!   check-auto  Check with automatic reasoner selection
//!   convert     Convert OWL to binary format
//!   stats       Show ontology statistics
//!   compare     Compare Sequential vs SPACL performance
//!
//! Examples:
//!   cargo run --bin owl2-reasoner -- check tests/data/univ-bench.owl
//!   cargo run --bin owl2-reasoner -- stats tests/data/univ-bench.owl
//!   cargo run --bin owl2-reasoner -- compare tests/data/univ-bench.owl

use std::env;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use owl2_reasoner::{
    detect_profile, select_consistency_reasoner, serializer::BinaryOntologyFormat,
    util::ontology_io::load_ontology_with_env, ConsistencyReasoner, Ontology,
    OntologyCharacteristics, SimpleReasoner, SpeculativeTableauxReasoner,
};

fn flush_stdout() {
    let _ = std::io::stdout().flush();
}

fn phase_log(label: &str, detail: &str) {
    eprintln!("[phase] {} {}", label, detail);
}

fn print_usage() {
    println!("OWL2 Reasoner CLI - Demo Application");
    println!();
    println!("Usage: owl2-reasoner <command> [options] <ontology_file>");
    println!();
    println!("Commands:");
    println!("  check <file>       Check ontology consistency");
    println!("  check-auto <file>  Check with automatic reasoner selection");
    println!("  convert <in> <out> Convert OWL to binary format (.owlbin)");
    println!("  stats <file>       Show ontology statistics");
    println!("  compare <file>     Compare Sequential vs SPACL performance");
    println!("  help               Show this help message");
    println!();
    println!("Options:");
    println!("  -v, --verbose      Verbose output");
    println!();
    println!("Environment (large ontology / production):");
    println!("  OWL2_REASONER_LARGE_PARSE=1   Enable large-file parser mode");
    println!("  OWL2_REASONER_AUTO_CACHE=1    Parse text and write .owlbin cache");
    println!("  OWL2_REASONER_FORCE_TEXT=1    Ignore .owlbin and parse source text");
    println!("  OWL2_REASONER_BIN_ONLY=1      Require .owlbin input/cache");
    println!("  OWL2_REASONER_MAX_FILE_SIZE   Override parser max file size in bytes");
    println!("  OWL2_REASONER_LARGE_PROFILE_AUTO=0  Disable auto large-file text profile");
    println!(
        "  OWL2_REASONER_LARGE_PROFILE_THRESHOLD=<bytes>  Auto profile threshold (default 4MB)"
    );
    println!(
        "  OWL2_REASONER_STRUCTURAL_XML_AUTO=0  Disable auto structural RDF/XML for large files"
    );
    println!(
        "  OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD=<bytes>  Auto mode threshold (default 4MB)"
    );
    println!("  OWL2_REASONER_EXPERIMENTAL_XML_PARSER=1  Enable experimental RDF/XML pipeline");
    println!("  OWL2_REASONER_EXPERIMENTAL_XML_STRICT=1  Fail if unsupported experimental terms are skipped");
    println!("  OWL2_REASONER_BIN_FORMAT=v1   Write legacy .owlbin format (default v2)");
    println!();
    println!("Examples:");
    println!("  OWL2_REASONER_LARGE_PARSE=1 owl2-reasoner check large.owl");
    println!("  OWL2_REASONER_AUTO_CACHE=1 owl2-reasoner check large.owl");
    println!("  owl2-reasoner check tests/data/univ-bench.owl");
    println!("  owl2-reasoner check-auto large.owl");
    println!("  owl2-reasoner convert large.owl large.owlbin");
    println!("  owl2-reasoner check large.owlbin");
    println!("  owl2-reasoner stats tests/data/univ-bench.owl");
    println!("  owl2-reasoner compare tests/data/univ-bench.owl");
}

fn load_ontology(path: &str) -> Result<Arc<Ontology>, String> {
    let path = Path::new(path);
    println!("Loading ontology: {}", path.display());
    flush_stdout();
    phase_log("parse_start", &format!("file={}", path.display()));
    let start = Instant::now();
    let ontology = load_ontology_with_env(path).map_err(|e| format!("{}", e))?;
    let load_time = start.elapsed();
    println!("✓ Loaded in {:?}", load_time);
    flush_stdout();
    phase_log("parse_done", &format!("ms={}", load_time.as_millis()));
    Ok(Arc::new(ontology))
}

fn cmd_check_auto(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        println!("Usage: owl2-reasoner check-auto <ontology_file>");
        return;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let profile = detect_profile(&ontology);
    let analysis = OntologyCharacteristics::analyze(&ontology);
    let decision = select_consistency_reasoner(&ontology, profile);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║           ONTOLOGY ANALYSIS & REASONER SELECTION             ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Statistics:");
    println!("  Classes:          {}", analysis.class_count);
    println!("  Properties:       {}", analysis.property_count);
    println!("  Axioms:           {}", ontology.axioms().len());
    println!("  Disjunctions:     {}", analysis.disjunction_count);
    println!("  Complexity:       {}", analysis.complexity_description());
    println!();
    println!("Characteristics:");
    println!(
        "  Hierarchy fast path: {}",
        if owl2_reasoner::HierarchicalClassificationEngine::can_handle(&ontology) {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "  Detected profile: {}",
        profile.map_or("OWL2 DL".to_string(), |p| p.name().to_string())
    );
    println!();

    let start = Instant::now();
    phase_log(
        "reason_start",
        &format!("reasoner={}", decision.reasoner.as_str()),
    );
    let consistent = match decision.reasoner {
        ConsistencyReasoner::Simple => {
            println!(
                "Selected: {} ({})",
                decision.reasoner.as_str(),
                decision.rationale
            );
            let reasoner = SimpleReasoner::from_arc(Arc::clone(&ontology));
            reasoner.is_consistent()
        }
        ConsistencyReasoner::Speculative => {
            println!(
                "Selected: {} ({})",
                decision.reasoner.as_str(),
                decision.rationale
            );
            let mut reasoner = SpeculativeTableauxReasoner::from_arc(Arc::clone(&ontology));
            reasoner.is_consistent()
        }
    };

    let check_time = start.elapsed();

    match consistent {
        Ok(result) => {
            println!("\n✓ Consistency check complete in {:?}", check_time);
            flush_stdout();
            phase_log(
                "reason_done",
                &format!(
                    "ms={} result={}",
                    check_time.as_millis(),
                    if result { "consistent" } else { "inconsistent" }
                ),
            );
            println!();
            if result {
                println!("Result: ✅ CONSISTENT");
            } else {
                println!("Result: ❌ INCONSISTENT");
            }
        }
        Err(e) => {
            eprintln!("Error during reasoning: {:?}", e);
        }
    }
}

fn cmd_convert(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Error: Need input and output files");
        println!("Usage: owl2-reasoner convert <input.owl> <output.owlbin>");
        return;
    }

    let input_path = &args[0];
    let output_path = &args[1];

    // Load ontology from OWL
    let ontology = match load_ontology(input_path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    // Serialize to binary
    println!("\nConverting to binary format...");
    let start = Instant::now();

    let mut file = match std::fs::File::create(output_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating output file: {}", e);
            return;
        }
    };

    match BinaryOntologyFormat::serialize(&ontology, &mut file) {
        Ok(()) => {
            let convert_time = start.elapsed();
            println!("✓ Conversion complete in {:?}", convert_time);

            // Show file size comparison
            let input_meta = std::fs::metadata(input_path).ok();
            let output_meta = std::fs::metadata(output_path).ok();

            if let (Some(in_meta), Some(out_meta)) = (input_meta, output_meta) {
                let in_size = in_meta.len();
                let out_size = out_meta.len();
                let ratio = out_size as f64 / in_size as f64;

                println!();
                println!("File sizes:");
                println!(
                    "  Input:  {} bytes ({:.2} MB)",
                    in_size,
                    in_size as f64 / 1_048_576.0
                );
                println!(
                    "  Output: {} bytes ({:.2} MB)",
                    out_size,
                    out_size as f64 / 1_048_576.0
                );
                println!("  Ratio:  {:.1}%", ratio * 100.0);
            }

            println!();
            println!("Usage: owl2-reasoner check {}", output_path);
        }
        Err(e) => {
            eprintln!("Error during conversion: {}", e);
        }
    }
}

fn cmd_check(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        println!("Usage: owl2-reasoner check <ontology_file>");
        return;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    println!("\nChecking consistency...");
    flush_stdout();
    let start = Instant::now();
    phase_log("reason_start", "reasoner=SpeculativeTableauxReasoner");

    let mut reasoner = SpeculativeTableauxReasoner::from_arc(ontology);
    match reasoner.is_consistent() {
        Ok(consistent) => {
            let check_time = start.elapsed();
            println!("✓ Consistency check complete in {:?}", check_time);
            flush_stdout();
            phase_log(
                "reason_done",
                &format!(
                    "ms={} result={}",
                    check_time.as_millis(),
                    if consistent {
                        "consistent"
                    } else {
                        "inconsistent"
                    }
                ),
            );
            println!();
            if consistent {
                println!("Result: ✅ CONSISTENT");
            } else {
                println!("Result: ❌ INCONSISTENT");
            }
        }
        Err(e) => {
            eprintln!("Error during reasoning: {:?}", e);
        }
    }
}

fn cmd_stats(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        println!("Usage: owl2-reasoner stats <ontology_file>");
        return;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let analysis = OntologyCharacteristics::analyze(&ontology);
    let profile = detect_profile(&ontology);
    let consistency_plan = select_consistency_reasoner(&ontology, profile);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                  ONTOLOGY STATISTICS                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Basic Counts:");
    println!("  Classes:             {}", ontology.classes().len());
    println!(
        "  Object Properties:   {}",
        ontology.object_properties().len()
    );
    println!(
        "  Data Properties:     {}",
        ontology.data_properties().len()
    );
    println!(
        "  Named Individuals:   {}",
        ontology.named_individuals().len()
    );
    println!("  Total Axioms:        {}", ontology.axioms().len());
    println!();

    println!("Analysis:");
    println!("  Disjunctions:        {}", analysis.disjunction_count);
    println!(
        "  Tree-like hierarchy: {}",
        if analysis.is_tree_like { "Yes" } else { "No" }
    );
    println!("  Complexity score:    {:.3}", analysis.complexity_score);
    println!(
        "  Detected profile:    {}",
        profile.map_or("OWL2 DL".to_string(), |p| p.name().to_string())
    );
    println!();

    println!(
        "Recommended consistency reasoner: {} ({})",
        consistency_plan.reasoner.as_str(),
        consistency_plan.rationale
    );
}

fn cmd_compare(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: No ontology file specified");
        println!("Usage: owl2-reasoner compare <ontology_file>");
        return;
    }

    let path = &args[0];
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              REASONER PERFORMANCE COMPARISON                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Shared ontology for both reasoners (no deep clone)
    let ontology = Arc::clone(&ontology);

    // SimpleReasoner
    println!("Running SimpleReasoner...");
    let start = Instant::now();
    let simple_reasoner = SimpleReasoner::from_arc(Arc::clone(&ontology));
    let simple_result = simple_reasoner.is_consistent();
    let simple_time = start.elapsed();
    println!("  Result: {:?} in {:?}", simple_result, simple_time);
    println!();

    // SpeculativeTableauxReasoner
    println!("Running SpeculativeTableauxReasoner (SPACL)...");
    let start = Instant::now();
    let mut speculative_reasoner = SpeculativeTableauxReasoner::from_arc(Arc::clone(&ontology));
    let speculative_result = speculative_reasoner.is_consistent();
    let speculative_time = start.elapsed();
    println!(
        "  Result: {:?} in {:?}",
        speculative_result, speculative_time
    );
    println!();

    // Summary
    println!("Summary:");
    println!("  SimpleReasoner:    {:?}", simple_time);
    println!("  SPACL:             {:?}", speculative_time);

    let speedup = simple_time.as_secs_f64() / speculative_time.as_secs_f64();
    if speedup > 1.0 {
        println!("  Speedup:           {:.2}x faster (SPACL)", speedup);
    } else {
        println!("  Speedup:           {:.2}x faster (Simple)", 1.0 / speedup);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    let command = &args[1];
    let command_args = &args[2..];

    match command.as_str() {
        "check" => cmd_check(command_args),
        "check-auto" => cmd_check_auto(command_args),
        "convert" => cmd_convert(command_args),
        "stats" => cmd_stats(command_args),
        "compare" => cmd_compare(command_args),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("Error: Unknown command '{}'", command);
            print_usage();
        }
    }
}
