#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$TMP_DIR"/{w3c,ore,owl2bench,query,out}

cat > "$TMP_DIR/w3c/case.json" <<'JSON'
{"schema_version":1,"w3c_lane_status":"baseline","result_status":"passed"}
JSON
cat > "$TMP_DIR/w3c/unsupported.json" <<'JSON'
{"schema_version":1,"w3c_lane_status":"unsupported","result_status":"unsupported"}
JSON
cat > "$TMP_DIR/w3c/run_metadata.json" <<'JSON'
{"source_revision":"w3c-test-revision","flags":{"OWL2_REASONER_FORCE_TEXT":"1"},"corpus":"w3c-approved-test"}
JSON
mkdir -p "$TMP_DIR/w3c/raw"
printf '%s\n' '{"case_id":"raw-case","result_status":"passed"}' > "$TMP_DIR/w3c/raw/raw-case.json"
cat > "$TMP_DIR/ore/ore_consistency_details.tsv" <<'TSV'
local_group	official_group	ore_track	ontology	source_ontology	reasoner	execution_status	result_status	correctness_status	oracle_status	semantic_result	oracle_case_id	consensus_expected	oracle_source
pure_dl	OWL DL	OWL DL Consistency	a.owl	a.owl	tableauxx	success	oracle_unavailable	oracle_unavailable	unavailable	consistent			
TSV
cat > "$TMP_DIR/owl2bench/owl2bench_stage_results.csv" <<'CSV'
run_id,stage,profile,seed,abox_size,university_count,reasoner,ontology,operation,execution_status,result_status,correctness_status,semantic_result,wall_time_ms,reported_time_ms,parse_time_ms,reason_time_ms,source_ontology
run-1,correctness,DL,1,,,tableauxx,0001_UNIV-BENCH-OWL2DL.owl,consistency,success,oracle_unavailable,oracle_unavailable,consistent,10,10,-1,-1,
run-2,scalability,EL,1,,1,tableauxx,0002_UNIV-BENCH-OWL2EL.owl,consistency,success,oracle_unavailable,oracle_unavailable,consistent,20,20,-1,-1,
CSV
cat > "$TMP_DIR/owl2bench/owl2bench_stage_report.md" <<'MD'
# OWL2Bench Stage Report

- Lane kind: performance (ADR 0027).
- Performance-lane contract: this lane adjudicates no correctness and no oracle exists for OWL2Bench consistency; `oracle_unavailable` is the permanent correctness status of every row, and this report claims execution performance and completability only.
MD
cat > "$TMP_DIR/query/summary.json" <<'JSON'
{"correctness_gate":"blocked","result_status_counts":{"oracle_unavailable":1}}
JSON

W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/out" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null

jq -e '.correctness_gate == "blocked" and .performance_claims_allowed == false' "$TMP_DIR/out/summary.json" >/dev/null
jq -e '.suites.w3c.case_count == 2 and .suites.w3c.result_status_counts.unsupported == 1' "$TMP_DIR/out/summary.json" >/dev/null
jq -e '.suites.owl2bench.case_count == 2 and .suites.owl2bench.result_status_counts.oracle_unavailable == 2' "$TMP_DIR/out/summary.json" >/dev/null
grep -F 'performance and scalability claims are suppressed' "$TMP_DIR/out/report.md" >/dev/null
grep -F '| owl2bench-query | blocked | 1 |' "$TMP_DIR/out/report.md" >/dev/null
jq -e '
  .suites.w3c.lane_kind == "correctness"
  and .suites.owl2bench.lane_kind == "performance"
  and .suites["owl2bench-query"].lane_kind == "correctness"
  and .suites.ore.lane_kind == "performance"
' "$TMP_DIR/out/summary.json" >/dev/null
# ADR 0027: the OWL2Bench stage suite is an informational performance lane;
# its execution evidence flows while it adjudicates no correctness.
jq -e '
  .suites.owl2bench.status == "informational"
  and .suites.owl2bench.correctness_gate == "not_applicable"
  and .suites.owl2bench.execution_status_counts.success == 2
  and .suites.owl2bench.timing_evidence.recorded_counts.wall_time_ms == 2
  and .suites.owl2bench.timing_evidence.recorded_counts.parse_time_ms == 0
