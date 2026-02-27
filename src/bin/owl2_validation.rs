//! OWL2 validation CLI using the shared ontology loader.

use std::env;
use std::path::Path;
use std::time::Instant;

use owl2_reasoner::{
    serializer::BinaryOntologyFormat, util::ontology_io::load_ontology_with_env, Ontology,
    SimpleReasoner,
};

fn print_usage() {
    println!("OWL2 Validation Tool");
    println!();
    println!("Usage: owl2_validation <command> <file> [out.owlbin]");
    println!();
    println!("Commands:");
    println!("  check <file>          Validate and run consistency check");
    println!("  stats <file>          Print ontology statistics");
    println!("  convert <in> <out>    Convert ontology to .owlbin");
    println!();
    println!("Environment:");
    println!("  OWL2_REASONER_LARGE_PARSE=1");
    println!("  OWL2_REASONER_AUTO_CACHE=1");
    println!("  OWL2_REASONER_FORCE_TEXT=1");
    println!("  OWL2_REASONER_BIN_ONLY=1");
    println!("  OWL2_REASONER_MAX_FILE_SIZE=<bytes>");
    println!("  OWL2_REASONER_LARGE_PROFILE_AUTO=0");
    println!("  OWL2_REASONER_LARGE_PROFILE_THRESHOLD=<bytes>");
    println!("  OWL2_REASONER_STRUCTURAL_XML_AUTO=0");
    println!("  OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD=<bytes>");
}

fn load_ontology(path: &str) -> Result<Ontology, String> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }
    println!("Loading ontology: {}", path.display());
    let start = Instant::now();
    let ontology = load_ontology_with_env(path).map_err(|e| format!("{}", e))?;
    println!("  ✓ Loaded in {:?}", start.elapsed());
    Ok(ontology)
}

fn cmd_check(path: &str) {
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    println!("Running consistency check...");
    let start = Instant::now();
    let reasoner = SimpleReasoner::new(ontology);
    match reasoner.is_consistent() {
        Ok(true) => {
            println!("  ✓ CONSISTENT ({:?})", start.elapsed());
        }
        Ok(false) => {
            println!("  ✗ INCONSISTENT ({:?})", start.elapsed());
        }
        Err(e) => {
            eprintln!("Error during reasoning: {:?}", e);
        }
    }
}

fn cmd_stats(path: &str) {
    let ontology = match load_ontology(path) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    println!("Classes: {}", ontology.classes().len());
    println!("Object properties: {}", ontology.object_properties().len());
    println!("Data properties: {}", ontology.data_properties().len());
    println!("Named individuals: {}", ontology.named_individuals().len());
    println!("Axioms: {}", ontology.axioms().len());
}

fn cmd_convert(input: &str, output: &str) {
    let ontology = match load_ontology(input) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    println!("Converting to binary format...");
    let start = Instant::now();
    let mut file = match std::fs::File::create(output) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating output file: {}", e);
            return;
        }
    };

    match BinaryOntologyFormat::serialize(&ontology, &mut file) {
        Ok(()) => println!("  ✓ Conversion complete in {:?}", start.elapsed()),
        Err(e) => eprintln!("Error during conversion: {}", e),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "check" => cmd_check(&args[2]),
        "stats" => cmd_stats(&args[2]),
        "convert" => {
            if args.len() < 4 {
                eprintln!("Error: convert requires output path");
                print_usage();
                return;
            }
            cmd_convert(&args[2], &args[3]);
        }
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
        }
    }
}
