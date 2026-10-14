#!/usr/bin/env bash
set -euo pipefail

# Contract tests for the SPACL RL engine adapter (#84): load materializes the
# RL profile context with owl2-reasoner and writes a materialization handoff;
# query validates the handoff and delegates to the pinned query backend.
# Everything here runs without docker: the reasoner and the query backend are
# both faked. The docker-gated smoke at the end is skipped unless enabled.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

ADAPTER="$SCRIPT_DIR/spacl_rl_engine.sh"
CAPABILITIES="$SCRIPT_DIR/spacl-rl-capabilities.json"

fail() { echo "[test-spacl-rl][assert] $1" >&2; exit 1; }

[[ -f "$ADAPTER" ]] || fail "adapter script is missing: $ADAPTER"
[[ -f "$CAPABILITIES" ]] || fail "capabilities file is missing: $CAPABILITIES"

mkdir -p "$TMP_DIR/context/RL"
cat > "$TMP_DIR/context/RL/ontology.owl" <<'OWL'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://example.org/m" xmlns="http://example.org/m#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:NamedIndividual rdf:about="#bob"/>
</rdf:RDF>
OWL
printf '<http://example.org/m#bob> <http://example.org/m#knows> <http://example.org/m#alice> .\n' > "$TMP_DIR/expected.nt"

# --- Fake reasoner: emits a stats line and produces the graph ----------------
cat > "$TMP_DIR/fake-reasoner" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "materialize" ]]
input="$2"; output="$3"
grep -q "NamedIndividual" "$input"
cat > "$output" <<'NT'
<http://example.org/m#bob> <http://example.org/m#knows> <http://example.org/m#alice> .
NT
printf '{"command":"materialize","base_triples":1,"derived_triples":0,"closure_triples":1,"out_of_fragment_axioms":0,"closure_sha256":"fake","parse_time_ms":1,"materialize_time_ms":1}\n'
ENGINE
chmod +x "$TMP_DIR/fake-reasoner"

# --- Fake query backend: asserts the handoff contract, answers TSV -----------
cat > "$TMP_DIR/fake-backend" <<'BACKEND'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" ]]
query_file="$2"
handoff="${OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF:?handoff env is not set}"
status="$(jq -r '.status // ""' "$handoff")"
[[ "$status" == "ready" ]] || { echo "handoff not ready" >&2; exit 1; }
graph="$(jq -r '.graph_path // ""' "$handoff")"
[[ -f "$graph" ]] || { echo "graph missing" >&2; exit 1; }
grep -q "knows" "$graph" || { echo "graph lacks expected triple" >&2; exit 1; }
if grep -Eq '\?x[[:space:]]+\?y' "$query_file"; then
  printf '?x\t?y\n"answer-a"\t"answer-b"\n'
else
  printf '?x\n"answer-a"\n'
fi
BACKEND
chmod +x "$TMP_DIR/fake-backend"

export SPACL_RL_REASONER_BIN="$TMP_DIR/fake-reasoner"
export SPACL_RL_QUERY_BACKEND="$TMP_DIR/fake-backend"

# --- load: handoff, provenance, stats capture --------------------------------
"$ADAPTER" load "$TMP_DIR/context/RL" > "$TMP_DIR/load-out.txt" 2> "$TMP_DIR/load-err.txt" \
  || fail "adapter load failed: $(cat "$TMP_DIR/load-err.txt")"
STATE_DIR="$TMP_DIR/context/RL/.spacl-rl"
[[ -f "$STATE_DIR/handoff.json" ]] || fail "handoff missing after load"
jq -e '.status == "ready" and (.graph_path | endswith("materialized.nt")) and (.graph_sha256 | startswith("sha256:"))' \
  "$STATE_DIR/handoff.json" >/dev/null || fail "handoff contract violated"
cmp -s "$STATE_DIR/materialized.nt" "$TMP_DIR/expected.nt" || fail "materialized graph differs from the reasoner output"
jq -e '.command == "materialize" and .closure_triples == 1' "$STATE_DIR/materialize-stats.json" >/dev/null \
  || fail "materialize stats were not captured"