' "$TMP_DIR/out/summary.json" >/dev/null
jq -e '
  .suites.w3c.performance_claims_allowed == true
  and .suites["owl2bench-query"].performance_claims_allowed == false
' "$TMP_DIR/out/summary.json" >/dev/null

cat > "$TMP_DIR/query/summary.json" <<'JSON'
{"correctness_gate":"passed","result_status_counts":{"passed":1}}
JSON
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/out" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null

jq -e '.correctness_gate == "blocked" and .performance_claims_allowed == false' "$TMP_DIR/out/summary.json" >/dev/null
grep -F 'performance and scalability claims are suppressed' "$TMP_DIR/out/report.md" >/dev/null

profile_summary() {
  local profile="$1"
  local count="$2"
  local valid_hash="sha256:$(printf '%064d' 0)"
  jq -n --arg profile "$profile" --argjson count "$count" --arg hash "$valid_hash" '{
    profile: $profile,
    split_stack_requested: true,
    case_count: $count,
    expected_case_count: $count,
    correctness_gate: "passed",
    result_status_counts: {passed: $count},
    execution_status_counts: {success: $count},
    correctness_status_counts: {passed: $count},
    oracle_status_counts: {available: $count},
    component_status_counts: {available: $count},
    materialization_status_counts: {complete: $count},
    component_identities: [{profile:$profile,status:"available",manifest_identity:$hash}],
    materialization_identities: [{profile:$profile,status:"complete",materialization_identity:$hash}],
    source_hashes: [$hash],
    oracle_hashes: [$hash],
    oracle_provenance_hashes: [$hash],
    oracle_provenance_artifact_hashes: [$hash],
    source_identities: [{source_query:"queries",source_corpus:"owl2bench",source_revision:"revision",source_query_sha256:$hash}],
    dataset_identities: [{dataset_id:"dataset",dataset_sha256:$hash,source:"fixture"}],
    engine_identities: [{engine:"konclude-unified",version:"v0.7.0-1138"}],
    canonicalizer_identities: [{name:"owl2bench-query-canonicalizer",version:"owl2bench-query-canonicalizer/2"}],
    oracle_identities: [{
      class:"official",
      oracle_id:("official:" + $profile),
      provenance:"official-answer-key",
      provenance_sha256:$hash,
      provenance_artifact:"oracle/provenance/official.json",
      provenance_artifact_sha256:$hash,
      artifact_sha256:$hash,
      canonical_result_hash:$hash,
      match_reason:"accepted"
    }],
    timing_provenance:{unit:"ms",load:{count:$count,recorded_count:$count},query:{count:$count,recorded_count:$count}}
  }'
}
DL_PROFILE_SUMMARY="$(profile_summary DL 22)"
EL_PROFILE_SUMMARY="$(profile_summary EL 10)"
QL_PROFILE_SUMMARY="$(profile_summary QL 10)"
RL_PROFILE_SUMMARY="$(profile_summary RL 18)"
jq -n \
  --argjson dl "$DL_PROFILE_SUMMARY" \
  --argjson el "$EL_PROFILE_SUMMARY" \
  --argjson ql "$QL_PROFILE_SUMMARY" \
  --argjson rl "$RL_PROFILE_SUMMARY" \
  '{
    "correctness_gate":"passed",
    "query_gate":"passed",
    "split_stack_requested":true,
    "component_status_counts":{"available":60},
    "materialization_status_counts":{"complete":60},
    "component_identities":[
      {"profile":"DL","status":"available","manifest_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"EL","status":"available","manifest_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"QL","status":"available","manifest_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"RL","status":"available","manifest_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}
    ],
    "materialization_identities":[
      {"profile":"DL","status":"complete","materialization_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"EL","status":"complete","materialization_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"QL","status":"complete","materialization_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"},
      {"profile":"RL","status":"complete","materialization_identity":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}
    ],
    "profile_gates":{"DL":"passed","EL":"passed","QL":"passed","RL":"passed"},
    "query_count":60,
    "profile_case_counts":{"DL":22,"EL":10,"QL":10,"RL":18},
    "result_status_counts":{"passed":60},
    profiles:{DL:$dl,EL:$el,QL:$ql,RL:$rl}
  }' > "$TMP_DIR/query/summary.json"
