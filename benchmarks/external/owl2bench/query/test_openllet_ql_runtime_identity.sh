#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ATTESTOR="$SCRIPT_DIR/openllet_ql_runtime_identity.py"
IDENTITY="$SCRIPT_DIR/openllet_ql_runtime_identity.json"
LOCK="$SCRIPT_DIR/openllet_ql_classpath.sha256"
DOCKERFILE="$SCRIPT_DIR/Dockerfile.openllet-ql"
POM="$SCRIPT_DIR/openllet_ql_pom.xml"
CONFIG="$SCRIPT_DIR/openllet_ql_runtime_config.json"
EXPORTER="$SCRIPT_DIR/OpenlletQlMaterializer.java"
BUILD_SCRIPT="$SCRIPT_DIR/openllet_ql_build_runtime.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

set +e
python3 "$ATTESTOR" attest --identity "$IDENTITY" --output "$TMP_DIR/missing-image-proof.json"
missing_image_proof_status="$?"
set -e
test "$missing_image_proof_status" = 125
jq -e '.status == "not_available" and ([.reasons[] | select(. == "image.inspect_missing")] | length) == 1' \
  "$TMP_DIR/missing-image-proof.json" >/dev/null

python3 - "$IDENTITY" "$TMP_DIR/image-inspect.json" <<'PY'
import json
import pathlib
import sys

identity = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(
        [
            {
                "Id": identity["runtime"]["image"]["digest"],
                "Architecture": "amd64",
                "Os": "linux",
                "RepoDigests": [identity["runtime"]["image"]["reference"]],
                "Config": {
                    "Labels": {
                        "org.opencontainers.image.revision": identity["runtime"]["variant_revision"]
                    }
                },
            }
        ]
    )
    + "\n",
    encoding="utf-8",
)
PY

python3 "$ATTESTOR" attest \
  --identity "$IDENTITY" \
  --image-inspect "$TMP_DIR/image-inspect.json" \
  --output "$TMP_DIR/attestation-with-image.json"
jq -e '
  .status == "available"
  and .component.name == "Openllet QL"
  and .runtime.image.local_attestation == "matched"
  and .runtime.variant_revision == "openllet-ql-runtime-v1"
  and .runtime.execution_network == "none"
  and .runtime.service_endpoint == "none"
  and .runtime.archive.committed == false
  and .runtime.archive.published == false
  and .classpath.jar_count == 78
  and (.classpath.jars | length) == 78
  and .license.status == "open_source_distribution_review_pending"
  and .resource_limits.network == "disabled"
' "$TMP_DIR/attestation-with-image.json" >/dev/null

jq '.[0].Config.Labels["org.opencontainers.image.revision"] = "unrelated-repository-head"' \
  "$TMP_DIR/image-inspect.json" > "$TMP_DIR/wrong-revision-inspect.json"
set +e
python3 "$ATTESTOR" attest \
  --identity "$IDENTITY" \
  --image-inspect "$TMP_DIR/wrong-revision-inspect.json" \
  --output "$TMP_DIR/wrong-revision.json"
wrong_revision_status="$?"
set -e
test "$wrong_revision_status" = 1
jq -e '.status == "failed" and ([.reasons[] | select(. == "image.variant_revision_mismatch")] | length) == 1' \
  "$TMP_DIR/wrong-revision.json" >/dev/null

! grep -q 'rev-parse HEAD' "$BUILD_SCRIPT"
grep -q 'runtime.variant_revision' "$BUILD_SCRIPT"

set +e
python3 "$ATTESTOR" attest \
  --identity "$IDENTITY" \
  --image-ref "tableauxx-openllet-ql:2.6.5" \
  --output "$TMP_DIR/mutable-tag.json" 2>"$TMP_DIR/mutable-tag.err"
mutable_status="$?"
set -e
test "$mutable_status" = 1
jq -e '.status == "failed" and ([.reasons[] | select(. == "image.reference_mutable_tag")] | length) == 1' \
  "$TMP_DIR/mutable-tag.json" >/dev/null

