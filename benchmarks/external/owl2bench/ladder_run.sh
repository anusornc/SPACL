#!/usr/bin/env bash
set -euo pipefail

# OWL2Bench consistency scalability sweep runner (issue #197, ADR 0025/0026).
#
# Executes exactly ONE ladder point (profile x universities x pass) through
# the generic comparative benchmark harness:
#
#   - the dataset is located in the committed ladder manifest (issue #196;
#     generator identity, invocation, size, profile, output sha256) and its
#     sha256 is verified before anything runs,
#   - the engine set follows ADR 0025: DL/QL/RL datasets run tableauxx,
#     hermit, konclude; the EL dataset adds elk,
#   - engines run serially under a uniform 300-second timeout (overridable
#     for operators; the sweep contract pins 300),
#   - the run id is always owl2bench_cmp_p<pass>_<profile>_u<universities>
#     (lowercase profile), which ladder_view.sh parses back,
#   - engine provenance (docker image digests, engine versions) is recorded
#     by the harness in run_metadata.json exactly as on the ORE leg, and the
#     dataset provenance is recorded in owl2bench_ladder_metadata.json
#     before the harness is invoked.
#
# Performance lane (ADR 0024/0026): this runner produces execution
# performance and completability evidence only.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="${OWL2BENCH_LADDER_RUN_BENCH:-$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh}"
LADDER_MANIFEST="${OWL2BENCH_LADDER_MANIFEST:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json}"
RESULTS_HISTORY="${OWL2BENCH_LADDER_RESULTS_HISTORY:-$ROOT_DIR/benchmarks/competitors/results/history}"

LADDER_METADATA_NAME="owl2bench_ladder_metadata.json"
EXPECTED_PASSES=(1 2 3)
LADDER_TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"

ladder_log() { printf '[owl2bench-ladder] %s\n' "$*"; }
ladder_err() { printf '[owl2bench-ladder][error] %s\n' "$*" >&2; }

if ! [[ "$LADDER_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [[ "$LADDER_TIMEOUT_SECONDS" -le 0 ]]; then
    ladder_err "TIMEOUT_SECONDS must be a positive integer: $LADDER_TIMEOUT_SECONDS"
    exit 1
fi

usage() {
    cat <<'EOF'
Usage: ladder_run.sh PROFILE UNIVERSITIES PASS [ACTION]

Executes one OWL2Bench scalability ladder point (profile x universities x
pass) through the generic comparative harness (ADR 0025/0026).

  PROFILE       DL|EL|QL|RL (case-insensitive)
  UNIVERSITIES  ladder point size, e.g. 1|2|5|10|20|50 (must exist in the
                ladder manifest)
  PASS          pass number 1|2|3 (three serial passes per ADR 0025)
  ACTION        harness action: prepare|build|run|report|all (default: all)

Env:
  OWL2BENCH_LADDER_MANIFEST        ladder manifest path (issue #196 shape)
  OWL2BENCH_LADDER_RESULTS_HISTORY history dir receiving the run dir
  OWL2BENCH_LADDER_RUN_BENCH       harness override (tests only)
  OWL2BENCH_LADDER_DRY_RUN=1       resolve, record, and print the plan
                                   without invoking the harness
  TIMEOUT_SECONDS                  per-case timeout (default 300, ADR 0025)
  SKIP_BUILD                       forwarded to the harness (default 1)
EOF
}

ladder_engines_for() {
    case "$1" in
        DL|QL|RL) printf '%s\n' "tableauxx,hermit,konclude" ;;
        EL) printf '%s\n' "tableauxx,hermit,konclude,elk" ;;
        *) return 1 ;;
    esac
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        ladder_err "Missing required command: $1"
        exit 1
    fi
}

PROFILE_ARG="${1:-}"
UNIVERSITIES_ARG="${2:-}"
PASS_ARG="${3:-}"
ACTION="${4:-all}"

if [[ -z "$PROFILE_ARG" || -z "$UNIVERSITIES_ARG" || -z "$PASS_ARG" ]]; then
    usage >&2
    exit 1
fi

PROFILE="$(printf '%s' "$PROFILE_ARG" | tr '[:lower:]' '[:upper:]')"
if [[ ! "$PROFILE" =~ ^(DL|EL|QL|RL)$ ]]; then
    ladder_err "Unsupported profile: $PROFILE_ARG (use DL|EL|QL|RL)"
    exit 1
fi
PROFILE_LOWER="$(printf '%s' "$PROFILE" | tr '[:upper:]' '[:lower:]')"

if ! [[ "$UNIVERSITIES_ARG" =~ ^[0-9]+$ ]] || [[ "$UNIVERSITIES_ARG" -le 0 ]]; then
    ladder_err "Universities must be a positive integer: $UNIVERSITIES_ARG"
    exit 1
fi
if ! [[ "$PASS_ARG" =~ ^[0-9]+$ ]] || [[ "$PASS_ARG" -le 0 ]]; then
    ladder_err "Pass must be a positive integer: $PASS_ARG"
    exit 1
