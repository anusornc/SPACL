#!/usr/bin/env bash
set -euo pipefail

# Run the SPACL split-stack query verdict (ADR 0027, #201).
#
# The wrapper re-runs the accepted SPACL verdict with the split-stack
# declaration the campaign's owl2bench-query contract validates.  It reuses
# the pinned inputs of the accepted run: the seed-1 profile ontologies (the
# dataset anchors), the shipped derived oracle, the pinned Oxigraph query
# backend digest, and the project reasoner binary.  Loads materialize each
# profile context first so the builder can source every identity from the
# adapters' pinned provenance; the run then re-loads the same contexts, and
# any nondeterminism between the two materializations fails the run closed
# through the profile result contract instead of being retried.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

VERDICT_ROOT="${SPACL_VERDICT_ROOT:-$ROOT_DIR/benchmarks/external/owl2bench/verdict-stack-shipped-seed1}"
DATA_DIR="${SPACL_VERDICT_DATA_DIR:-$VERDICT_ROOT/data}"
STACK_DIR="${SPACL_VERDICT_STACK_DIR:-$VERDICT_ROOT/stack}"
RESULTS_ROOT="${SPACL_VERDICT_RESULTS_ROOT:-$VERDICT_ROOT/results}"
RUN_ID="${SPACL_VERDICT_RUN_ID:-spacl-stack-shipped-verdict-seed1-split}"
PRIOR_RUN_DIR="${SPACL_VERDICT_PRIOR_RUN:-$VERDICT_ROOT/results/spacl-stack-shipped-verdict-seed1}"
ORACLE_DIR="${SPACL_VERDICT_ORACLE_DIR:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/derived-oracle-shipped-combined-v2}"
QUERY_MANIFEST="${SPACL_VERDICT_QUERY_MANIFEST:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv}"
DATASET_METADATA="${SPACL_VERDICT_DATASET_METADATA:-$ORACLE_DIR/dataset-metadata.json}"
CAPABILITIES_FILE="$SCRIPT_DIR/spacl-stack-capabilities.json"
BUILDER="$SCRIPT_DIR/spacl_stack_manifest.py"
CANONICALIZER="$SCRIPT_DIR/canonicalize_result.py"
TIMEOUT_SECONDS="${SPACL_VERDICT_TIMEOUT_SECONDS:-3600}"

fail() { echo "[spacl-stack-verdict][error] $1" >&2; exit 1; }

[[ -d "$DATA_DIR" ]] || fail "profile data directory is missing: $DATA_DIR"
[[ -f "$ORACLE_DIR/derived-oracle-manifest.json" ]] || fail "derived oracle manifest is missing"
[[ -f "$QUERY_MANIFEST" ]] || fail "query manifest is missing: $QUERY_MANIFEST"
[[ -f "$DATASET_METADATA" ]] || fail "dataset anchor metadata is missing: $DATASET_METADATA"
[[ -x "$SCRIPT_DIR/split_profile_engine.sh" ]] || fail "split profile engine is missing"
[[ -r "$CANONICALIZER" ]] || fail "the oracle-pinned canonicalizer is missing: $CANONICALIZER"
oracle_canonicalizer_version="$(python3 "$CANONICALIZER" --version)"
pinned_canonicalizer_version="$(jq -r '.oracles[0].canonicalizer_version' "$ORACLE_DIR/derived-oracle-manifest.json")"
[[ "$oracle_canonicalizer_version" == "$pinned_canonicalizer_version" ]] \
  || fail "canonicalizer pin does not match the oracle identity: $oracle_canonicalizer_version != $pinned_canonicalizer_version"

export OWL2BENCH_SPLIT_DL_ENGINE="$SCRIPT_DIR/spacl_dl_engine.sh"
export OWL2BENCH_SPLIT_EL_ENGINE="$SCRIPT_DIR/spacl_el_engine.sh"
export OWL2BENCH_SPLIT_QL_ENGINE="$SCRIPT_DIR/spacl_ql_engine.sh"
export OWL2BENCH_SPLIT_RL_ENGINE="$SCRIPT_DIR/spacl_rl_engine.sh"