[[ -f "$STATE_DIR/provenance.json" ]] || fail "provenance missing after load"
jq -e '
  .adapter == "spacl-rl" and .profile == "RL"
  and (.reasoner_bin_sha256 | startswith("sha256:"))
  and (.ontology.sha256 | startswith("sha256:"))
  and .oxigraph.image == "oxigraph/oxigraph:0.5.9"
  and (.oxigraph.image_digest | startswith("sha256:"))
  and .materialize.closure_triples == 1
' "$STATE_DIR/provenance.json" >/dev/null || fail "provenance contract violated"

# --- query: delegates with the handoff ----------------------------------------
printf 'SELECT DISTINCT ?x WHERE { ?x :knows ?y }' > "$TMP_DIR/query.rq"
"$ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/context/RL" > "$TMP_DIR/query-out.txt" 2> "$TMP_DIR/query-err.txt" \
  || fail "adapter query failed: $(cat "$TMP_DIR/query-err.txt")"
grep -q 'answer-a' "$TMP_DIR/query-out.txt" || fail "query backend output did not reach stdout"

# --- tampered graph fails closed before the backend ---------------------------
printf 'tampered\n' >> "$STATE_DIR/materialized.nt"
"$ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/context/RL" > /dev/null 2> "$TMP_DIR/tamper-err.txt" \
  && fail "tampered graph must fail the handoff hash check"
grep -q "hash" "$TMP_DIR/tamper-err.txt" || fail "tamper failure did not name the hash mismatch"
# restore for the lane run
"$ADAPTER" load "$TMP_DIR/context/RL" > /dev/null 2>&1 || fail "reload after tamper failed"

# --- non-RL profile contexts are refused ---------------------------------------
mkdir -p "$TMP_DIR/context/EL"
cp "$TMP_DIR/context/RL/ontology.owl" "$TMP_DIR/context/EL/"
"$ADAPTER" load "$TMP_DIR/context/EL" > /dev/null 2>&1 \
  && fail "adapter must refuse a non-RL profile context"
jq -e '.profiles == ["RL"] and .data_mode == "directory"' "$CAPABILITIES" >/dev/null \
  || fail "capabilities must declare RL-only directory mode"

# --- lane-level wiring through run.sh ------------------------------------------
RUN_ROOT="$TMP_DIR/results"
OFFICIAL_STAGED="${OWL2BENCH_SPACTL_TEST_STAGED_DIR:-$SCRIPT_DIR/../../../ontologies/external/owl2bench/query-staged}"
[[ -r "$OFFICIAL_STAGED/manifest.tsv" ]] || { echo "[test-spacl-rl][skip] staged query manifest unavailable"; exit 0; }
RUN_ID="spacl-rl-lane"
OWL2BENCH_QUERY_STAGED_DIR="$OFFICIAL_STAGED" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
RUN_ID="$RUN_ID" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_DATA_DIR="$TMP_DIR/context" \
"$SCRIPT_DIR/run.sh" >/dev/null 2>&1 || fail "lane run with the spacl-rl adapter failed"
rl_record="$(find "$RUN_ROOT/$RUN_ID" -maxdepth 1 -type f -name 'owl2bench_query_002__RL.json' -print -quit)"
[[ -n "$rl_record" ]] || fail "no RL case record produced by the lane run"
jq -e '
  .execution_status == "success"
  and .profile_context.status == "available"
  and .profile_context.directory == $dir
  and (.load_time_ms | type == "number")
  and .result_status == "oracle_unavailable"
' --arg dir "$TMP_DIR/context/RL" "$rl_record" >/dev/null || fail "RL record contract violated: $(jq -c '{execution_status, result_status, profile_context, load_time_ms}' "$rl_record")"
dl_record="$(find "$RUN_ROOT/$RUN_ID" -maxdepth 1 -type f -name 'owl2bench_query_001__DL.json' -print -quit)"
[[ -n "$dl_record" ]] || fail "no DL case record produced"
jq -e '.execution_status == "not_available"' "$dl_record" >/dev/null || fail "non-RL profile must be not_available"

