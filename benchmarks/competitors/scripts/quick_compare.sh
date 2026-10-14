#!/bin/bash
#
# Quick comparison: HermiT vs Tableauxx
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$BENCHMARK_DIR/../.." && pwd)"

ONTOLOGY="${1:-$BENCHMARK_DIR/ontologies/disjunctive_test.owl}"
ONTOLOGY_NAME=$(basename "$ONTOLOGY")

echo "=========================================="
echo "OWL2 Reasoner Quick Comparison"
echo "=========================================="
echo "Ontology: $ONTOLOGY_NAME"
echo ""

# Test HermiT
echo "--- HermiT ---"
if docker images | grep -q "owl-reasoner-hermit"; then
    start_time=$(date +%s%N)
    docker run --rm -v "$BENCHMARK_DIR/ontologies:/ontologies:ro" \
        owl-reasoner-hermit "/ontologies/$ONTOLOGY_NAME" consistency 2>&1 | tail -5
    end_time=$(date +%s%N)
    hermit_ms=$(( (end_time - start_time) / 1000000 ))
    echo "Total time: ${hermit_ms}ms"
else
    echo "HermiT image not built. Run: docker build -f docker/Dockerfile.hermit -t owl-reasoner-hermit ."
fi

echo ""

# Test Tableauxx
echo "--- Tableauxx ---"
if [ -f "$PROJECT_ROOT/target/release/owl2-reasoner" ]; then
    start_time=$(date +%s%N)
    "$PROJECT_ROOT/target/release/owl2-reasoner" check "$ONTOLOGY" 2>&1
    end_time=$(date +%s%N)
    tableauxx_ms=$(( (end_time - start_time) / 1000000 ))
    echo ""
    echo "{\"reasoner\": \"Tableauxx\", \"duration_ms\": $tableauxx_ms, \"status\": \"completed\"}"
else
    echo "Tableauxx binary not found. Run: cargo build --release --bin owl2-reasoner"
fi

echo ""
echo "=========================================="
echo "Comparison Summary"
echo "=========================================="
