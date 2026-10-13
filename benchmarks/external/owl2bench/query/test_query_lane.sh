#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
cleanup() {
  if [[ "${KEEP_QUERY_TEST_DIR:-0}" == "1" ]]; then
    echo "[test-owl2bench-query] retained test directory: $TMP_DIR" >&2
  else
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

SOURCE_DIR="$TMP_DIR/source"
STAGED_DIR="$TMP_DIR/staged"
ORACLE_DIR="$TMP_DIR/oracle"
mkdir -p "$SOURCE_DIR" "$ORACLE_DIR"
printf 'SELECT * WHERE { ?s ?p ?o }\n' > "$SOURCE_DIR/OWL2Bench-EL-query-1.rq"

mkdir -p "$TMP_DIR/official-source" "$TMP_DIR/official-staged"
cat > "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt" <<'QUERIES'
OWL2Bench SPARQL Queries

SELECT DISTINCT ?x WHERE { ?x :knows ?y }
Description: Find related instances.
Construct Involved: Reflexive Object Property.
Profile: EL, QL, DL

SELECT DISTINCT ?x ?y WHERE { ?x :isPartOf ?y }
Description: Find organization parts.
Construct Involved: Transitive Object Property.
Profile: EL, RL, DL
QUERIES

if OWL2BENCH_QUERY_SOURCE_DIR="$TMP_DIR/official-source" \
  OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
  OWL2BENCH_QUERY_SOURCE_ID="zenodo:3838735" \
  OWL2BENCH_QUERY_SOURCE_REVISION="87af702049661ae9c2979dd59281d83985aed8900bbcc682f5556058c0dbdb56" \
  "$SCRIPT_DIR/prepare.sh" >/dev/null; then
  jq -e '.query_count == 2 and .source_metadata_status == "verified"' \
    "$TMP_DIR/official-staged/prepare_summary.json" >/dev/null
  test "$(find "$TMP_DIR/official-staged" -maxdepth 1 -type f -name '*.rq' | wc -l | tr -d '[:space:]')" = "2"
else
  echo "assertion failed: official OWL2Bench query text should be supported" >&2
  exit 1
fi

OWL2BENCH_QUERY_SOURCE_DIR="$SOURCE_DIR" \
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_SOURCE_ID="owl2bench-test" \
OWL2BENCH_QUERY_SOURCE_REVISION="test-revision" \
"$SCRIPT_DIR/prepare.sh" >/dev/null

OWL2BENCH_QUERY_SOURCE_DIR="$SOURCE_DIR" \
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_SOURCE_ID="owl2bench-test" \
OWL2BENCH_QUERY_SOURCE_REVISION="test-revision" \
"$SCRIPT_DIR/prepare.sh" >/dev/null
test "$(find "$STAGED_DIR" -maxdepth 1 -type f -name '*.rq' | wc -l | tr -d '[:space:]')" = "1"
jq -e '.source_corpus == "owl2bench-test" and .source_revision == "test-revision" and .source_metadata_status == "verified"' "$STAGED_DIR/prepare_summary.json" >/dev/null

RUN_ROOT="$TMP_DIR/results"
RUN_ID="query-unavailable"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.not_available == 1 and .correctness_gate == "blocked"' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

cat > "$TMP_DIR/fake-engine" <<'ENGINE'
#!/usr/bin/env bash
[[ "${1:-}" == "query" ]]
query_file="${2:?}"
if grep -Eiq '\?x[[:space:]]+\?y' "$query_file"; then
  printf '?x\t?y\n"answer-a"\t\n'
elif grep -Eiq '\?x' "$query_file"; then
  printf '?x\n"answer-a"\n'
else
  printf '?value\n"answer-a"\n'
fi
ENGINE
chmod +x "$TMP_DIR/fake-engine"
cat > "$TMP_DIR/capabilities.json" <<'CAPABILITIES'
{"supports_query":true,"profiles":["EL"],"data_mode":"query-file-only"}
CAPABILITIES
cat > "$TMP_DIR/directory-capabilities.json" <<'CAPABILITIES'
{"supports_query":true,"profiles":["EL"],"data_mode":"directory"}
CAPABILITIES
cat > "$TMP_DIR/all-capabilities.json" <<'CAPABILITIES'
{"supports_query":true,"profiles":["DL","EL","QL","RL"],"data_mode":"query-file-only"}
CAPABILITIES
cat > "$TMP_DIR/all-directory-capabilities.json" <<'CAPABILITIES'
{"supports_query":true,"profiles":["DL","EL","QL","RL"],"data_mode":"directory"}
CAPABILITIES

PROFILE_DATA_ROOT="$TMP_DIR/profile-data"
mkdir -p "$PROFILE_DATA_ROOT"/DL "$PROFILE_DATA_ROOT"/EL "$PROFILE_DATA_ROOT"/QL "$PROFILE_DATA_ROOT"/RL
for profile in DL EL QL RL; do
  printf '%s\n' "$profile" > "$PROFILE_DATA_ROOT/$profile/context.marker"
done
cat > "$TMP_DIR/profile-aware-engine" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail

trace_file="${PROFILE_ENGINE_TRACE:?}"
case "${1:-}" in
  load)
    [[ $# -eq 2 ]]
    printf 'load\t%s\n' "$2" >> "$trace_file"
    [[ -f "$2/context.marker" ]]
    ;;
  query)
    [[ $# -eq 3 ]]
    printf 'query\t%s\t%s\n' "$2" "$3" >> "$trace_file"
    [[ -f "$3/context.marker" ]]
    if grep -Eiq '\?x[[:space:]]+\?y' "$2"; then
      printf '?x\t?y\n"answer-a"\t\n'
    elif grep -Eiq '\?x' "$2"; then
      printf '?x\n"answer-a"\n'
    else
      printf '?value\n"answer-a"\n'
    fi
    ;;
  *)
    exit 2
    ;;
esac
ENGINE
chmod +x "$TMP_DIR/profile-aware-engine"

PROFILE_ENGINE_TRACE="$TMP_DIR/profile-aware.trace"
RUN_ID="directory-profile-contexts"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/profile-aware-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-directory-capabilities.json" \
OWL2BENCH_QUERY_DATA_DIR="$PROFILE_DATA_ROOT" \
PROFILE_ENGINE_TRACE="$PROFILE_ENGINE_TRACE" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 6 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
jq -e '.profile_contexts.EL.directory == $dir and .profile_contexts.EL.identity == ("profile=EL;directory=" + $dir) and .profile_contexts.EL.status == "available" and .profile_contexts.EL.load_status == "success" and .profile_contexts.EL.load_time_ms >= 0' \
  --arg dir "$PROFILE_DATA_ROOT/EL" "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
jq -e '.profile_contexts.DL.identity == ("profile=DL;directory=" + $root + "/DL") and .profile_contexts.EL.identity == ("profile=EL;directory=" + $root + "/EL") and .profile_contexts.QL.identity == ("profile=QL;directory=" + $root + "/QL") and .profile_contexts.RL.identity == ("profile=RL;directory=" + $root + "/RL")' \
  --arg root "$PROFILE_DATA_ROOT" "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
test "$(awk -F '\t' '$1 == "load" {count++} END {print count + 0}' "$PROFILE_ENGINE_TRACE")" = "4"
test "$(awk -F '\t' '$1 == "query" {count++} END {print count + 0}' "$PROFILE_ENGINE_TRACE")" = "6"
expected_queries="$(printf '%s\t%s\n' \
  "$TMP_DIR/official-staged/0001_owl2bench_query_001.rq" "$PROFILE_DATA_ROOT/EL" \
  "$TMP_DIR/official-staged/0001_owl2bench_query_001.rq" "$PROFILE_DATA_ROOT/QL" \
  "$TMP_DIR/official-staged/0001_owl2bench_query_001.rq" "$PROFILE_DATA_ROOT/DL" \
  "$TMP_DIR/official-staged/0002_owl2bench_query_002.rq" "$PROFILE_DATA_ROOT/EL" \
  "$TMP_DIR/official-staged/0002_owl2bench_query_002.rq" "$PROFILE_DATA_ROOT/RL" \
  "$TMP_DIR/official-staged/0002_owl2bench_query_002.rq" "$PROFILE_DATA_ROOT/DL" | sort)"
actual_queries="$(awk -F '\t' '$1 == "query" {print $2 "\t" $3}' "$PROFILE_ENGINE_TRACE" | sort)"
test "$actual_queries" = "$expected_queries"
jq -e '.profile_context.directory == $dir and .profile_context.status == "available" and .profile_context.load_status == "success" and .load_time_ms >= 0 and .query_time_ms >= 0' \
  --arg dir "$PROFILE_DATA_ROOT/EL" "$RUN_ROOT/$RUN_ID/owl2bench_query_001__EL.json" >/dev/null
for profile_query in "DL owl2bench_query_001" "EL owl2bench_query_001" "QL owl2bench_query_001" "RL owl2bench_query_002"; do
  read -r profile query_id <<< "$profile_query"
  jq -e '.profile_context.profile == $profile and .profile_context.identity == ("profile=" + $profile + ";directory=" + $dir)' \
    --arg profile "$profile" --arg dir "$PROFILE_DATA_ROOT/$profile" \
    "$RUN_ROOT/$RUN_ID/${query_id}__${profile}.json" >/dev/null
done

MISSING_PROFILE_DATA_ROOT="$TMP_DIR/profile-data-missing"
mkdir -p "$MISSING_PROFILE_DATA_ROOT/EL"
printf 'EL\n' > "$MISSING_PROFILE_DATA_ROOT/EL/context.marker"
MISSING_PROFILE_ENGINE_TRACE="$TMP_DIR/profile-aware-missing.trace"
RUN_ID="directory-missing-profile-context"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/profile-aware-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-directory-capabilities.json" \
OWL2BENCH_QUERY_DATA_DIR="$MISSING_PROFILE_DATA_ROOT" \
PROFILE_ENGINE_TRACE="$MISSING_PROFILE_ENGINE_TRACE" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 2 and .result_status_counts.not_available == 4 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
test "$(awk -F '\t' '$1 == "load" {count++} END {print count + 0}' "$MISSING_PROFILE_ENGINE_TRACE")" = "1"
test "$(awk -F '\t' '$1 == "query" {count++} END {print count + 0}' "$MISSING_PROFILE_ENGINE_TRACE")" = "2"
jq -e '.profile_contexts.EL.status == "available" and .profile_contexts.QL.status == "not_available" and .profile_contexts.RL.status == "not_available" and .profile_contexts.DL.status == "not_available"' \
  "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
jq -e '.result_status == "oracle_unavailable" and .profile_context.status == "available" and .profile_context.directory == $dir' \
  --arg dir "$MISSING_PROFILE_DATA_ROOT/EL" "$RUN_ROOT/$RUN_ID/owl2bench_query_001__EL.json" >/dev/null
jq -e '.result_status == "not_available" and .profile_context.status == "not_available" and .profile_context.directory == $dir' \
  --arg dir "$MISSING_PROFILE_DATA_ROOT/QL" "$RUN_ROOT/$RUN_ID/owl2bench_query_001__QL.json" >/dev/null

OFFICIAL_MATRIX_DIR="$TMP_DIR/official-matrix"
mkdir -p "$OFFICIAL_MATRIX_DIR"
printf 'SELECT DISTINCT ?x WHERE { ?x :knows ?y }\n' > "$OFFICIAL_MATRIX_DIR/query.rq"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n' > "$OFFICIAL_MATRIX_DIR/manifest.tsv"
official_profiles=(
  "EL,QL,DL" "EL,RL,DL" "EL,RL,DL" "EL,RL,DL" "EL,RL,DL" "EL,DL"
  "QL,RL,DL" "QL,RL,DL" "QL,RL,DL" "QL,RL,DL" "QL,RL,DL"
  "RL,DL" "RL,DL" "RL,DL" "RL,DL" "RL,DL" "DL" "DL"
  "EL,QL,RL,DL" "EL,QL,RL,DL" "EL,QL,RL,DL" "EL,QL,RL,DL"
)
for index in "${!official_profiles[@]}"; do
  query_id="official_query_$(printf '%03d' "$((index + 1))")"
  printf '%s\tofficial-source#%s\t%s\t%s\tofficial-text\tzenodo:3838735\tofficial-revision\n' \
    "$OFFICIAL_MATRIX_DIR/query.rq" "$query_id" "$query_id" "${official_profiles[$index]}" \
    >> "$OFFICIAL_MATRIX_DIR/manifest.tsv"
done
RUN_ID="official-22-to-60"
OWL2BENCH_QUERY_STAGED_DIR="$OFFICIAL_MATRIX_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 60 and .result_status_counts.not_available == 60 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
jq -e '.query_case_count == 60' "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
official_result_files="$RUN_ROOT/$RUN_ID"
test "$(find "$official_result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' | wc -l | tr -d '[:space:]')" = "60"
test "$(find "$official_result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 | xargs -0 jq -r '.query_profile' | sort | uniq -c | awk '{print $2 "=" $1}' | sort | paste -sd, -)" = "DL=22,EL=10,QL=10,RL=18"

RUN_ID="query-profile-matrix"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-capabilities.json" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 6 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
jq -e '.query_case_count == 6' "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
result_files="$RUN_ROOT/$RUN_ID"
test "$(find "$result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' | wc -l | tr -d '[:space:]')" = "6"
test "$(find "$result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 | xargs -0 jq -r '.query_profile' | sort | paste -sd, -)" = "DL,DL,EL,EL,QL,RL"
test "$(find "$result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 | xargs -0 jq -r '.query_id' | sort -u | paste -sd, -)" = "owl2bench_query_001,owl2bench_query_002"
test "$(find "$result_files" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 | xargs -0 jq -r '.case_id' | sort -u | wc -l | tr -d '[:space:]')" = "6"

