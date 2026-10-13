#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/split_stack_manifest.py"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

STAGED_DIR="$TMP_DIR/staged"
mkdir -p "$STAGED_DIR"
QUERY_FILE="$STAGED_DIR/query.rq"
printf 'SELECT DISTINCT ?x WHERE { ?x ?p ?o }\n' > "$QUERY_FILE"
QUERY_MANIFEST="$STAGED_DIR/manifest.tsv"

python3 - "$QUERY_MANIFEST" "$QUERY_FILE" <<'PY'
import pathlib
import sys

manifest = pathlib.Path(sys.argv[1])
query_file = sys.argv[2]
profiles = [
    "EL,QL,DL", "EL,RL,DL", "EL,RL,DL", "EL,RL,DL", "EL,RL,DL", "EL,DL",
    "QL,RL,DL", "QL,RL,DL", "QL,RL,DL", "QL,RL,DL", "QL,RL,DL",
    "RL,DL", "RL,DL", "RL,DL", "RL,DL", "RL,DL", "DL", "DL",
    "EL,QL,RL,DL", "EL,QL,RL,DL", "EL,QL,RL,DL", "EL,QL,RL,DL",
]
lines = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for index, profile in enumerate(profiles, start=1):
    query_id = f"official_query_{index:03d}"
    lines.append(
        "\t".join(
            [
                query_file,
                f"official-source#{query_id}",
                query_id,
                profile,
                "official-text",
                "owl2bench-official",
                "revision-test",
            ]
        )
    )
manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

COMPONENT_MANIFEST="$TMP_DIR/components.json"
python3 - "$COMPONENT_MANIFEST" <<'PY'
import json
import pathlib
import sys

