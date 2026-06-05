#!/bin/sh
set -eu

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-jfact.sh <ontology.owl> [operation]" >&2
    exit 2
fi

echo "{\"reasoner\": \"JFact\", \"version\": \"5.0.3\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\"}"

java --add-opens java.base/java.lang=ALL-UNNAMED \
    -cp "/opt/reasoner/bin:/opt/reasoner/target/dependency/*" \
    OwlapiConsistencyRunner jfact "$ONTOLOGY_FILE" "$OPERATION"
