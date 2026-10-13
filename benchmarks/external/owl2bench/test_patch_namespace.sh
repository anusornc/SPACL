#!/usr/bin/env bash
# Contract tests for the namespace patcher (#95): deterministic ABox re-base
# onto the TBox vocabulary, refusals, and byte-determinism across runs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PATCHER="$SCRIPT_DIR/patch_namespace.py"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fail() {
  echo "[test-patch-namespace][fail] $1" >&2
  exit 1
}

KRA="https://kracr.iiitd.edu.in/OWL2Bench#"
BENCH="http://benchmark/OWL2Bench#"

cat >"$WORK/mini.owl" <<XML
<?xml version="1.0"?>
<rdf:RDF xml:base="http://benchmark/OWL2Bench" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#" xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="${BENCH}Student">
    <rdfs:subClassOf rdf:resource="${BENCH}Person"/>
  </owl:Class>
  <owl:NamedIndividual rdf:about="${KRA}U0C0D0S0">
    <rdf:type rdf:resource="${KRA}Student"/>
    <hasFriend rdf:resource="${KRA}U0C0D0S1"/>
  </owl:NamedIndividual>
</rdf:RDF>
XML

# --- happy path: re-base, refuse leftovers --------------------------------
python3 "$PATCHER" "$WORK/mini.owl" "$WORK/patched.owl" >"$WORK/result.json" \
  || fail "patcher failed: $(cat "$WORK/result.json")"
jq -e '.status == "patched" and .replaced_occurrences > 0 and (.output_sha256 | startswith("sha256:"))' \
  "$WORK/result.json" >/dev/null || fail "patcher result contract violated"
grep -q "$KRA" "$WORK/patched.owl" && fail "generator namespace survived the patch"
grep -q "rdf:about=\"${BENCH}U0C0D0S0\"" "$WORK/patched.owl" || fail "ABox individual not re-based"
grep -q "rdf:resource=\"${BENCH}Student\"" "$WORK/patched.owl" || fail "ABox class not re-based"
grep -q "rdf:resource=\"${BENCH}Person\"" "$WORK/patched.owl" || fail "TBox axiom changed"
grep -q "<rdfs:subClassOf rdf:resource=\"${BENCH}Person\"/>" "$WORK/patched.owl" \
  || fail "TBox bytes must be untouched by the re-base"

# --- determinism: two runs, identical bytes -------------------------------
python3 "$PATCHER" "$WORK/mini.owl" "$WORK/patched-2.owl" >/dev/null || fail "second patch run failed"
cmp -s "$WORK/patched.owl" "$WORK/patched-2.owl" || fail "patcher is not byte-deterministic"

# --- refusals: no-op input, leftover after patch --------------------------
cat >"$WORK/noop.owl" <<XML
<?xml version="1.0"?>
<rdf:RDF xml:base="http://benchmark/OWL2Bench" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="$BENCH Person"/>
</rdf:RDF>
XML
python3 "$PATCHER" "$WORK/noop.owl" "$WORK/noop-out.owl" >/dev/null 2>&1 \
  && fail "patcher must refuse an input with no generator-namespace occurrences"

echo "[test-patch-namespace] ok"
