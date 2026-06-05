#!/bin/sh
set -eu

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-pellet.sh <ontology.owl> [operation]" >&2
    exit 2
fi

echo "{\"reasoner\": \"Pellet\", \"version\": \"2.3.3\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\"}"

java -cp "/opt/reasoner/bin:/opt/reasoner/target/dependency/*" \
    OwlapiConsistencyRunner pellet "$ONTOLOGY_FILE" "$OPERATION"