MALFORMED_STAGED_DIR="$TMP_DIR/malformed-staged"
mkdir -p "$MALFORMED_STAGED_DIR"
cp "$TMP_DIR/official-staged/0001_owl2bench_query_001.rq" "$MALFORMED_STAGED_DIR/malformed-query.rq"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n%s\t%s\tmalformed-query\tBOGUS\tofficial-text\ttest\trevision\n%s\t%s\tempty-query\t\tofficial-text\ttest\trevision\n' \
  "$MALFORMED_STAGED_DIR/malformed-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#malformed-query" \
  "$MALFORMED_STAGED_DIR/malformed-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#empty-query" \
  > "$MALFORMED_STAGED_DIR/manifest.tsv"
printf '%s\t%s\tempty-path\tEL\tofficial-text\ttest\trevision\n' \
  "" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#empty-path" >> "$MALFORMED_STAGED_DIR/manifest.tsv"
printf '%s\t%s\ttruncated-query\tEL\tofficial-text\n' \
  "$MALFORMED_STAGED_DIR/malformed-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#truncated-query" >> "$MALFORMED_STAGED_DIR/manifest.tsv"
printf '%s\t%s\textra-query\tEL\tofficial-text\ttest\trevision\textra-field\n' \
  "$MALFORMED_STAGED_DIR/malformed-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#extra-query" >> "$MALFORMED_STAGED_DIR/manifest.tsv"
