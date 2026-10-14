#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_ROOT="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

fail() {
  printf '[test-owl2bench][fail] %s\n' "$*" >&2
  exit 1
}

assert_file() {
  [[ -f "$1" ]] || fail "missing file: $1"
}

assert_eq() {
  local expected="$1"
  local actual="$2"
  local msg="$3"
  [[ "$expected" == "$actual" ]] || fail "$msg: expected '$expected', got '$actual'"
}

write_owl() {
  local path="$1"
  mkdir -p "$(dirname "$path")"
  printf '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"></rdf:RDF>\n' > "$path"
}

SOURCE_DIR="$TMP_ROOT/source"
STAGED_DIR="$TMP_ROOT/staged"

write_owl "$SOURCE_DIR/correctness/UNIV-BENCH-OWL2DL-seed=11.owl"
write_owl "$SOURCE_DIR/correctness/UNIV-BENCH-OWL2EL-seed=11.owl"
write_owl "$SOURCE_DIR/correctness/UNIV-BENCH-OWL2QL-seed=11.owl"
write_owl "$SOURCE_DIR/correctness/UNIV-BENCH-OWL2RL-seed=11.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2DL-seed=9-universities=1.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2DL-seed=9-universities=2.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2DL-seed=9-universities=3.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2EL-seed=7-abox=100.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2EL-seed=7-abox=200.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2QL-seed=7-abox=100.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2RL-seed=7-abox=100.owl"
write_owl "$SOURCE_DIR/scalability/UNIV-BENCH-OWL2RL-seed=7-abox=200.owl"

OWL2BENCH_SOURCE_DIR="$SOURCE_DIR" \
OWL2BENCH_STAGED_DIR="$STAGED_DIR" \
OWL2BENCH_LINK_MODE=copy \
OWL2BENCH_SMOKE=1 \
"$SCRIPT_DIR/prepare.sh" >/dev/null

MANIFEST="$STAGED_DIR/manifest.tsv"
assert_file "$MANIFEST"

header="$(sed -n '1p' "$MANIFEST")"
assert_eq $'staged_ontology\tsource_ontology\tprofile\tseed\tabox_size\tuniversity_count\tstage' "$header" "manifest header preserves metadata fields"

for profile in DL EL QL RL; do
  count="$(awk -F '\t' -v p="$profile" '$3 == p && $7 == "correctness" { c++ } END { print c + 0 }' "$MANIFEST")"
  assert_eq "1" "$count" "smoke keeps one correctness point for $profile"
done

dl_sizes="$(awk -F '\t' '$3 == "DL" && $7 == "scalability" { print $6 }' "$MANIFEST" | sort | tr '\n' ',' | sed 's/,$//')"
assert_eq "1,2" "$dl_sizes" "smoke keeps first two university counts"

el_sizes="$(awk -F '\t' '$3 == "EL" && $7 == "scalability" { print $5 }' "$MANIFEST" | sort | tr '\n' ',' | sed 's/,$//')"
assert_eq "100,200" "$el_sizes" "smoke keeps first two ABox sizes"

RUN_DIR="$TMP_ROOT/run"
mkdir -p "$RUN_DIR"
cp "$MANIFEST" "$RUN_DIR/owl2bench_manifest.tsv"

first_dl="$(awk -F '\t' '$3 == "DL" && $7 == "correctness" { n=$1; sub(/^.*\//, "", n); print n; exit }' "$MANIFEST")"
first_el_scalability="$(awk -F '\t' '$3 == "EL" && $7 == "scalability" { n=$1; sub(/^.*\//, "", n); print n; exit }' "$MANIFEST")"

{
  printf 'run_id,ontology_suite,case_id,reasoner,ontology,operation,status,execution_status,execution_detail,result_status,correctness_status,oracle_status,oracle_source,oracle_case_id,oracle_expected,semantic_result,reasoning_result,wall_time_ms,reported_time_ms,parse_time_ms,reason_time_ms,timestamp\n'
  printf '"test","standard","standard:%s:consistency","tableauxx","%s","consistency","failed","failed","failed","not_evaluated","not_evaluated","not_configured","","","","unknown","unknown",5,5,-1,-1,"2026-01-01T00:00:00Z"\n' "$first_dl" "$first_dl"
  printf '"test","standard","standard:%s:consistency","tableauxx","%s","consistency","success","success","success","not_evaluated","not_evaluated","not_configured","","","","consistent","consistent",10,10,-1,-1,"2026-01-01T00:00:00Z"\n' "$first_el_scalability" "$first_el_scalability"
} > "$RUN_DIR/results.csv"

"$SCRIPT_DIR/stage_report.sh" "$RUN_DIR" "$RUN_DIR/owl2bench_manifest.tsv" >/dev/null

assert_file "$RUN_DIR/owl2bench_stage_results.csv"
assert_file "$RUN_DIR/owl2bench_stage_report.md"

grep -q 'Lane kind: performance (ADR 0027)' "$RUN_DIR/owl2bench_stage_report.md" || fail "stage report declares its performance lane kind"
grep -q 'this lane adjudicates no correctness' "$RUN_DIR/owl2bench_stage_report.md" || fail "stage report declares the performance-lane contract"
if grep -q 'Correctness gate:' "$RUN_DIR/owl2bench_stage_report.md"; then
  fail "stage report must not carry a correctness gate line"
fi
grep -q '| correctness | DL | 1 | 0 | 1 | 0 | 0 | 0 | 0 | N/A |' "$RUN_DIR/owl2bench_stage_report.md" || fail "failed correctness-stage execution stays visible as execution evidence"
grep -q '| scalability | EL | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 10 |' "$RUN_DIR/owl2bench_stage_report.md" || fail "scalability average follows completed executions, not a correctness gate"
grep -q 'issue #7 owns query execution and query oracles' "$RUN_DIR/owl2bench_stage_report.md" || fail "query lane remains explicit out of scope"

printf '[test-owl2bench] ok\n'