mkdir -p "$TMP_DIR/query/profiles"
for profile in DL EL QL RL; do
  mkdir -p "$TMP_DIR/query/profiles/$profile"
  jq --arg profile "$profile" \
    '.profiles[$profile] + {run_dir:"fixture",run_metadata:"fixture/run_metadata.json"}' \
    "$TMP_DIR/query/summary.json" > "$TMP_DIR/query/profiles/$profile/summary.json"
  printf '# OWL2Bench Query Report — %s\n' "$profile" > "$TMP_DIR/query/profiles/$profile/report.md"
done
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/complete" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '.correctness_gate == "passed" and .performance_claims_allowed == true' \
  "$TMP_DIR/complete/summary.json" >/dev/null
grep -F 'performance claims may be interpreted' "$TMP_DIR/complete/report.md" >/dev/null
# ADR 0024: ORE is a performance lane. Every row is oracle_unavailable, yet
# the campaign correctness gate passes because the conjunction covers the
# correctness-kind suites only.
jq -e '
  .suites.ore.lane_kind == "performance"
  and .suites.ore.status == "informational"
  and .suites.ore.correctness_gate == "not_applicable"
  and .suites.ore.performance_claims_allowed == true
  and .suites.ore.result_status_counts.oracle_unavailable == 1
  and .suites.w3c.performance_claims_allowed == true
' "$TMP_DIR/complete/summary.json" >/dev/null
# ADR 0027: the OWL2Bench stage suite is likewise a performance lane. Every
# correctness row stays oracle_unavailable, yet the campaign correctness
# gate passes because the conjunction is W3C AND owl2bench-query.
jq -e '
  .suites.owl2bench.lane_kind == "performance"
  and .suites.owl2bench.status == "informational"
  and .suites.owl2bench.correctness_gate == "not_applicable"
  and .suites.owl2bench.performance_claims_allowed == true
  and .suites.owl2bench.result_status_counts.oracle_unavailable == 2
  and .suites.owl2bench.execution_status_counts.success == 2
  and .suites.w3c.performance_claims_allowed == true
' "$TMP_DIR/complete/summary.json" >/dev/null
grep -F '| owl2bench | informational |' "$TMP_DIR/complete/report.md" >/dev/null
grep -F 'Performance-claim permission is scoped per suite' "$TMP_DIR/complete/report.md" >/dev/null
grep -F 'adjudicates no correctness' "$TMP_DIR/complete/report.md" >/dev/null

# A missing ORE run directory is reported informationally and never blocks
# the campaign correctness gate (contrast: a missing correctness suite still
# blocks, as the "missing" and "omitted-query" scenarios below assert).
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/no-ore" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '
  .correctness_gate == "passed"
  and .suites.ore.status == "not_available"
  and .suites.ore.lane_kind == "performance"
  and .suites.ore.correctness_gate == "not_applicable"
  and (.suites.ore.note | contains("informationally"))
  and .suites.ore.performance_claims_allowed == false
  and .performance_claims_allowed == false
' "$TMP_DIR/no-ore/summary.json" >/dev/null
grep -F '| ore | not_available |' "$TMP_DIR/no-ore/report.md" >/dev/null

# A missing OWL2Bench stage run directory is likewise informational and
# never blocks the campaign correctness gate (ADR 0027).
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/no-stage" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '
  .correctness_gate == "passed"
  and .suites.owl2bench.status == "not_available"
  and .suites.owl2bench.lane_kind == "performance"
  and .suites.owl2bench.correctness_gate == "not_applicable"
  and (.suites.owl2bench.note | contains("informationally"))
  and .suites.owl2bench.performance_claims_allowed == false
  and .performance_claims_allowed == false
' "$TMP_DIR/no-stage/summary.json" >/dev/null
grep -F '| owl2bench | not_available |' "$TMP_DIR/no-stage/report.md" >/dev/null