printf '%s\t%s\ttrailing-query\tEL\tofficial-text\ttest\trevision\t\n' \
  "$MALFORMED_STAGED_DIR/malformed-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#trailing-query" >> "$MALFORMED_STAGED_DIR/manifest.tsv"
mkdir -p "$TMP_DIR/malformed-oracle"
printf 'answer-a\n' > "$TMP_DIR/malformed-oracle/trailing-query.out"
RUN_ID="query-malformed-profile"
OWL2BENCH_QUERY_STAGED_DIR="$MALFORMED_STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/malformed-oracle" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.failed == 6 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
malformed_result="$RUN_ROOT/$RUN_ID/malformed-query__BOGUS.json"
jq -e '.query_id == "malformed-query" and .query_profile == "BOGUS" and .case_id == "malformed-query__BOGUS" and .result_status == "failed"' \
  "$malformed_result" >/dev/null
empty_result="$RUN_ROOT/$RUN_ID/empty-query__unknown.json"
jq -e '.query_id == "empty-query" and .query_profile == "unknown" and .case_id == "empty-query__unknown" and .result_status == "failed"' \
  "$empty_result" >/dev/null
for invalid_query in empty-path truncated-query extra-query trailing-query; do
  invalid_result="$(find "$RUN_ROOT/$RUN_ID" -maxdepth 1 -type f -name "${invalid_query}__*.json" -print -quit)"
  test -n "$invalid_result"
  jq -e '.query_profile == "EL" and .result_status == "failed"' "$invalid_result" >/dev/null