profiles = {}
for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1):
    digest = f"sha256:{index:064x}"
    profiles[profile] = {
        "components": {
            "engine": {
                "name": {"DL": "HermiT", "EL": "ELK", "QL": "QL-rules", "RL": "RL-rules"}[profile],
                "version": "1.0.0",
                "revision": f"{profile.lower()}-revision",
                "artifact_digest": digest,
            },
            "materializer": {
                "name": f"{profile.lower()}-materializer",
                "version": "1.0.0",
                "revision": f"{profile.lower()}-materializer-revision",
                "artifact_digest": digest,
            },
            "query": {
                "name": f"{profile.lower()}-query",
                "version": "1.0.0",
                "revision": f"{profile.lower()}-query-revision",
                "artifact_digest": digest,
            },
        },
        "ruleset": {
            "id": f"{profile.lower()}-ruleset",
            "version": "1.0.0",
            "revision": f"{profile.lower()}-ruleset-revision",
            "artifact_digest": digest,
            "conformance_boundary": f"{profile.lower()}-ruleset-boundary",
        },
        "configuration": {
            "id": f"{profile.lower()}-configuration",
            "digest": digest,
        },
        "runtime": {
            "id": f"{profile.lower()}-runtime",
            "dependencies": [
                {"name": "runtime-lib", "version": "1.0.0", "digest": digest},
            ],
        },
        "credentials": {"status": "not_required", "token": "must-not-be-copied"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
        "dataset": {
            "id": f"anchor-{profile.lower()}",
            "sha256": digest,
            "tbox_id": f"tbox-{profile.lower()}",
            "tbox_sha256": digest,
            "abox_id": f"abox-{profile.lower()}",
            "abox_sha256": digest,
        },
        "generator": {
            "id": "owl2bench-generator",
            "revision": "generator-revision",
            "seed": 1,
        },
        "profile_context": {
            "id": f"anchor-context-{profile.lower()}",
            "directory": f"/data/{profile}",
        },
    }

document = {
    "schema_version": 1,
    "manifest_id": "test-split-stack",
    "matrix": {
        "expansion": "query_profile",
        "expected_query_count": 60,
        "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18},
    },
    "profiles": profiles,
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
PY

OUTPUT="$TMP_DIR/validation.json"
python3 "$VALIDATOR" validate \
  --component-manifest "$COMPONENT_MANIFEST" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$OUTPUT"
jq -e '
  .status == "available"
  and .matrix_status == "available"
  and .case_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and ([.profiles[].status] | sort) == ["available","available","available","available"]
  and .profiles.DL.credentials.token == null
  and .profiles.DL.license.status == "not_required"
  and .profiles.DL.license.id == null
  and .profiles.DL.components.engine.name == "HermiT"
  and .profiles.DL.components.engine.artifact_digest == "sha256:0000000000000000000000000000000000000000000000000000000000000001"
  and .profiles.DL.configuration.digest == "sha256:0000000000000000000000000000000000000000000000000000000000000001"
  and .profiles.DL.runtime.dependencies[0].digest == "sha256:0000000000000000000000000000000000000000000000000000000000000001"
  and .profiles.DL.resource_limits.timeout_seconds == 600
  and .profiles.DL.dataset.tbox_id == "tbox-dl"
  and .profiles.DL.dataset.abox_id == "abox-dl"
  and ([.cases[] | select(.profile == "EL" and .query_id == "official_query_001")][0]
       | .case_id == "official_query_001__EL"
       and .official_query_source == "official-source#official_query_001"
       and .dataset.id == "anchor-el"
       and .generator.seed == 1
       and .profile_context.id == "anchor-context-el")
' "$OUTPUT" >/dev/null

# A provisioned license records only a secret-free identity and digest; both
# are required when the license status is available.
LICENSED_MANIFEST="$TMP_DIR/licensed.json"
jq '
  .profiles.RL.license = {
    status: "available",
    id: "enterprise-license-rl",
    digest: "sha256:0000000000000000000000000000000000000000000000000000000000000004"
  }
' "$COMPONENT_MANIFEST" > "$LICENSED_MANIFEST"
python3 "$VALIDATOR" validate \
  --component-manifest "$LICENSED_MANIFEST" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/licensed-validation.json"
jq -e '
  .status == "available"
  and .profiles.RL.license.status == "available"
  and .profiles.RL.license.id == "enterprise-license-rl"
  and .profiles.RL.license.digest == "sha256:0000000000000000000000000000000000000000000000000000000000000004"
' "$TMP_DIR/licensed-validation.json" >/dev/null
BAD_LICENSE="$TMP_DIR/bad-license.json"
jq '.profiles.RL.license = {status:"available"}' "$COMPONENT_MANIFEST" > "$BAD_LICENSE"
python3 "$VALIDATOR" validate \
  --component-manifest "$BAD_LICENSE" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/bad-license-validation.json"
jq -e '
  .status == "not_available"
  and .profiles.RL.status == "not_available"
  and ([.profiles.RL.reasons[] | select(. == "license.id_missing")] | length) == 1
  and ([.profiles.RL.reasons[] | select(. == "license.digest_invalid")] | length) == 1
' "$TMP_DIR/bad-license-validation.json" >/dev/null

# Profile dataset and context identities must remain disjoint.
SHARED_DATASET="$TMP_DIR/shared-dataset.json"
jq '.profiles.EL.dataset = .profiles.DL.dataset | .profiles.EL.profile_context = .profiles.DL.profile_context' "$COMPONENT_MANIFEST" > "$SHARED_DATASET"
python3 "$VALIDATOR" validate \
  --component-manifest "$SHARED_DATASET" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/shared-dataset-validation.json"
jq -e '
  .status == "failed"
  and .matrix_status == "failed"
  and ([.manifest_errors[] | select(. == "shared_profile_identity:dataset.id:DL,EL")] | length) == 1
  and ([.manifest_errors[] | select(. == "shared_profile_identity:profile_context.id:DL,EL")] | length) == 1
' "$TMP_DIR/shared-dataset-validation.json" >/dev/null

# A valid mixed-profile source row expands into independent profile-qualified cases.
jq -e '
  ([.cases[] | select(.query_id == "official_query_001") | .profile] | sort) == ["DL","EL","QL"]
  and ([.cases[] | select(.query_id == "official_query_001") | .case_id] | sort)
      == ["official_query_001__DL","official_query_001__EL","official_query_001__QL"]
' "$OUTPUT" >/dev/null

# A missing exact pin is an explicit profile-local not_available result, never a fallback.
MISSING_PIN="$TMP_DIR/missing-pin.json"
jq '.profiles.DL.components.engine.version = ""' "$COMPONENT_MANIFEST" > "$MISSING_PIN"
python3 "$VALIDATOR" validate \
  --component-manifest "$MISSING_PIN" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/missing-pin-validation.json"
jq -e '
  .status == "not_available"
  and .profiles.DL.status == "not_available"
  and .profiles.EL.status == "available"
  and ([.cases[] | select(.profile == "DL") | .component.status] | unique) == ["not_available"]
  and ([.cases[] | select(.profile == "EL") | .component.status] | unique) == ["available"]
' "$TMP_DIR/missing-pin-validation.json" >/dev/null

# A declared capability gap is unsupported, not an unavailable pin.
UNSUPPORTED="$TMP_DIR/unsupported.json"
jq '.profiles.QL.capability.status = "unsupported"' "$COMPONENT_MANIFEST" > "$UNSUPPORTED"
python3 "$VALIDATOR" validate \
  --component-manifest "$UNSUPPORTED" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/unsupported-validation.json"
jq -e '
  .status == "unsupported"
  and .profiles.QL.status == "unsupported"
  and ([.cases[] | select(.profile == "QL") | .component.status] | unique) == ["unsupported"]
' "$TMP_DIR/unsupported-validation.json" >/dev/null

# Answer-key fields are rejected even when nested inside a profile record.
NESTED_ANSWER_KEY="$TMP_DIR/nested-answer-key.json"
jq '.profiles.DL.answer_keys = [{"case_id":"must-not-exist"}]' "$COMPONENT_MANIFEST" > "$NESTED_ANSWER_KEY"
python3 "$VALIDATOR" validate \
  --component-manifest "$NESTED_ANSWER_KEY" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/nested-answer-key-validation.json"
jq -e '
  .status == "failed"
  and ([.manifest_errors[] | select(. == "answer_key_field_not_allowed:profiles.DL.answer_keys")] | length) == 1
' "$TMP_DIR/nested-answer-key-validation.json" >/dev/null

for forbidden_field in answer_file answer_sha256 oracle_key; do
  NESTED_FORBIDDEN="$TMP_DIR/nested-$forbidden_field.json"
  jq --arg key "$forbidden_field" \
    '.profiles.DL[$key] = "must-not-exist"' \
    "$COMPONENT_MANIFEST" > "$NESTED_FORBIDDEN"
  python3 "$VALIDATOR" validate \
    --component-manifest "$NESTED_FORBIDDEN" \
    --query-manifest "$QUERY_MANIFEST" \
    --output "$TMP_DIR/nested-$forbidden_field-validation.json"
  jq -e --arg field "$forbidden_field" '
    .status == "failed"
    and ([.manifest_errors[] | select(. == ("answer_key_field_not_allowed:profiles.DL." + $field))] | length) == 1
  ' "$TMP_DIR/nested-$forbidden_field-validation.json" >/dev/null
done

# Duplicate, unknown, and missing source rows remain invalid and are never hidden.
for kind in duplicate unknown missing; do
  BAD_MANIFEST="$TMP_DIR/$kind.tsv"
  cp "$QUERY_MANIFEST" "$BAD_MANIFEST"
  case "$kind" in
    duplicate)
      awk 'NF { line = $0 } END { print line }' "$QUERY_MANIFEST" >> "$BAD_MANIFEST"
      ;;
    unknown)
      printf '%s\tunknown-source\tunknown-query\tZZ\tofficial-text\towl2bench-official\trevision-test\n' "$QUERY_FILE" >> "$BAD_MANIFEST"
      ;;
    missing)
      printf '%s\tmissing-source\t\tEL\tofficial-text\towl2bench-official\trevision-test\n' "$QUERY_FILE" >> "$BAD_MANIFEST"
      ;;
  esac
  python3 "$VALIDATOR" validate \
    --component-manifest "$COMPONENT_MANIFEST" \
    --query-manifest "$BAD_MANIFEST" \
    --output "$TMP_DIR/$kind-validation.json"
  jq -e '
    .matrix_status == "failed"
    and .query_manifest_status == "failed"
    and ([.cases[] | select(.manifest_row_status == "invalid")] | length) >= 1
  ' "$TMP_DIR/$kind-validation.json" >/dev/null
