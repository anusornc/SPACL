#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDENTITY="$SCRIPT_DIR/openllet_ql_runtime_identity.json"
IMAGE_TAG="${OPENLLET_QL_IMAGE_TAG:-tableauxx-openllet-ql:2.6.5}"
OUTPUT_DIR="${OPENLLET_QL_BUILD_OUTPUT:-/tmp/tableauxx-openllet-ql-runtime-build}"
DOCKER_BIN="${OPENLLET_QL_DOCKER_BIN:-docker}"
REVISION="$(jq -er '.runtime.variant_revision | select(type == "string" and length > 0)' "$IDENTITY")"

mkdir -p "$OUTPUT_DIR"

"$DOCKER_BIN" build \
  --platform linux/amd64 \
  --network host \
  --provenance=false \
  --build-arg "BUILD_REVISION=$REVISION" \
  --file "$SCRIPT_DIR/Dockerfile.openllet-ql" \
  --tag "$IMAGE_TAG" \
  "$SCRIPT_DIR" \
  2>&1 | tee "$OUTPUT_DIR/openllet_ql_build.log"

image_id="$("$DOCKER_BIN" image inspect "$IMAGE_TAG" --format '{{.Id}}')"
[[ "$image_id" == sha256:* ]]
"$DOCKER_BIN" image inspect "$image_id" > "$OUTPUT_DIR/openllet_ql_image_inspect.json"
"$DOCKER_BIN" run --rm --network none --entrypoint /bin/sh "$image_id" \
  -lc 'cat /opt/reasoner/classpath.sha256' > "$OUTPUT_DIR/openllet_ql_classpath.sha256"
cmp "$SCRIPT_DIR/openllet_ql_classpath.sha256" "$OUTPUT_DIR/openllet_ql_classpath.sha256"
"$DOCKER_BIN" run --rm --network none --entrypoint /bin/sh "$image_id" \
  -lc 'cat /opt/reasoner/provenance.sha256' > "$OUTPUT_DIR/openllet_ql_provenance.sha256"

python3 "$SCRIPT_DIR/openllet_ql_runtime_identity.py" attest \
  --identity "$IDENTITY" \
  --image-ref "tableauxx-openllet-ql@$image_id" \
  --image-inspect "$OUTPUT_DIR/openllet_ql_image_inspect.json" \
  --output "$OUTPUT_DIR/openllet_ql_attestation.json"

jq . "$OUTPUT_DIR/openllet_ql_attestation.json"