fi
pass_ok=0
for expected_pass in "${EXPECTED_PASSES[@]}"; do
    [[ "$PASS_ARG" == "$expected_pass" ]] && pass_ok=1
done
if [[ "$pass_ok" != "1" ]]; then
    ladder_err "Pass must be one of ${EXPECTED_PASSES[*]} (three serial passes per ADR 0025): $PASS_ARG"
    exit 1
fi

case "$ACTION" in
    prepare|build|run|report|all) ;;
    *)
        ladder_err "Unsupported action: $ACTION (use prepare|build|run|report|all)"
        exit 1
        ;;
esac

require_cmd jq
require_cmd sha256sum

if [[ ! -x "$RUN_BENCH" && "${OWL2BENCH_LADDER_DRY_RUN:-0}" != "1" ]]; then
    ladder_err "Missing harness runner: $RUN_BENCH"
    exit 1
fi

if [[ ! -f "$LADDER_MANIFEST" ]]; then
    ladder_err "ladder manifest is missing or unreadable: $LADDER_MANIFEST"
    ladder_err "Generate the scalability ladder first (issue #196)."
    exit 1
fi

if ! jq -e . "$LADDER_MANIFEST" >/dev/null 2>&1; then
    ladder_err "ladder manifest is malformed: $LADDER_MANIFEST"
    exit 1
fi

# Exactly one manifest point per (profile, universities).
MATCHING_POINTS="$(jq -r --arg profile "$PROFILE" --argjson universities "$UNIVERSITIES_ARG" \
    '[.points[]? | select(.profile == $profile and .universities == $universities)] | length' \
    "$LADDER_MANIFEST")"
if [[ "$MATCHING_POINTS" != "1" ]]; then
    ladder_err "ladder manifest has no point for profile $PROFILE universities=$UNIVERSITIES_ARG" \
        "(matches: $MATCHING_POINTS, manifest: $LADDER_MANIFEST)"
    exit 1
fi

POINT_JSON="$(jq -c --arg profile "$PROFILE" --argjson universities "$UNIVERSITIES_ARG" \
    '.points[] | select(.profile == $profile and .universities == $universities)' \
    "$LADDER_MANIFEST")"

DATASET_PATH="$(jq -r '.output.path // ""' <<<"$POINT_JSON")"
DATASET_REL="$(jq -r '.output.repo_relative_path // ""' <<<"$POINT_JSON")"
DATASET_SHA="$(jq -r '.output.sha256 // ""' <<<"$POINT_JSON")"
POINT_SEED="$(jq -r '.seed // ""' <<<"$POINT_JSON")"
POINT_INVOCATION="$(jq -r '.invocation // ""' <<<"$POINT_JSON")"
GENERATOR_REVISION="$(jq -r '.generator_revision // ""' <<<"$POINT_JSON")"
GENERATOR_JAR_SHA="$(jq -r '.generator_jar_sha256 // ""' <<<"$POINT_JSON")"

if [[ -z "$DATASET_SHA" || -z "$POINT_INVOCATION" || -z "$GENERATOR_REVISION" || -z "$GENERATOR_JAR_SHA" ]]; then
    ladder_err "ladder manifest point $PROFILE u$UNIVERSITIES_ARG lacks provenance fields" \
        "(output.sha256, invocation, generator_revision, generator_jar_sha256): $LADDER_MANIFEST"
    exit 1
fi

DATASET_FILE=""
if [[ -n "$DATASET_PATH" && -f "$DATASET_PATH" ]]; then
    DATASET_FILE="$DATASET_PATH"
elif [[ -n "$DATASET_REL" && -f "$ROOT_DIR/$DATASET_REL" ]]; then
    DATASET_FILE="$ROOT_DIR/$DATASET_REL"
else
    ladder_err "dataset file for $PROFILE u$UNIVERSITIES_ARG not found" \
        "(manifest path: ${DATASET_PATH:-<empty>}; repo-relative: ${DATASET_REL:-<empty>})"
    exit 1
fi

DATASET_BASENAME="$(basename "$DATASET_FILE")"
ACTUAL_SHA_HEX="$(sha256sum "$DATASET_FILE" | cut -d' ' -f1)"
EXPECTED_SHA_HEX="${DATASET_SHA#sha256:}"
if [[ "$ACTUAL_SHA_HEX" != "$EXPECTED_SHA_HEX" ]]; then
    ladder_err "dataset sha256 mismatch for $PROFILE u$UNIVERSITIES_ARG: manifest sha256:$EXPECTED_SHA_HEX, file $DATASET_FILE is sha256:$ACTUAL_SHA_HEX"
    exit 1
fi

ENGINES="$(ladder_engines_for "$PROFILE")"
RUN_ID="owl2bench_cmp_p${PASS_ARG}_${PROFILE_LOWER}_u${UNIVERSITIES_ARG}"
RUN_DIR="$RESULTS_HISTORY/$RUN_ID"