done

# The query runner honors a profile-local not_available pin and does not
# silently execute those cases through the generic engine.
ENGINE="$TMP_DIR/fake-engine"
TRACE="$TMP_DIR/fake-engine.trace"
cat > "$ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" ]]
printf '%s\n' "${2:?}" >> "${TRACE:?}"
printf '?x\n<http://example.test/value>\n'
ENGINE
chmod +x "$ENGINE"
CAPABILITIES="$TMP_DIR/capabilities.json"
printf '{"supports_query":true,"profiles":["DL","EL","QL","RL"],"data_mode":"query-file-only"}\n' > "$CAPABILITIES"
RUN_ROOT="$TMP_DIR/runner-results"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MISSING_PIN" \
TRACE="$TRACE" \
RUN_ID="split-manifest-runner" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 22
  and .result_status_counts.oracle_unavailable == 38
  and .correctness_gate == "blocked"
' "$RUN_ROOT/split-manifest-runner/summary.json" >/dev/null
jq -e '
  .component_manifest.status == "not_available"
  and .component_manifest.profiles.DL.status == "not_available"
  and .component_manifest.profiles.EL.status == "available"
' "$RUN_ROOT/split-manifest-runner/run_metadata.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .component_manifest.status == "not_available"
  and (.component_manifest.reasons | index("component.engine.version_missing")) != null
' "$RUN_ROOT/split-manifest-runner/official_query_001__DL.json" >/dev/null
test "$(wc -l < "$TRACE" | tr -d '[:space:]')" = "38"

# Credential and runtime blockers remain profile-local while unaffected
# profiles continue through the execution path.
BOUNDARY_MANIFEST="$TMP_DIR/credential-runtime-boundary.json"
jq '
  .profiles.DL.credentials.status = "not_available"
  | .profiles.EL.runtime.dependencies = []
