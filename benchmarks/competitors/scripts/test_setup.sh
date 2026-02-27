#!/bin/bash
#
# Quick test of benchmark infrastructure
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$BENCHMARK_DIR/../.." && pwd)"

echo "=== OWL2 Reasoner Benchmark Setup Test ==="
echo

# Check Docker
echo "Checking Docker..."
if command -v docker &> /dev/null; then
    docker --version
    echo "✓ Docker available"
else
    echo "✗ Docker not found"
    exit 1
fi

# Check Docker Compose
echo
echo "Checking Docker Compose..."
if docker compose version &> /dev/null || docker-compose version &> /dev/null; then
    echo "✓ Docker Compose available"
else
    echo "⚠ Docker Compose not found (optional)"
fi

# Check jq (for JSON processing)
echo
echo "Checking jq..."
if command -v jq &> /dev/null; then
    echo "✓ jq available"
else
    echo "⚠ jq not found (optional, for report generation)"
fi

# Verify directory structure
echo
echo "Checking directory structure..."
for dir in docker scripts results ontologies; do
    if [ -d "$BENCHMARK_DIR/$dir" ]; then
        echo "✓ $dir/ exists"
    else
        echo "✗ $dir/ missing - creating..."
        mkdir -p "$BENCHMARK_DIR/$dir"
    fi
done

# Check Dockerfiles
echo
echo "Checking Dockerfiles..."
for dockerfile in docker/Dockerfile.{hermit,konclude,openllet,elk,jfact,pellet,factpp,tableauxx}; do
    if [ -f "$BENCHMARK_DIR/$dockerfile" ]; then
        echo "✓ $dockerfile exists"
    else
        echo "✗ $dockerfile missing"
    fi
done

# Copy test ontologies
echo
echo "Preparing test ontologies..."
mkdir -p "$BENCHMARK_DIR/ontologies"
if [ -d "$PROJECT_ROOT/tests/data" ]; then
    cp "$PROJECT_ROOT/tests/data"/*.owl "$BENCHMARK_DIR/ontologies/" 2>/dev/null || true
    count=$(find "$BENCHMARK_DIR/ontologies" -name "*.owl" | wc -l)
    echo "✓ Copied $count OWL files"
else
    echo "⚠ No tests/data directory found"
fi

# Test build SPACL only (fastest, most likely to work)
echo
echo "Testing SPACL Docker build..."
cd "$BENCHMARK_DIR"
if docker build -f docker/Dockerfile.tableauxx -t owl-reasoner-tableauxx-test "$PROJECT_ROOT" 2>&1 | tail -5; then
    echo "✓ SPACL image built successfully"
    
    # Quick test run if we have ontologies
    if [ -f "$BENCHMARK_DIR/ontologies/univ-bench.owl" ]; then
        echo
        echo "Running quick test..."
        docker run --rm \
            -v "$BENCHMARK_DIR/ontologies:/ontologies:ro" \
            owl-reasoner-tableauxx-test \
            /ontologies/univ-bench.owl consistency 2>&1 | tail -10
    fi
else
    echo "✗ SPACL build failed"
fi

# Cleanup test image
docker rmi owl-reasoner-tableauxx-test 2>/dev/null || true

echo
echo "=== Setup Test Complete ==="
echo
echo "To run full benchmarks:"
echo "  cd $BENCHMARK_DIR"
echo "  ./scripts/run_benchmarks.sh"
