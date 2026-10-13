#!/usr/bin/env bash
set -euo pipefail

# Run the existing query runner against the exact validated materialization
# stack. This wrapper deliberately permits profile-local unavailable/timeout
# outcomes so a ready profile is not hidden by another profile's blocker.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

STACK_DIR="${OWL2BENCH_PROFILE_STACK_DIR:-}"
QUERY_MANIFEST="${OWL2BENCH_PROFILE_QUERY_MANIFEST:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv}"
RESULTS_ROOT="${OWL2BENCH_PROFILE_RESULTS_ROOT:-$ROOT_DIR/benchmarks/external/owl2bench/query/results/history}"
RUN_ID="${RUN_ID:-owl2bench_profile_smoke_$(date +%Y%m%d_%H%M%S)}"

[[ -n "$STACK_DIR" && -d "$STACK_DIR" ]] || {
  echo "[owl2bench-profile-smoke][error] OWL2BENCH_PROFILE_STACK_DIR is required" >&2
  exit 1
}
[[ -r "$STACK_DIR/component.json" && -r "$STACK_DIR/materialization.json" ]] || {
  echo "[owl2bench-profile-smoke][error] stack manifests are missing under $STACK_DIR" >&2
  exit 1
}
STACK_METADATA="$STACK_DIR/stack.json"
[[ -r "$STACK_METADATA" ]] || {
  echo "[owl2bench-profile-smoke][error] assembled stack metadata is missing: $STACK_METADATA" >&2
  exit 1
}
[[ -r "$QUERY_MANIFEST" ]] || {
  echo "[owl2bench-profile-smoke][error] query manifest is missing: $QUERY_MANIFEST" >&2
  exit 1
}

MATRIX_CHECK="$(mktemp "${TMPDIR:-/tmp}/owl2bench-query-matrix.XXXXXX")"
set +e
python3 "$SCRIPT_DIR/split_stack_manifest.py" validate \
  --component-manifest "$STACK_DIR/component.json" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$MATRIX_CHECK"
matrix_status="$?"
set -e
if [[ "$matrix_status" -ne 0 ]] || ! jq -e '
  .query_manifest_status == "available"
  and (.query_manifest_errors | length) == 0
  and .matrix_status == "available"
  and .case_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .unclassified_case_count == 0
' "$MATRIX_CHECK" >/dev/null 2>&1; then
  echo "[owl2bench-profile-smoke][error] query manifest is not the exact 60-case profile matrix" >&2
  jq '{query_manifest_status,matrix_status,case_count,profile_case_counts,unclassified_case_count,query_manifest_errors}' "$MATRIX_CHECK" >&2 || true
  exit 1
fi

COMPONENT_SHA256="sha256:$(sha256sum "$STACK_DIR/component.json" | awk '{print $1}')"
MATERIALIZATION_SHA256="sha256:$(sha256sum "$STACK_DIR/materialization.json" | awk '{print $1}')"
if ! jq -e \
  --arg component_path "$STACK_DIR/component.json" \
  --arg component_sha256 "$COMPONENT_SHA256" \
  --arg materialization_path "$STACK_DIR/materialization.json" \
  --arg materialization_sha256 "$MATERIALIZATION_SHA256" \
  --arg stack_dir "$STACK_DIR" '
  .component_manifest.path == $component_path
  and .component_manifest.sha256 == $component_sha256
  and .materialization_manifest.path == $materialization_path
  and .materialization_manifest.sha256 == $materialization_sha256
  and .assembly_status == "complete"
  and .validation.path == ($stack_dir + "/stack-validation.json")
  and .dataset_manifest.path
  and (.profiles | type == "object")
  and ((["DL", "EL", "QL", "RL"] - (.profiles | keys)) | length == 0)
  and (.profiles | to_entries | all(
    (.value.status != "complete" or .value.graph_validation_status == "complete")
    and (.value.component_status != "available"
      or .value.materialization_status != "complete"
      or .value.status == "complete")
  ))
' "$STACK_METADATA" >/dev/null 2>&1; then
  echo "[owl2bench-profile-smoke][error] stack contains an unvalidated complete profile" >&2
  jq '{status,profiles}' "$STACK_METADATA" >&2 || true
  exit 1
fi
VALIDATION_PATH="$(jq -r '.validation.path' "$STACK_METADATA")"
VALIDATION_SHA256="$(jq -r '.validation.sha256' "$STACK_METADATA")"
DATASET_MANIFEST_PATH="$(jq -r '.dataset_manifest.path' "$STACK_METADATA")"
DATASET_MANIFEST_SHA256="$(jq -r '.dataset_manifest.sha256' "$STACK_METADATA")"
[[ -r "$VALIDATION_PATH" && -r "$DATASET_MANIFEST_PATH" ]] || {
  echo "[owl2bench-profile-smoke][error] recorded stack provenance artifacts are missing" >&2
  exit 1
}
ACTUAL_VALIDATION_SHA256="sha256:$(sha256sum "$VALIDATION_PATH" | awk '{print $1}')"
ACTUAL_DATASET_MANIFEST_SHA256="sha256:$(sha256sum "$DATASET_MANIFEST_PATH" | awk '{print $1}')"
[[ "$ACTUAL_VALIDATION_SHA256" == "$VALIDATION_SHA256" ]] || {
  echo "[owl2bench-profile-smoke][error] stack validation provenance hash mismatch" >&2
  exit 1
}
[[ "$ACTUAL_DATASET_MANIFEST_SHA256" == "$DATASET_MANIFEST_SHA256" ]] || {
  echo "[owl2bench-profile-smoke][error] dataset manifest provenance hash mismatch" >&2
  exit 1
}
STACK_STATUS="$(jq -r '.status' "$STACK_METADATA")"
STACK_PROFILES="$(jq '.profiles' "$STACK_METADATA")"
if ! jq -e --arg stack_status "$STACK_STATUS" --argjson stack_profiles "$STACK_PROFILES" '
  .status == $stack_status
  and (.graph_validations | type == "object")
  and ([.graph_validations | to_entries[]
    | .value.status == $stack_profiles[.key].graph_validation_status] | all)
' "$VALIDATION_PATH" >/dev/null 2>&1; then
  echo "[owl2bench-profile-smoke][error] stack validation status/profiles are stale or inconsistent" >&2
  exit 1
fi

export RUN_ID
export OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT"
export OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST"
export OWL2BENCH_QUERY_ENGINE_BIN="${OWL2BENCH_QUERY_ENGINE_BIN:-$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh}"
export OWL2BENCH_QUERY_CAPABILITIES_FILE="${OWL2BENCH_QUERY_CAPABILITIES_FILE:-$SCRIPT_DIR/materialized-graph-query-capabilities.json}"
export OWL2BENCH_QUERY_COMPONENT_MANIFEST="$STACK_DIR/component.json"
export OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$STACK_DIR/materialization.json"
export OWL2BENCH_QUERY_SPLIT_STACK=1
export OWL2BENCH_QUERY_EXECUTION_ONLY=1

exec "$SCRIPT_DIR/run.sh"
