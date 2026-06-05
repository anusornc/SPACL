#!/bin/sh
set -eu

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-hermit.sh <ontology.owl> [operation]" >&2
    exit 2
fi

echo "{\"reasoner\": \"HermiT\", \"version\": \"1.4.5.519\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\"}"

if [ "$OPERATION" != "consistency" ]; then
    echo "{\"duration_ms\": -1, \"status\": \"unsupported_operation\", \"reasoning_result\": \"unknown\"}"
    exit 2
fi

tmp_output=$(mktemp)
start_time=$(date +%s%N)

if java -cp "/opt/reasoner/target/dependency/*" \
    org.semanticweb.HermiT.cli.CommandLine -k "$ONTOLOGY_FILE" > "$tmp_output" 2>&1; then
    cmd_status=0
else
    cmd_status=$?
fi

end_time=$(date +%s%N)
duration_ms=$(( (end_time - start_time) / 1000000 ))

cat "$tmp_output"

reasoning_result="unknown"
if grep -qi "not satisfiable" "$tmp_output"; then
    reasoning_result="inconsistent"
elif grep -qi "is satisfiable" "$tmp_output"; then
    reasoning_result="consistent"
fi

# HermiT CLI may exit 0 even when parsing fails; detect fatal parser signatures.
if grep -Eqi "Could not parse ontology|It all went pear-shaped|Exception in thread|UnparsableOntologyException" "$tmp_output"; then
    cmd_status=1
    reasoning_result="unknown"
fi

if [ "$cmd_status" -eq 0 ]; then
    echo "{\"duration_ms\": $duration_ms, \"status\": \"completed\", \"reasoning_result\": \"$reasoning_result\"}"
else
    echo "{\"duration_ms\": $duration_ms, \"status\": \"failed\", \"reasoning_result\": \"$reasoning_result\"}"
fi

rm -f "$tmp_output"
exit "$cmd_status"
