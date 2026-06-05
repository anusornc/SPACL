#!/bin/sh
set -eu

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-openllet.sh <ontology.owl> [operation]" >&2
    exit 2
fi

echo "{\"reasoner\": \"Openllet\", \"version\": \"2.6.5\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\"}"

java -cp "/opt/reasoner/bin:/opt/reasoner/target/dependency/*" \
    OwlapiConsistencyRunner openllet "$ONTOLOGY_FILE" "$OPERATION"