done

mkdir -p "$TMP_DIR/official-oracle"
printf '?x\n"answer-a"\n' > "$TMP_DIR/official-oracle/owl2bench_query_001.out"
printf '?x\t?y\n"answer-a"\t\n' > "$TMP_DIR/official-oracle/owl2bench_query_002.out"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="official-multi-profile" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/official-oracle" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 2 and .result_status_counts.unsupported == 4 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/official-multi-profile/summary.json" >/dev/null

mkdir -p "$TMP_DIR/profile-oracle/EL"
printf '?x\n"answer-a"\n' > "$TMP_DIR/profile-oracle/EL/owl2bench_query_001.out"
printf '?x\t?y\n"answer-a"\t\n' > "$TMP_DIR/profile-oracle/EL/owl2bench_query_002.out"
RUN_ID="profile-qualified-oracle"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/profile-oracle" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 6 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

# A derived-oracle directory (derived-oracle-manifest.json, no manifest.json)
# is picked up from OWL2BENCH_QUERY_ORACLE_DIR alone; an empty oracle list
# still fails every case closed.
mkdir -p "$TMP_DIR/derived-oracle"
printf '{"schema_version":1,"oracles":[]}' > "$TMP_DIR/derived-oracle/derived-oracle-manifest.json"
RUN_ID="derived-manifest-default"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/derived-oracle" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e --arg manifest "$TMP_DIR/derived-oracle/derived-oracle-manifest.json" \
  '.oracle_manifest == $manifest' "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