' "$COMPONENT_MANIFEST" > "$BOUNDARY_MANIFEST"
BOUNDARY_TRACE="$TMP_DIR/credential-runtime-boundary.trace"
BOUNDARY_RUN_ROOT="$TMP_DIR/credential-runtime-boundary-results"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$BOUNDARY_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$BOUNDARY_MANIFEST" \
TRACE="$BOUNDARY_TRACE" \
RUN_ID="credential-runtime-boundary" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 32
  and .result_status_counts.oracle_unavailable == 28
  and .correctness_gate == "blocked"
' "$BOUNDARY_RUN_ROOT/credential-runtime-boundary/summary.json" >/dev/null
jq -e '
  .component_manifest.profiles.DL.status == "not_available"
  and .component_manifest.profiles.EL.status == "not_available"
  and .component_manifest.profiles.QL.status == "available"
  and .component_manifest.profiles.RL.status == "available"
' "$BOUNDARY_RUN_ROOT/credential-runtime-boundary/run_metadata.json" >/dev/null
jq -e '
  .component_manifest.status == "not_available"
  and (.component_manifest.reasons | index("credentials.not_available")) != null
' "$BOUNDARY_RUN_ROOT/credential-runtime-boundary/official_query_001__DL.json" >/dev/null
jq -e '
  .component_manifest.status == "not_available"
  and (.component_manifest.reasons | index("runtime.dependencies_missing")) != null
' "$BOUNDARY_RUN_ROOT/credential-runtime-boundary/official_query_001__EL.json" >/dev/null
test "$(wc -l < "$BOUNDARY_TRACE" | tr -d '[:space:]')" = "28"

# A declared unsupported capability is preserved as unsupported and cannot
# fall back to another profile's component.
UNSUPPORTED_TRACE="$TMP_DIR/unsupported.trace"
UNSUPPORTED_RUN_ROOT="$TMP_DIR/unsupported-results"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$UNSUPPORTED_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$UNSUPPORTED" \
TRACE="$UNSUPPORTED_TRACE" \
RUN_ID="unsupported-profile" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.unsupported == 10
  and .result_status_counts.oracle_unavailable == 50
  and .correctness_gate == "blocked"
' "$UNSUPPORTED_RUN_ROOT/unsupported-profile/summary.json" >/dev/null
jq -e '
  .component_manifest.profiles.QL.status == "unsupported"
  and .component_manifest.profiles.DL.status == "available"
' "$UNSUPPORTED_RUN_ROOT/unsupported-profile/run_metadata.json" >/dev/null
jq -e '
  .result_status == "unsupported"
  and .component_manifest.status == "unsupported"
  and (.component_manifest.reasons | index("capability.unsupported")) != null
' "$UNSUPPORTED_RUN_ROOT/unsupported-profile/official_query_001__QL.json" >/dev/null
test "$(wc -l < "$UNSUPPORTED_TRACE" | tr -d '[:space:]')" = "50"

# Malformed component-manifest evidence remains failed, distinct from a
# missing/unavailable component pin.
INVALID_COMPONENT_MANIFEST="$TMP_DIR/invalid-component-manifest.json"
printf '{"schema_version":' > "$INVALID_COMPONENT_MANIFEST"
INVALID_TRACE="$TMP_DIR/invalid-component.trace"
: > "$INVALID_TRACE"
INVALID_RUN_ROOT="$TMP_DIR/invalid-component-results"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$INVALID_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$INVALID_COMPONENT_MANIFEST" \
TRACE="$INVALID_TRACE" \
RUN_ID="invalid-component-manifest" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.failed == 60
  and .correctness_gate == "blocked"
' "$INVALID_RUN_ROOT/invalid-component-manifest/summary.json" >/dev/null
jq -e '
  .component_manifest.status == "failed"
' "$INVALID_RUN_ROOT/invalid-component-manifest/run_metadata.json" >/dev/null
jq -e '
  .result_status == "failed"
  and .component_manifest.status == "failed"
' "$INVALID_RUN_ROOT/invalid-component-manifest/official_query_001__DL.json" >/dev/null
test ! -s "$INVALID_TRACE"

MISSING_TRACE="$TMP_DIR/missing-manifest-engine.trace"
: > "$MISSING_TRACE"
MISSING_RUN_ROOT="$TMP_DIR/missing-manifest-runner-results"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$MISSING_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_SPLIT_STACK=1 \
TRACE="$MISSING_TRACE" \
RUN_ID="split-manifest-missing" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 60
  and .correctness_gate == "blocked"
' "$MISSING_RUN_ROOT/split-manifest-missing/summary.json" >/dev/null
jq -e '
  .component_manifest.requested == true
  and .component_manifest.status == "not_available"
' "$MISSING_RUN_ROOT/split-manifest-missing/run_metadata.json" >/dev/null
test ! -s "$MISSING_TRACE"

echo "[test-owl2bench-split-stack-manifest] ok"