# The dataset anchors pin the verdict inputs: the profile ontologies on disk
# must hash to the anchor metadata before anything is declared.
for profile in DL EL QL RL; do
  ontology="$(find "$DATA_DIR/$profile" -maxdepth 1 -type f -name '*.owl' | head -1)"
  [[ -n "$ontology" ]] || fail "profile $profile has no seed ontology under $DATA_DIR/$profile"
  actual="sha256:$(sha256sum "$ontology" | awk '{print $1}')"
  anchor="sha256:$(jq -r --arg profile "$profile" '.[$profile].dataset_sha256' "$DATASET_METADATA")"
  [[ "$actual" == "$anchor" ]] \
    || fail "profile $profile ontology does not match the dataset anchor: $ontology"
done

# Loads first: the adapter materializes each context and writes the pinned
# provenance and handoff the declaration is sourced from.
mkdir -p "$STACK_DIR"
for profile in DL EL QL RL; do
  echo "[spacl-stack-verdict] load $profile" >&2
  timeout "$TIMEOUT_SECONDS" "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_DIR/$profile" \
    > "$STACK_DIR/load-$profile.output" 2> "$STACK_DIR/load-$profile.diagnostics" \
    || fail "profile $profile load failed: $(tail -n 3 "$STACK_DIR/load-$profile.diagnostics" 2>/dev/null || true)"
done

python3 "$BUILDER" build \
  --data-dir "$DATA_DIR" \
  --capabilities "$CAPABILITIES_FILE" \
  --dataset-metadata "$DATASET_METADATA" \
  --reasoner-version "$(awk -F '\"' '/^version = / {print $2; exit}' "$ROOT_DIR/Cargo.toml")" \
  --generator-revision "$(tail -n +2 "$QUERY_MANIFEST" | head -1 | cut -f7)" \
  --timeout-seconds "$TIMEOUT_SECONDS" \
  --output-dir "$STACK_DIR" \
  || fail "split-stack manifests could not be built from the pinned provenance"

python3 "$SCRIPT_DIR/materialization_manifest.py" verify-stack \
  --component-manifest "$STACK_DIR/component.json" \
  --materialization-manifest "$STACK_DIR/materialization.json" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$STACK_DIR/stack-validation.json" \
  || fail "the SPACL split stack failed cross-contract verification"
jq -e '.status == "complete" and (.errors | length == 0)' "$STACK_DIR/stack-validation.json" >/dev/null \
  || fail "split stack validation is not clean"

echo "[spacl-stack-verdict] run $RUN_ID" >&2
env \
  OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$STACK_DIR/component.json" \
  OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$STACK_DIR/materialization.json" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
  OWL2BENCH_QUERY_ORACLE_MANIFEST="$ORACLE_DIR/derived-oracle-manifest.json" \
  OWL2BENCH_QUERY_DATASET_METADATA="$DATASET_METADATA" \
  OWL2BENCH_QUERY_CANONICALIZER="$CANONICALIZER" \
  TIMEOUT_SECONDS="$TIMEOUT_SECONDS" \
  RUN_ID="$RUN_ID" \
  "$SCRIPT_DIR/run.sh"

RUN_DIR="$RESULTS_ROOT/$RUN_ID"
SUMMARY="$RUN_DIR/summary.json"
[[ -f "$SUMMARY" ]] || fail "the verdict run produced no summary"

# The run re-loaded every profile context; the graphs must still hash to the
# declared materialization outputs or the declaration described a different
# graph than the one queried.
for profile in DL EL QL RL; do
  graph="$(jq -r --arg profile "$profile" '.profiles[$profile].output_graph.path' "$STACK_DIR/materialization.json")"
  declared="$(jq -r --arg profile "$profile" '.profiles[$profile].output_graph.sha256' "$STACK_DIR/materialization.json")"
  actual="sha256:$(sha256sum "$graph" | awk '{print $1}')"
  [[ "$actual" == "$declared" ]] \
    || fail "profile $profile materialization is not deterministic: $graph changed during the run"
