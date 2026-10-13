#!/usr/bin/env bash
# Contract tests for the external-key cross-check recorder (#100): agreement,
# disagreement, and coverage verdicts against a mini key fixture; the key is
# read-only evidence and never influences the oracle.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fail() {
  echo "[test-crosscheck][fail] $1" >&2
  exit 1
}

# --- a mini patched oracle: three cases with known answers -----------------
ORACLE="$WORK/oracle"
mkdir -p "$ORACLE/answers" "$WORK/key/answer_key/QL-1-s1"

cat >"$ORACLE/derived-oracle-manifest.json" <<JSON
{
  "schema_version": 1,
  "oracles": [
    {"case_id": "owl2bench_query_008:QL", "query_id": "owl2bench_query_008", "profile": "QL", "acceptance_status": "accepted", "answer_file": "answers/owl2bench_query_008:QL.srx"},
    {"case_id": "owl2bench_query_010:QL", "query_id": "owl2bench_query_010", "profile": "QL", "acceptance_status": "accepted", "answer_file": "answers/owl2bench_query_010:QL.srx"},
    {"case_id": "owl2bench_query_020:QL", "query_id": "owl2bench_query_020", "profile": "QL", "acceptance_status": "accepted", "answer_file": "answers/owl2bench_query_020:QL.srx"}
  ]
}
JSON

write_srx() {
  local path="$1" var="$2"; shift 2
  {
    echo '<?xml version="1.0"?>'
    echo '<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head>'
    echo "  <variable name=\"$var\"/>"
    echo '</head><results>'
    for row in "$@"; do
      echo "  <result><binding name=\"$var\"><uri>http://benchmark/OWL2Bench#$row</uri></binding></result>"
    done
    echo '</results></sparql>'
  } >"$path"
}

write_srx "$ORACLE/answers/owl2bench_query_008:QL.srx" x alice bob carol
write_srx "$ORACLE/answers/owl2bench_query_010:QL.srx" x alice bob
write_srx "$ORACLE/answers/owl2bench_query_020:QL.srx" x alice

# --- the mini key: 008 agrees, 010 disagrees, 020 absent --------------------
printf '# SELECT DISTINCT ?x WHERE { ?x :knows ?y }\n# source: agree(3); matching reasoners: hermit,openllet,pellet\nx\nalice\nbob\ncarol\n' >"$WORK/key/answer_key/QL-1-s1/Q08.tsv"
printf '# SELECT DISTINCT ?x WHERE { ?x :hasCollaborationWith ?y }\n# source: agree(3); matching reasoners: hermit,openllet,pellet\nx\nalice\nbob\ncarol\ndave\n' >"$WORK/key/answer_key/QL-1-s1/Q10.tsv"

RESULT="$(python3 "$HERE/crosscheck_external_key.py" --oracle-dir "$ORACLE" --key-dir "$WORK/key")"
echo "$RESULT" | jq -e '.status == "crosschecked" and .cases == 3 and .agree == 1 and .disagree == 1 and .not_covered == 1' >/dev/null \
  || fail "verdict counts wrong: $RESULT"

jq -e '.results[] | select(.query == "Q08") | .verdict == "agree" and .oracle_rows == 3 and .key_rows == 3' "$ORACLE/crosscheck.json" >/dev/null \
  || fail "Q08 must agree"
jq -e '.results[] | select(.query == "Q10") | .verdict == "disagree" and .oracle_rows == 2 and .key_rows == 4' "$ORACLE/crosscheck.json" >/dev/null \
  || fail "Q10 must disagree with both row counts recorded"
jq -e '.results[] | select(.query == "Q20") | .verdict == "not_covered"' "$ORACLE/crosscheck.json" >/dev/null \
  || fail "Q20 must be not_covered"

# The cross-check must not touch the manifest's acceptance state.
jq -e '[.oracles[] | .acceptance_status == "accepted"] | all' "$ORACLE/derived-oracle-manifest.json" >/dev/null \
  || fail "cross-check must not modify oracle acceptance"

echo "[test-crosscheck] ok"