# --- docker-gated smoke with the real pinned backend ---------------------------
if [[ "${SPACL_RL_TEST_DOCKER:-0}" == "1" ]] && command -v docker >/dev/null 2>&1; then
  # The pinned backend injects PREFIX : <https://kracr.iiitd.edu.in/OWL2Bench#>,
  # so the real-query smoke must use that namespace in its graph.
  cat > "$TMP_DIR/fake-reasoner-kracr" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
printf '<https://kracr.iiitd.edu.in/OWL2Bench#bob> <https://kracr.iiitd.edu.in/OWL2Bench#knows> <https://kracr.iiitd.edu.in/OWL2Bench#alice> .\n' > "$3"
printf '{"command":"materialize","base_triples":1,"derived_triples":0,"closure_triples":1,"out_of_fragment_axioms":0,"closure_sha256":"x","parse_time_ms":1,"materialize_time_ms":1}\n'
ENGINE
  chmod +x "$TMP_DIR/fake-reasoner-kracr"
  SPACL_RL_REASONER_BIN="$TMP_DIR/fake-reasoner-kracr" \
    "$ADAPTER" load "$TMP_DIR/context/RL" > /dev/null 2>&1 || fail "kracr reload failed"
  export MATERIALIZED_GRAPH_QUERY_ALLOW_PULL=1
  unset SPACL_RL_QUERY_BACKEND
  "$ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/context/RL" > "$TMP_DIR/docker-out.txt" 2> "$TMP_DIR/docker-err.txt" \
    || fail "docker smoke failed: $(tail -3 "$TMP_DIR/docker-err.txt")"
  grep -q 'kracr.iiitd.edu.in/OWL2Bench#bob' "$TMP_DIR/docker-out.txt" \
    || fail "docker smoke produced no answer rows: $(cat "$TMP_DIR/docker-out.txt")"
else
  echo "[test-spacl-rl][skip] docker smoke (set SPACL_RL_TEST_DOCKER=1 to enable)"
fi

# --- QL binding: the shared core serves QL contexts and refuses RL ones -----
QL_ADAPTER="$SCRIPT_DIR/spacl_ql_engine.sh"
QL_CAPABILITIES="$SCRIPT_DIR/spacl-ql-capabilities.json"
[[ -x "$QL_ADAPTER" && -f "$QL_CAPABILITIES" ]] || fail "QL adapter or capabilities missing"
mkdir -p "$TMP_DIR/qcontext/QL"
cp "$TMP_DIR/context/RL/ontology.owl" "$TMP_DIR/qcontext/QL/"
"$QL_ADAPTER" load "$TMP_DIR/qcontext/QL" > /dev/null 2> "$TMP_DIR/ql-load-err.txt" \
  || fail "QL load failed: $(cat "$TMP_DIR/ql-load-err.txt")"
[[ -f "$TMP_DIR/qcontext/QL/.spacl-ql/handoff.json" ]] || fail "QL state dir missing"
jq -e '.profile == "QL" and .status == "ready"' "$TMP_DIR/qcontext/QL/.spacl-ql/handoff.json" >/dev/null \
  || fail "QL handoff contract violated"
jq -e '.profiles == ["QL"]' "$QL_CAPABILITIES" >/dev/null || fail "QL capabilities must be QL-only"
"$QL_ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/qcontext/QL" > "$TMP_DIR/ql-query-out.txt" 2>/dev/null \
  || fail "QL query failed"
grep -q 'answer-a' "$TMP_DIR/ql-query-out.txt" || fail "QL query produced no answer"
"$QL_ADAPTER" load "$TMP_DIR/context/RL" > /dev/null 2>&1 \
  && fail "QL adapter must refuse an RL profile context"

