#!/bin/sh
set -eu

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-elk.sh <ontology.owl> [operation]" >&2
    exit 2
fi

echo "{\"reasoner\": \"ELK\", \"version\": \"0.4.3\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\"}"

java -cp "/opt/reasoner/bin:/opt/reasoner/target/dependency/*" \
    OwlapiConsistencyRunner elk "$ONTOLOGY_FILE" "$OPERATION"