if [[ -e "$RUN_DIR" ]]; then
    ladder_err "run directory already exists: $RUN_DIR (run ids are one shot; move it away before rerunning)"
    exit 1
fi

DATASET_BYTES="$(stat -c%s "$DATASET_FILE")"
MANIFEST_IDENTITY_SHA="$(jq -r '.identity_sha256 // ""' "$LADDER_MANIFEST")"
MANIFEST_SCHEMA_VERSION="$(jq -r '.schema_version // ""' "$LADDER_MANIFEST")"

mkdir -p "$RUN_DIR"
jq -n \
    --arg lane "owl2bench_consistency_scalability" \
    --arg lane_terms "execution performance and completability evidence only (ADR 0024/0026)" \
    --arg run_id "$RUN_ID" \
    --arg profile "$PROFILE" \
    --argjson universities "$UNIVERSITIES_ARG" \
    --argjson seed "${POINT_SEED:-0}" \
    --argjson pass "$PASS_ARG" \
    --arg operation "consistency" \
    --argjson timeout_seconds "$LADDER_TIMEOUT_SECONDS" \
    --arg engines "$ENGINES" \
    --arg harness_action "$ACTION" \
    --arg filename "$DATASET_BASENAME" \
    --arg sha256 "sha256:$ACTUAL_SHA_HEX" \
    --argjson bytes "$DATASET_BYTES" \
    --arg manifest_path "$LADDER_MANIFEST" \
    --arg manifest_identity "$MANIFEST_IDENTITY_SHA" \
    --argjson manifest_schema_version "${MANIFEST_SCHEMA_VERSION:-0}" \
    --arg point_invocation "$POINT_INVOCATION" \
    --arg generator_revision "$GENERATOR_REVISION" \
    --arg generator_jar_sha256 "$GENERATOR_JAR_SHA" \
    '{
        lane: $lane,
        lane_terms: $lane_terms,
        run_id: $run_id,
        profile: $profile,
        universities: $universities,
        seed: $seed,
        pass: $pass,
        operation: $operation,
        timeout_seconds: $timeout_seconds,
        engines: ($engines | split(",")),
        harness_action: $harness_action,
        dataset: {
            filename: $filename,
            sha256: $sha256,
            bytes: $bytes
        },
        dataset_manifest: {
            path: $manifest_path,
            schema_version: $manifest_schema_version,
            identity_sha256: $manifest_identity,
            point_invocation: $point_invocation,
            generator_revision: $generator_revision,
            generator_jar_sha256: $generator_jar_sha256
        },
        engine_provenance: "recorded by the harness in run_metadata.json (docker image digests and engine versions, ADR 0025)",
        statistic: "per-case repetition median of container-inclusive wall time across three serial passes (ADR 0025)"
    }' > "$RUN_DIR/$LADDER_METADATA_NAME"

ladder_log "run_id=$RUN_ID profile=$PROFILE u=$UNIVERSITIES_ARG pass=$PASS_ARG action=$ACTION"
ladder_log "engines=$ENGINES timeout=${LADDER_TIMEOUT_SECONDS}s dataset=$DATASET_BASENAME"
ladder_log "dataset sha256=$ACTUAL_SHA_HEX manifest=$LADDER_MANIFEST"

if [[ "${OWL2BENCH_LADDER_DRY_RUN:-0}" == "1" ]]; then
    ladder_log "Dry run complete (harness not invoked)"
    exit 0
fi

STAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/owl2bench-ladder.XXXXXX")"
cleanup_stage() { rm -rf "$STAGE_DIR"; }
trap cleanup_stage EXIT

if ! ln -f "$DATASET_FILE" "$STAGE_DIR/$DATASET_BASENAME" 2>/dev/null; then
    cp -f "$DATASET_FILE" "$STAGE_DIR/$DATASET_BASENAME"
fi
staged_count="$(find "$STAGE_DIR" -maxdepth 1 -type f -name '*.owl' | wc -l | tr -d '[:space:]')"
if [[ "$staged_count" != "1" ]]; then
    ladder_err "expected exactly one staged dataset, found $staged_count in $STAGE_DIR"
    exit 1
fi

ONTOLOGY_REGEX="^$(printf '%s' "$DATASET_BASENAME" | sed 's/[.]/\\./g')$"

export ONTOLOGIES_DIR_OVERRIDE="$STAGE_DIR"
export ONTOLOGY_SUITE="standard"
export INCLUDE_CHEBI="0"
export OPERATION="consistency"
export TIMEOUT_SECONDS="$LADDER_TIMEOUT_SECONDS"
export SKIP_BUILD="${SKIP_BUILD:-1}"
export RUN_ID="$RUN_ID"
export ONTOLOGY_REGEX="$ONTOLOGY_REGEX"
export REASONERS_OVERRIDE="$ENGINES"

"$RUN_BENCH" "$ACTION"