# --- EL binding: the shared core serves EL contexts and refuses RL ones ----
EL_ADAPTER="$SCRIPT_DIR/spacl_el_engine.sh"
EL_CAPABILITIES="$SCRIPT_DIR/spacl-el-capabilities.json"
[[ -x "$EL_ADAPTER" && -f "$EL_CAPABILITIES" ]] || fail "EL adapter or capabilities missing"
mkdir -p "$TMP_DIR/econtext/EL"
cp "$TMP_DIR/context/RL/ontology.owl" "$TMP_DIR/econtext/EL/"
"$EL_ADAPTER" load "$TMP_DIR/econtext/EL" > /dev/null 2> "$TMP_DIR/el-load-err.txt" \
  || fail "EL load failed: $(cat "$TMP_DIR/el-load-err.txt")"
[[ -f "$TMP_DIR/econtext/EL/.spacl-el/handoff.json" ]] || fail "EL state dir missing"
jq -e '.profile == "EL" and .status == "ready"' "$TMP_DIR/econtext/EL/.spacl-el/handoff.json" >/dev/null \
  || fail "EL handoff contract violated"
jq -e '.profiles == ["EL"]' "$EL_CAPABILITIES" >/dev/null || fail "EL capabilities must be EL-only"
jq -e '.engine == "SPACL-EL" and .profile == "EL"' "$TMP_DIR/econtext/EL/.spacl-el/provenance.json" >/dev/null \
  || fail "EL provenance must carry the EL engine identity"
"$EL_ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/econtext/EL" > "$TMP_DIR/el-query-out.txt" 2>/dev/null \
  || fail "EL query failed"
grep -q 'answer-a' "$TMP_DIR/el-query-out.txt" || fail "EL query produced no answer"
"$EL_ADAPTER" load "$TMP_DIR/context/RL" > /dev/null 2>&1 \
  && fail "EL adapter must refuse an RL profile context"

# --- DL binding: the shared core serves DL contexts and refuses RL ones ----
DL_ADAPTER="$SCRIPT_DIR/spacl_dl_engine.sh"
DL_CAPABILITIES="$SCRIPT_DIR/spacl-dl-capabilities.json"
[[ -x "$DL_ADAPTER" && -f "$DL_CAPABILITIES" ]] || fail "DL adapter or capabilities missing"
mkdir -p "$TMP_DIR/dcontext/DL"
cp "$TMP_DIR/context/RL/ontology.owl" "$TMP_DIR/dcontext/DL/"
"$DL_ADAPTER" load "$TMP_DIR/dcontext/DL" > /dev/null 2> "$TMP_DIR/dl-load-err.txt" \
  || fail "DL load failed: $(cat "$TMP_DIR/dl-load-err.txt")"
[[ -f "$TMP_DIR/dcontext/DL/.spacl-dl/handoff.json" ]] || fail "DL state dir missing"
jq -e '.profile == "DL" and .status == "ready"' "$TMP_DIR/dcontext/DL/.spacl-dl/handoff.json" >/dev/null \
  || fail "DL handoff contract violated"
jq -e '.profiles == ["DL"]' "$DL_CAPABILITIES" >/dev/null || fail "DL capabilities must be DL-only"
jq -e '.engine == "SPACL-DL" and .profile == "DL"' "$TMP_DIR/dcontext/DL/.spacl-dl/provenance.json" >/dev/null \
  || fail "DL provenance must carry the DL engine identity"
"$DL_ADAPTER" query "$TMP_DIR/query.rq" "$TMP_DIR/dcontext/DL" > "$TMP_DIR/dl-query-out.txt" 2>/dev/null \
  || fail "DL query failed"
grep -q 'answer-a' "$TMP_DIR/dl-query-out.txt" || fail "DL query produced no answer"
"$DL_ADAPTER" load "$TMP_DIR/context/RL" > /dev/null 2>&1 \
  && fail "DL adapter must refuse an RL profile context"

echo "[test-spacl-rl] ok"