jq -e '.query_count == 6 and .result_status_counts.oracle_unavailable == 6 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

# When both layouts exist the official manifest.json wins.
mkdir -p "$TMP_DIR/both-manifests-oracle"
printf '{"schema_version":1,"oracles":[]}' > "$TMP_DIR/both-manifests-oracle/manifest.json"
printf '{"schema_version":1,"oracles":[]}' > "$TMP_DIR/both-manifests-oracle/derived-oracle-manifest.json"
RUN_ID="derived-manifest-precedence"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR/official-staged" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/all-capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/both-manifests-oracle" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e --arg manifest "$TMP_DIR/both-manifests-oracle/manifest.json" \
  '.oracle_manifest == $manifest' "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null

NOISY_STAGED_DIR="$TMP_DIR/noisy-staged"
mkdir -p "$NOISY_STAGED_DIR"
cp "$TMP_DIR/official-staged/0001_owl2bench_query_001.rq" "$NOISY_STAGED_DIR/noisy-query.rq"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n%s\t%s\tnoisy-query\tEL,\tofficial-text\ttest\trevision\n' \
  "$NOISY_STAGED_DIR/noisy-query.rq" "$TMP_DIR/official-source/OWL2BENCH_SPARQL_Queries.txt#noisy-query" \
  > "$NOISY_STAGED_DIR/manifest.tsv"
mkdir -p "$TMP_DIR/noisy-oracle"
printf '?x\n"answer-a"\n' > "$TMP_DIR/noisy-oracle/noisy-query.out"
RUN_ID="noisy-single-profile"
OWL2BENCH_QUERY_STAGED_DIR="$NOISY_STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$TMP_DIR/noisy-oracle" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 1 and .result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

ROOT_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
ACTUAL_STAGED_DIR="$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged"
ACTUAL_MANIFEST="$ACTUAL_STAGED_DIR/manifest.tsv"
if [[ -f "$ACTUAL_MANIFEST" ]] && [[ -r "$(awk -F '\t' 'NR == 2 {print $1; exit}' "$ACTUAL_MANIFEST")" ]]; then
  test "$(tail -n +2 "$ACTUAL_MANIFEST" | wc -l | tr -d '[:space:]')" = "22"
  RUN_ID="official-staged-22-to-60"
  OWL2BENCH_QUERY_STAGED_DIR="$ACTUAL_STAGED_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
  RUN_ID="$RUN_ID" \
  "$SCRIPT_DIR/run.sh" >/dev/null
  jq -e '.query_count == 60 and .result_status_counts.not_available == 60 and .correctness_gate == "blocked"' \
    "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
  jq -e '.query_case_count == 60' "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null
fi

cat > "$TMP_DIR/bad-capabilities.json" <<'CAPABILITIES'
{"supports_query":true,"profiles":["EL"]}
CAPABILITIES
RUN_ID="query-bad-capabilities"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/bad-capabilities.json" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.not_available == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

RUN_ID="query-oracle-unavailable"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.oracle_unavailable == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

printf '?value\n"answer-a"\n' > "$ORACLE_DIR/OWL2Bench-EL-query-1.out"
RUN_ID="query-passed"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
jq -e '.query_profile == "EL" and .query_id == "OWL2Bench-EL-query-1" and .source_query != ""' "$RUN_ROOT/$RUN_ID/OWL2Bench-EL-query-1__EL.json" >/dev/null
jq -e '.oracle.status == "unavailable" and .oracle.match_reason == "oracle_manifest_unconfigured"' "$RUN_ROOT/$RUN_ID/OWL2Bench-EL-query-1__EL.json" >/dev/null
jq -e '.canonicalization.status == "available" and .canonicalization.canonicalizer_version == "owl2bench-query-canonicalizer/2" and (.canonicalization.result_hash | startswith("sha256:")) and .semantic_result == .canonicalization.result_hash' \
  "$RUN_ROOT/$RUN_ID/OWL2Bench-EL-query-1__EL.json" >/dev/null