done

jq -e '
  .query_count == 60
  and .split_stack_requested == true
  and .profile_gates.DL == "passed"
  and .profile_gates.EL == "passed"
  and .profile_gates.QL == "passed"
  and .profile_gates.RL == "passed"
  and .query_gate == "passed"
  and .correctness_gate == "passed"
  and .component_status_counts == {"available": 60}
  and .materialization_status_counts == {"complete": 60}
  and .result_status_counts == {"passed": 60}
' "$SUMMARY" >/dev/null || fail "the split-stack verdict did not reproduce the accepted gate"

for profile in DL EL QL RL; do
  accepted="$(jq -e --arg profile "$profile" \
    '([.profiles[$profile].oracle_identities[] | select(.match_reason == "accepted")] | length) == .profiles[$profile].case_count' \
    "$SUMMARY" >/dev/null && echo yes || echo no)"
  [[ "$accepted" == "yes" ]] \
    || fail "profile $profile has oracle matches that were not accepted; this is a blocker, not a retry"
done

# Canonical counts must be unchanged against the prior accepted run. Hash
# equality is enforced only within one canonicalizer era: an era change
# (for example v1 -> v2 RDF 1.1 simple-literal normalization, ADR 0023)
# legitimately changes every canonical hash, so across eras only the result
# status counts are compared and the era difference is recorded.
if [[ -f "$PRIOR_RUN_DIR/summary.json" ]]; then
  prior_canonicalizer="$(jq -r 'first(.canonicalizer_identities[]?.version // .profiles.DL.canonicalizer_identities[]?.version // empty)' "$PRIOR_RUN_DIR/summary.json")"
  current_canonicalizer="$(python3 "$CANONICALIZER" --version)"
  for profile in DL EL QL RL; do
    prior_counts="$(jq -r --arg profile "$profile" '.profiles[$profile].result_status_counts | tojson' "$PRIOR_RUN_DIR/summary.json")"
    rerun_counts="$(jq -r --arg profile "$profile" '.profiles[$profile].result_status_counts | tojson' "$SUMMARY")"
    [[ "$prior_counts" == "$rerun_counts" ]] \
      || fail "profile $profile result status counts changed vs the accepted run"
    if [[ "$prior_canonicalizer" == "$current_canonicalizer" ]]; then
      prior_hashes="$(jq -r --arg profile "$profile" '.profiles[$profile].oracle_canonical_result_hashes | sort | tojson' "$PRIOR_RUN_DIR/summary.json")"
      rerun_hashes="$(jq -r --arg profile "$profile" '.profiles[$profile].oracle_canonical_result_hashes | sort | tojson' "$SUMMARY")"
      [[ "$prior_hashes" == "$rerun_hashes" ]] \
        || fail "profile $profile canonical oracle hashes changed vs the accepted run"
    fi
  done
  if [[ "$prior_canonicalizer" == "$current_canonicalizer" ]]; then
    echo "[spacl-stack-verdict] canonical counts unchanged vs $PRIOR_RUN_DIR" >&2
  else
    echo "[spacl-stack-verdict] counts unchanged vs $PRIOR_RUN_DIR; hash comparison skipped across canonicalizer era ($prior_canonicalizer -> $current_canonicalizer)" >&2
  fi
else
  echo "[spacl-stack-verdict] prior accepted run not found; skipped comparison: $PRIOR_RUN_DIR" >&2
fi

echo "[spacl-stack-verdict] summary=$SUMMARY"
echo "[spacl-stack-verdict] report=$RUN_DIR/report.md"
echo "[spacl-stack-verdict] query_gate=passed split_stack=declared"
