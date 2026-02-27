#!/bin/bash
# Overnight Real-World Ontology Benchmark
# Runs benchmarks with progress tracking and saves results

set -e

REPO_DIR="/home/admindigit/tableauxx"
RESULTS_DIR="$REPO_DIR/results/overnight"
LOG_FILE="$RESULTS_DIR/benchmark.log"
RESULTS_FILE="$RESULTS_DIR/results.json"
PID_FILE="$RESULTS_DIR/benchmark.pid"

# Create results directory
mkdir -p "$RESULTS_DIR"

# Save PID for monitoring
echo $$ > "$PID_FILE"

# Logging function
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Progress function
progress() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] PROGRESS: $1" | tee -a "$LOG_FILE"
}

log "╔════════════════════════════════════════════════════════════════╗"
log "║     Overnight Real-World Ontology Benchmark Started           ║"
log "╚════════════════════════════════════════════════════════════════╝"
log ""
log "PID: $$"
log "Results directory: $RESULTS_DIR"
log "Log file: $LOG_FILE"
log ""

# Ontologies to test (ordered by size - smallest first)
ONTOLOGIES=(
    "LUBM:tests/data/univ-bench.owl:small"
    "PATO:benchmarks/ontologies/other/pato.owl:medium"
    "DOID:benchmarks/ontologies/other/doid.owl:medium"
    "UBERON:benchmarks/ontologies/other/uberon.owl:large"
    "GO_Basic:benchmarks/ontologies/other/go-basic.owl:xlarge"
)

# Initialize results JSON
echo '{"start_time": "'$(date -Iseconds)'", "status": "running", "ontologies": []}' > "$RESULTS_FILE"

cd "$REPO_DIR"

# Function to test a single ontology
test_ontology() {
    local name=$1
    local path=$2
    local size=$3
    
    progress "Starting $name (size: $size)"
    
    # Check if file exists
    if [ ! -f "$path" ]; then
        log "ERROR: $path not found"
        return 1
    fi
    
    # Get file size
    local file_size=$(stat -c%s "$path" 2>/dev/null || echo "0")
    local file_size_mb=$(echo "scale=2; $file_size / 1024 / 1024" | bc 2>/dev/null || echo "0")
    
    progress "$name: File size ${file_size_mb}MB"
    
    # Build a simple test binary
    local test_name="test_${name,,}"
    
    # Create a temporary test program
    cat > "/tmp/${test_name}.rs" << EOF
use std::path::Path;
use std::time::Instant;
use owl2_reasoner::{Ontology, SimpleReasoner, SpeculativeTableauxReasoner, OwlReasoner};
use owl2_reasoner::parser::{RdfXmlParser, OntologyParser};

fn main() {
    let path = "$path";
    let name = "$name";
    
    println!("=== Testing {} ===", name);
    
    // Load file
    let start = Instant::now();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("ERROR: Failed to read file: {:?}", e);
            std::process::exit(1);
        }
    };
    let read_time = start.elapsed();
    println!("File read time: {:?}", read_time);
    
    // Parse ontology
    let start = Instant::now();
    let parser = RdfXmlParser::new();
    let ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            println!("ERROR: Failed to parse: {:?}", e);
            std::process::exit(1);
        }
    };
    let parse_time = start.elapsed();
    
    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();
    
    println!("Parse time: {:?}", parse_time);
    println!("Classes: {}", class_count);
    println!("Axioms: {}", axiom_count);
    
    // Sequential reasoning
    println!("Running sequential reasoning...");
    let start = Instant::now();
    let mut seq_reasoner = SimpleReasoner::new(ontology.clone());
    let seq_result = seq_reasoner.is_consistent();
    let seq_time = start.elapsed();
    
    println!("Sequential time: {:?}", seq_time);
    println!("Sequential result: {:?}", seq_result.is_ok());
    
    // SPACL reasoning
    println!("Running SPACL reasoning...");
    let start = Instant::now();
    let mut spacl_reasoner = SpeculativeTableauxReasoner::new(ontology.clone());
    let spacl_result = spacl_reasoner.is_consistent();
    let spacl_time = start.elapsed();
    
    println!("SPACL time: {:?}", spacl_time);
    println!("SPACL result: {:?}", spacl_result.is_ok());
    
    // Calculate speedup
    let speedup = if spacl_time.as_nanos() > 0 {
        seq_time.as_nanos() as f64 / spacl_time.as_nanos() as f64
    } else {
        0.0
    };
    
    println!("Speedup: {:.2}x", speedup);
    
    // Output JSON results
    println!("\nJSON_RESULT: {{\"name\": \"{}\", \"file_size_mb\": {:.2}, \"classes\": {}, \"axioms\": {}, \"read_time_ms\": {}, \"parse_time_ms\": {}, \"seq_time_ms\": {}, \"spacl_time_ms\": {}, \"speedup\": {:.2}, \"status\": \"success\"}}",
        name,
        $file_size_mb,
        class_count,
        axiom_count,
        read_time.as_millis(),
        parse_time.as_millis(),
        seq_time.as_millis(),
        spacl_time.as_millis(),
        speedup
    );
}
EOF
    
    progress "$name: Compiling test program..."
    
    # Run using cargo with a timeout
    progress "$name: Running benchmark (this may take a while)..."
    
    timeout 3600 cargo run --release --example overnight_test 2>&1 | tee -a "$LOG_FILE" || {
        log "WARNING: $name benchmark timed out or failed"
        return 1
    }
    
    progress "$name: Complete"
    return 0
}