jq -e '.canonicalizer.version == "owl2bench-query-canonicalizer/2" and .canonicalizer.name == "owl2bench-query-canonicalizer"' \
  "$RUN_ROOT/$RUN_ID/run_metadata.json" >/dev/null

OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
RUN_ID="query-passed" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.oracle_unavailable == 1' "$RUN_ROOT/query-passed/summary.json" >/dev/null

cat > "$TMP_DIR/malformed-result-engine" <<'ENGINE'
#!/usr/bin/env bash
printf '<sparql>'
ENGINE
chmod +x "$TMP_DIR/malformed-result-engine"
RUN_ID="query-malformed-result"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/malformed-result-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.failed == 1 and .correctness_gate == "blocked"' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
jq -e '.execution_status == "failed" and .result_status == "failed" and .canonicalization.status == "failed" and .canonicalization.result_hash == null' \
  "$RUN_ROOT/$RUN_ID/OWL2Bench-EL-query-1__EL.json" >/dev/null

cat > "$TMP_DIR/query-only-engine" <<'ENGINE'
#!/usr/bin/env bash
if [[ "${1:-}" == "load" ]]; then
  touch "${QUERY_ONLY_LOAD_MARKER:?}"
  exit 9
fi
printf '?value\n"answer-a"\n'
ENGINE
chmod +x "$TMP_DIR/query-only-engine"
QUERY_ONLY_LOAD_MARKER="$TMP_DIR/load-called"
RUN_ID="query-file-only-with-data-dir"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/query-only-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_DATA_DIR="$TMP_DIR/data-that-must-be-ignored" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
QUERY_ONLY_LOAD_MARKER="$QUERY_ONLY_LOAD_MARKER" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.oracle_unavailable == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
test ! -e "$QUERY_ONLY_LOAD_MARKER"

DIRECTORY_LOAD_MARKER="$TMP_DIR/directory-load-called"
RUN_ID="query-directory-without-data-dir"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/query-only-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/directory-capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
QUERY_ONLY_LOAD_MARKER="$DIRECTORY_LOAD_MARKER" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.not_available == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
test ! -e "$DIRECTORY_LOAD_MARKER"

NONEXISTENT_DATA_DIR="$TMP_DIR/does-not-exist"
RUN_ID="query-directory-with-nonexistent-data-dir"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/query-only-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/directory-capabilities.json" \
OWL2BENCH_QUERY_DATA_DIR="$NONEXISTENT_DATA_DIR" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
QUERY_ONLY_LOAD_MARKER="$DIRECTORY_LOAD_MARKER" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.not_available == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null
test ! -e "$DIRECTORY_LOAD_MARKER"

printf '?value\n"answer-b"\n' > "$ORACLE_DIR/OWL2Bench-EL-query-1.out"
RUN_ID="query-wrong"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

mkdir -p "$TMP_DIR/data/EL"
printf '?value\n"answer-a"\n' > "$ORACLE_DIR/OWL2Bench-EL-query-1.out"
RUN_ID="query-load-split"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/directory-capabilities.json" \
OWL2BENCH_QUERY_DATA_DIR="$TMP_DIR/data" \
OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.load_time_ms >= 0 and .query_time_ms >= 0' "$RUN_ROOT/$RUN_ID/OWL2Bench-EL-query-1__EL.json" >/dev/null

cat > "$TMP_DIR/failing-engine" <<'ENGINE'
#!/usr/bin/env bash
exit 7
ENGINE
chmod +x "$TMP_DIR/failing-engine"
RUN_ID="query-failed"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/failing-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.failed == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

cat > "$TMP_DIR/timeout-engine" <<'ENGINE'
#!/usr/bin/env bash
sleep 2
ENGINE
chmod +x "$TMP_DIR/timeout-engine"
RUN_ID="query-timeout"
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/timeout-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
TIMEOUT_SECONDS=1 \
RUN_ID="$RUN_ID" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.result_status_counts.timeout == 1' "$RUN_ROOT/$RUN_ID/summary.json" >/dev/null

echo "[test-owl2bench-query] ok"