jq '.runtime.base_image.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$IDENTITY" > "$TMP_DIR/bad-base.json"
set +e
python3 "$ATTESTOR" attest --identity "$TMP_DIR/bad-base.json" --output "$TMP_DIR/bad-base-result.json"
bad_base_status="$?"
set -e
test "$bad_base_status" = 1
jq -e '.status == "failed" and ([.reasons[] | select(. == "base_image.digest_mismatch")] | length) == 1' \
  "$TMP_DIR/bad-base-result.json" >/dev/null

jq '.runtime.image.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$IDENTITY" > "$TMP_DIR/bad-image.json"
set +e
python3 "$ATTESTOR" attest --identity "$TMP_DIR/bad-image.json" --output "$TMP_DIR/bad-image-result.json"
bad_image_status="$?"
set -e
test "$bad_image_status" = 1
jq -e '.status == "failed" and ([.reasons[] | select(. == "image.digest_mismatch")] | length) == 1' \
  "$TMP_DIR/bad-image-result.json" >/dev/null

sed '$d' "$LOCK" > "$TMP_DIR/drifted-classpath.sha256"
set +e
python3 "$ATTESTOR" attest \
  --identity "$IDENTITY" \
  --classpath-lock "$TMP_DIR/drifted-classpath.sha256" \
  --output "$TMP_DIR/drifted-classpath.json"
classpath_status="$?"
set -e
test "$classpath_status" = 1
jq -e '.status == "failed" and ([.reasons[] | select(. == "classpath.lock_sha256_mismatch")] | length) == 1' \
  "$TMP_DIR/drifted-classpath.json" >/dev/null

set +e
python3 "$ATTESTOR" attest --identity "$TMP_DIR/missing-identity.json" --output "$TMP_DIR/missing.json"
missing_status="$?"
set -e
test "$missing_status" = 125
jq -e '.status == "not_available" and ([.reasons[] | select(. == "identity.missing")] | length) == 1' \
  "$TMP_DIR/missing.json" >/dev/null

jq 'del(.runtime.image)' "$IDENTITY" > "$TMP_DIR/missing-runtime.json"
set +e
python3 "$ATTESTOR" attest --identity "$TMP_DIR/missing-runtime.json" --output "$TMP_DIR/missing-runtime-result.json"
missing_runtime_status="$?"
set -e
test "$missing_runtime_status" = 125
jq -e '.status == "not_available" and ([.reasons[] | select(. == "runtime.image_missing")] | length) == 1' \
  "$TMP_DIR/missing-runtime-result.json" >/dev/null

python3 "$ATTESTOR" summarize-build-inputs \
  --dockerfile "$DOCKERFILE" \
  --pom "$POM" \
  --classpath-lock "$LOCK" \
  --configuration "$CONFIG" \
  --exporter "$EXPORTER" \
  --output "$TMP_DIR/build-inputs.json"
jq -e '
  .base_image.digest == "sha256:e2532c643e3fbc52e339b24b6a47196b46ec8c269e08da3cd49365db41d9d252"
  and .openllet_version == "2.6.5"
  and .owlapi_version == "5.5.1"
  and .classpath.jar_count == 78
  and .dockerfile.classpath_drift_check == true
  and .dockerfile.build_network_contract == "host-maven-resolution-only"
' "$TMP_DIR/build-inputs.json" >/dev/null

if [[ "${OPENLLET_QL_RUNTIME_REAL_BUILD:-0}" == "1" ]]; then
  real_build_output="${OPENLLET_QL_RUNTIME_REAL_BUILD_OUTPUT:-$TMP_DIR/real-build}"
  OPENLLET_QL_BUILD_OUTPUT="$real_build_output" "$BUILD_SCRIPT" >/dev/null
  jq -e '
    .status == "available"
    and .runtime.image.local_attestation == "matched"
    and .runtime.variant_revision == "openllet-ql-runtime-v1"
  ' "$real_build_output/openllet_ql_attestation.json" >/dev/null
fi

python3 "$SCRIPT_DIR/test_openllet_ql_variant_control.py"
