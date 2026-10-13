#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATA_ROOT="$TMP_DIR/data"
TRACE="$TMP_DIR/trace"
mkdir -p "$DATA_ROOT"/{DL,EL,QL,RL}
for profile in DL EL QL RL; do
  printf '%s\n' "query" > "$DATA_ROOT/$profile/query.rq"
done

for profile in DL EL QL RL; do
  adapter="$TMP_DIR/adapter-$profile"
  cat > "$adapter" <<'ADAPTER'
#!/usr/bin/env bash
set -euo pipefail
profile="${ADAPTER_PROFILE:-${BASH_SOURCE[0]##*-}}"
command="${1:?}"
context="${2:?}"
printf '%s %s %s\n' "$profile" "$command" "$context" >> "${ADAPTER_TRACE:?}"
case "$command" in
  load)
    mkdir -p "$context/.split-${profile,,}"
    jq -n --arg profile "$profile" --arg context "$context" --arg adapter "split-$profile" \
      '{profile:$profile,adapter:$adapter,context_dir:$context,context_identity:("profile=" + $profile + ";directory=" + $context)}' \
      > "$context/.split-${profile,,}/provenance.json"
    ;;
  query)
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>'
    printf '%s\n' '<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><results><result><binding name="x"><uri>https://example.test/value</uri></binding></result></results></sparql>'
    ;;
  *) exit 2 ;;
esac
ADAPTER
  chmod +x "$adapter"
  export "ADAPTER_TRACE=$TRACE"
  case "$profile" in
    DL) export OWL2BENCH_SPLIT_DL_ENGINE="$adapter" ;;
    EL) export OWL2BENCH_SPLIT_EL_ENGINE="$adapter" ;;
    QL) export OWL2BENCH_SPLIT_QL_ENGINE="$adapter" ;;
    RL) export OWL2BENCH_SPLIT_RL_ENGINE="$adapter" ;;
  esac
done

for profile in DL EL QL RL; do
  "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_ROOT/$profile"
  "$SCRIPT_DIR/split_profile_engine.sh" query "$DATA_ROOT/$profile/query.rq" "$DATA_ROOT/$profile"
done

for profile in DL EL QL RL; do
  test "$(grep -c "^$profile " "$TRACE")" = 2
done

if env -u OWL2BENCH_SPLIT_DL_ENGINE -u OWL2BENCH_SPLIT_EL_ENGINE \
  -u OWL2BENCH_SPLIT_QL_ENGINE -u OWL2BENCH_SPLIT_RL_ENGINE \
  "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_ROOT/DL" \
  >/dev/null 2>"$TMP_DIR/missing-adapter.stderr"; then
  echo "assertion failed: router supplied an implicit adapter default" >&2
  exit 1
else
  test "$?" = 125
fi

# The same router must also feed the normal campaign runner, whose profile
# provenance lookup needs a profile-specific adapter/path map.
STAGED_DIR="$TMP_DIR/staged"
RESULTS_ROOT="$TMP_DIR/results"
mkdir -p "$STAGED_DIR"
printf '%s\n' 'SELECT ?x WHERE { ?x ?p ?o }' > "$STAGED_DIR/query.rq"
printf '%b\n' \
  'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision' \
  "$STAGED_DIR/query.rq\tofficial-query\tquery-001\tDL\tfixture\tfixture\tr1" \
  "$STAGED_DIR/query.rq\tofficial-query\tquery-002\tEL\tfixture\tfixture\tr1" \
  "$STAGED_DIR/query.rq\tofficial-query\tquery-003\tQL\tfixture\tfixture\tr1" \
  "$STAGED_DIR/query.rq\tofficial-query\tquery-004\tRL\tfixture\tfixture\tr1" \
  > "$STAGED_DIR/manifest.tsv"
cat > "$TMP_DIR/router-capabilities.json" <<'JSON'
{
  "supports_query": true,
  "profiles": ["DL", "EL", "QL", "RL"],
  "data_mode": "directory",
  "adapter": "split-profile-test",
  "provenance_adapters": {
    "DL": "split-DL",
    "EL": "split-EL",
    "QL": "split-QL",
    "RL": "split-RL"
  },
  "provenance_relative_paths": {
    "DL": ".split-dl/provenance.json",
    "EL": ".split-el/provenance.json",
    "QL": ".split-ql/provenance.json",
    "RL": ".split-rl/provenance.json"
  },
  "query_result_format": "sparql-results-xml"
}
JSON
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_MANIFEST="$STAGED_DIR/manifest.tsv" \
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/router-capabilities.json" \
RUN_ID="split-profile-runner" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.oracle_unavailable == 4
  and .correctness_gate == "blocked"
' "$RESULTS_ROOT/split-profile-runner/summary.json" >/dev/null
for profile in DL EL QL RL; do
  case "$profile" in
    DL) number=1 ;;
    EL) number=2 ;;
    QL) number=3 ;;
    RL) number=4 ;;
  esac
  jq -e --arg profile "$profile" --arg adapter "split-$profile" '
    .query_profile == $profile
    and .result_status == "oracle_unavailable"
    and .engine_provenance.adapter == $adapter
    and .profile_context.profile == $profile
  ' "$RESULTS_ROOT/split-profile-runner/query-00${number}__${profile}.json" >/dev/null
done

MISSING_MANIFEST_RESULTS="$TMP_DIR/missing-manifest-results"
FULL_STAGED_DIR="$TMP_DIR/full-staged"
mkdir -p "$FULL_STAGED_DIR"
cp "$STAGED_DIR/query.rq" "$FULL_STAGED_DIR/query.rq"
{
  printf '%b\n' 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision'
  for profile in DL EL QL RL; do
    case "$profile" in
      DL) count=22 ;;
      EL|QL) count=10 ;;
      RL) count=18 ;;
    esac
    for index in $(seq 1 "$count"); do
      printf '%b\n' "$FULL_STAGED_DIR/query.rq\tofficial-query\t${profile,,}-query-${index}\t$profile\tfixture\tfixture\tr1"
    done
  done
} > "$FULL_STAGED_DIR/manifest.tsv"
OWL2BENCH_QUERY_STAGED_DIR="$FULL_STAGED_DIR" \
OWL2BENCH_QUERY_MANIFEST="$FULL_STAGED_DIR/manifest.tsv" \
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$MISSING_MANIFEST_RESULTS" \
OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SCRIPT_DIR/split-profile-capabilities.json" \
RUN_ID="split-profile-missing-manifests" \
  "$SCRIPT_DIR/run.sh" >/dev/null
if ! jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 60
  and .correctness_gate == "blocked"
' "$MISSING_MANIFEST_RESULTS/split-profile-missing-manifests/summary.json" >/dev/null; then
  cat "$MISSING_MANIFEST_RESULTS/split-profile-missing-manifests/summary.json" >&2
  exit 1
fi
jq -e '
  .component_manifest.requested == true
  and .materialization.requested == true
' "$MISSING_MANIFEST_RESULTS/split-profile-missing-manifests/run_metadata.json" >/dev/null
test "$(wc -l < "$TRACE" | tr -d '[:space:]')" = 16

if "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_ROOT/unknown" >/dev/null 2>"$TMP_DIR/unknown.stderr"; then
  echo "assertion failed: unknown profile context was accepted" >&2
  exit 1
else
  test "$?" = 125
fi

echo "owl2bench split profile engine tests passed"