cp "$TMP_DIR/query/summary.json" "$TMP_DIR/query/complete-summary.json"
jq 'del(.split_stack_requested)' "$TMP_DIR/query/summary.json" > "$TMP_DIR/query/non-split.json"
mv "$TMP_DIR/query/non-split.json" "$TMP_DIR/query/summary.json"
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/non-split" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '
  .correctness_gate == "blocked"
  and (.suites["owl2bench-query"].strict_query_gate_errors
       | any(.[]; contains("split-stack declaration is required")))
' "$TMP_DIR/non-split/summary.json" >/dev/null
mv "$TMP_DIR/query/complete-summary.json" "$TMP_DIR/query/summary.json"

cp "$TMP_DIR/query/summary.json" "$TMP_DIR/query/complete-summary.json"
jq '.component_identities = [.component_identities[0]]' \
  "$TMP_DIR/query/summary.json" > "$TMP_DIR/query/incomplete-identities.json"
mv "$TMP_DIR/query/incomplete-identities.json" "$TMP_DIR/query/summary.json"
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/incomplete-identities" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '
  .correctness_gate == "blocked"
  and (.suites["owl2bench-query"].strict_query_gate_errors
       | any(.[]; contains("split-stack component identity profile coverage mismatch")))
' "$TMP_DIR/incomplete-identities/summary.json" >/dev/null
mv "$TMP_DIR/query/complete-summary.json" "$TMP_DIR/query/summary.json"

jq '.profiles.DL.source_hashes[0] = "not-a-hash"' "$TMP_DIR/query/summary.json" > "$TMP_DIR/query/bad-hash.json"
mv "$TMP_DIR/query/bad-hash.json" "$TMP_DIR/query/summary.json"
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/bad-hash" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '.correctness_gate == "blocked" and .performance_claims_allowed == false' \
  "$TMP_DIR/bad-hash/summary.json" >/dev/null

cat > "$TMP_DIR/query/summary.json" <<'JSON'
{
  "correctness_gate": "passed",
  "query_gate": "passed",
  "profile_gates": {"DL":"passed","EL":"blocked","QL":"passed","RL":"passed"},
  "result_status_counts": {"passed":60}
}
JSON
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/profile-blocked" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
OWL2BENCH_QUERY_RUN_DIR="$TMP_DIR/query" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '.correctness_gate == "blocked" and .performance_claims_allowed == false' \
  "$TMP_DIR/profile-blocked/summary.json" >/dev/null
grep -F '| owl2bench-query | blocked |' "$TMP_DIR/profile-blocked/report.md" >/dev/null

CAMPAIGN_OUTPUT_DIR="$TMP_DIR/missing" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '.correctness_gate == "blocked" and .suites.w3c.status == "not_available"' "$TMP_DIR/missing/summary.json" >/dev/null
jq -e '
  .suites.ore.status == "not_available"
  and .suites.ore.lane_kind == "performance"
  and .suites.ore.correctness_gate == "not_applicable"
' "$TMP_DIR/missing/summary.json" >/dev/null
jq -e '
  .suites.owl2bench.status == "not_available"
  and .suites.owl2bench.lane_kind == "performance"
  and .suites.owl2bench.correctness_gate == "not_applicable"
' "$TMP_DIR/missing/summary.json" >/dev/null

jq -e '.suites.w3c.metadata["run_metadata.json"].source_revision == "w3c-test-revision"' "$TMP_DIR/out/summary.json" >/dev/null
jq -e '(.suites.w3c.raw_result_artifacts | any(.[]; contains("raw-case.json")))' "$TMP_DIR/out/summary.json" >/dev/null

# The query lane is required by the campaign contract even when its run
# directory is omitted; absence must be represented as not_available.
CAMPAIGN_OUTPUT_DIR="$TMP_DIR/omitted-query" \
W3C_RUN_DIR="$TMP_DIR/w3c" \
ORE_RUN_DIR="$TMP_DIR/ore" \
OWL2BENCH_RUN_DIR="$TMP_DIR/owl2bench" \
"$ROOT_DIR/benchmarks/campaign/report.sh" >/dev/null
jq -e '
  .correctness_gate == "blocked"
  and .suites["owl2bench-query"].status == "not_available"
  and .suites["owl2bench-query"].correctness_gate == "blocked"
' "$TMP_DIR/omitted-query/summary.json" >/dev/null

echo "campaign report tests passed"