# Create the example
mkdir -p "$REPO_DIR/examples"
cat > "$REPO_DIR/examples/overnight_test.rs" << 'RUST_EOF'
use std::path::Path;
use std::time::Instant;
use owl2_reasoner::{Ontology, SimpleReasoner, SpeculativeTableauxReasoner, OwlReasoner};
use owl2_reasoner::parser::{RdfXmlParser, OntologyParser};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: overnight_test <name> <path>");
        std::process::exit(1);
    }
    
    let name = &args[1];
    let path = &args[2];
    
    println!("=== Testing {} ===", name);
    println!("Path: {}", path);
    
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        println!("ERROR: File not found: {}", path);
        std::process::exit(1);
    }
    
    let file_size = std::fs::metadata(path_obj).map(|m| m.len()).unwrap_or(0);
    let file_size_mb = file_size as f64 / 1024.0 / 1024.0;
    println!("File size: {:.2} MB", file_size_mb);
    
    // Load file
    println!("Loading file...");
    let start = Instant::now();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("ERROR: Failed to read file: {:?}", e);
            std::process::exit(1);
        }
    };
    let read_time = start.elapsed();
    println!("✓ File read time: {:?}", read_time);
    
    // Parse ontology
    println!("Parsing ontology...");
    let start = Instant::now();
    let parser = RdfXmlParser::new();
    let ontology: Ontology = match parser.parse_str(&content) {
        Ok(o) => o,
        Err(e) => {
            println!("ERROR: Failed to parse: {:?}", e);
            std::process::exit(1);
        }
    };
    let parse_time = start.elapsed();
    
    let class_count = ontology.classes().len();
    let axiom_count = ontology.axioms().len();
    
    println!("✓ Parse time: {:?}", parse_time);
    println!("✓ Classes: {}", class_count);
    println!("✓ Axioms: {}", axiom_count);
    
    // Sequential reasoning
    println!("Running sequential reasoning (may take several minutes)...");
    let start = Instant::now();
    let mut seq_reasoner = SimpleReasoner::new(ontology.clone());
    let seq_result = seq_reasoner.is_consistent();
    let seq_time = start.elapsed();
    
    println!("✓ Sequential time: {:?}", seq_time);
    println!("✓ Sequential result: consistent={}", seq_result.unwrap_or(false));
    
    // SPACL reasoning
    println!("Running SPACL reasoning (may take several minutes)...");
    let start = Instant::now();
    let mut spacl_reasoner = SpeculativeTableauxReasoner::new(ontology.clone());
    let spacl_result = spacl_reasoner.is_consistent();
    let spacl_time = start.elapsed();
    
    println!("✓ SPACL time: {:?}", spacl_time);
    println!("✓ SPACL result: consistent={}", spacl_result.unwrap_or(false));
    
    // Calculate speedup
    let speedup = if spacl_time.as_nanos() > 0 {
        seq_time.as_nanos() as f64 / spacl_time.as_nanos() as f64
    } else {
        0.0
    };
    
    println!("✓ Speedup: {:.2}x", speedup);
    
    // Output JSON results
    println!("\n=== JSON_RESULT ===");
    println!("{{\"name\": \"{}\", \"file_size_mb\": {:.2}, \"classes\": {}, \"axioms\": {}, \"read_time_ms\": {}, \"parse_time_ms\": {}, \"seq_time_ms\": {}, \"spacl_time_ms\": {}, \"speedup\": {:.2}, \"status\": \"success\"}}",
        name,
        file_size_mb,
        class_count,
        axiom_count,
        read_time.as_millis(),
        parse_time.as_millis(),
        seq_time.as_millis(),
        spacl_time.as_millis(),
        speedup
    );
}
RUST_EOF

# Run benchmarks for each ontology
for ont in "${ONTOLOGIES[@]}"; do
    IFS=':' read -r name path size <<< "$ont"
    
    log ""
    log "═══════════════════════════════════════════════════════════════"
    log "Testing $name ($size)"
    log "═══════════════════════════════════════════════════════════════"
    
    # Run the example with timeout
    timeout 7200 cargo run --release --example overnight_test -- "$name" "$path" 2>&1 | tee -a "$LOG_FILE" || {
        log "WARNING: $name benchmark failed or timed out after 2 hours"
    }
    
    log ""
    log "Completed $name"
done

log ""
log "╔════════════════════════════════════════════════════════════════╗"
log "║     Overnight Benchmark Complete                              ║"
log "╚════════════════════════════════════════════════════════════════╝"
log "Results saved to: $RESULTS_DIR"

# Mark as complete
jq '.status = "complete" | .end_time = "'$(date -Iseconds)'"' "$RESULTS_FILE" > "${RESULTS_FILE}.tmp" && mv "${RESULTS_FILE}.tmp" "$RESULTS_FILE"

rm -f "$PID_FILE"
