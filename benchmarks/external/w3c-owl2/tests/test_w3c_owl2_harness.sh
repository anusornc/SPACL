#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
LANE_DIR="$ROOT_DIR/benchmarks/external/w3c-owl2"
TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [[ "$expected" != "$actual" ]]; then
    printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
    exit 1
  fi
}

mkdir -p "$TEST_DIR/source"

mkdir -p "$TEST_DIR/unsafe-staged"
printf '%s\n' 'do not delete' > "$TEST_DIR/unsafe-staged/user-file.txt"
if W3C_OWL2_SOURCE_DIR="$TEST_DIR/source" \
  W3C_OWL2_STAGED_DIR="$TEST_DIR/unsafe-staged" \
  "$LANE_DIR/prepare.sh" >/dev/null 2>&1; then
  echo "assertion failed: unstamped staged directory should be refused" >&2
  exit 1
fi
test -f "$TEST_DIR/unsafe-staged/user-file.txt"

cat > "$TEST_DIR/source/consistency.ofn" <<'CASE'
Ontology(<urn:test:consistency>
  ClassAssertion(:ConsistencyTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:semantics :test :Direct)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  ObjectPropertyAssertion(:species :test :DL)
  DataPropertyAssertion(:identifier :test "consistency")
  DataPropertyAssertion(:rdfXMLPremiseOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/inconsistency.ofn" <<'CASE'
Ontology(<urn:test:inconsistency>
  ClassAssertion(:InconsistencyTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:semantics :test :Direct)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  ObjectPropertyAssertion(:species :test :DL)
  DataPropertyAssertion(:identifier :test "inconsistency")
  DataPropertyAssertion(:rdfXMLPremiseOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/positive.ofn" <<'CASE'
Ontology(<urn:test:positive>
  ClassAssertion(:PositiveEntailmentTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:semantics :test :Direct)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  ObjectPropertyAssertion(:species :test :DL)
  DataPropertyAssertion(:identifier :test "positive")
  DataPropertyAssertion(:rdfXMLPremiseOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
  DataPropertyAssertion(:rdfXMLConclusionOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/negative.ofn" <<'CASE'
Ontology(<urn:test:negative>
  ClassAssertion(:NegativeEntailmentTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:semantics :test :Direct)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  ObjectPropertyAssertion(:species :test :DL)
  DataPropertyAssertion(:identifier :test "negative")
  DataPropertyAssertion(:rdfXMLPremiseOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
  DataPropertyAssertion(:rdfXMLNonConclusionOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/rdf-based.ofn" <<'CASE'
Ontology(<urn:test:rdf>
  ClassAssertion(:PositiveEntailmentTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:semantics :test :RDFBased)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  DataPropertyAssertion(:identifier :test "rdf-based")
  DataPropertyAssertion(:rdfXMLPremiseOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
  DataPropertyAssertion(:rdfXMLConclusionOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/profile.ofn" <<'CASE'
Ontology(<urn:test:profile>
  ClassAssertion(:ProfileIdentificationTest :test)
  ObjectPropertyAssertion(:status :test :Approved)
  ObjectPropertyAssertion(:normativeSyntax :test :RDFXML)
  DataPropertyAssertion(:identifier :test "profile")
  DataPropertyAssertion(:expectedResult :test "Error")
  DataPropertyAssertion(:rdfXMLInputOntology :test "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>")
)
CASE

cat > "$TEST_DIR/source/aggregate.rdf" <<'CASE'
<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF
  xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
  xmlns:owl="http://www.w3.org/2002/07/owl#"
  xmlns:test="http://www.w3.org/2007/OWL/testOntology#">
  <test:TestCase rdf:about="urn:test:aggregate-one">
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#ConsistencyTest"/>
    <test:identifier>aggregate-one</test:identifier>
    <test:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <test:semantics rdf:resource="http://www.w3.org/2007/OWL/testOntology#DIRECT"/>
    <test:normativeSyntax rdf:resource="http://www.w3.org/2007/OWL/testOntology#RDFXML"/>
    <test:species rdf:resource="http://www.w3.org/2007/OWL/testOntology#DL"/>
    <test:rdfXmlPremiseOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlPremiseOntology>
  </test:TestCase>
  <test:TestCase rdf:about="urn:test:aggregate-two">
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#InconsistencyTest"/>
    <test:identifier>aggregate-two</test:identifier>
    <test:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <test:semantics rdf:resource="http://www.w3.org/2007/OWL/testOntology#DIRECT"/>
    <test:normativeSyntax rdf:resource="http://www.w3.org/2007/OWL/testOntology#RDFXML"/>
    <test:species rdf:resource="http://www.w3.org/2007/OWL/testOntology#DL"/>
    <test:rdfXmlPremiseOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlPremiseOntology>
  </test:TestCase>
  <test:TestCase rdf:about="urn:test:aggregate-three">
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#ConsistencyTest"/>
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#PositiveEntailmentTest"/>
    <test:identifier>aggregate-three</test:identifier>
    <test:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <test:semantics rdf:resource="http://www.w3.org/2007/OWL/testOntology#DIRECT"/>
    <test:normativeSyntax rdf:resource="http://www.w3.org/2007/OWL/testOntology#RDFXML"/>
    <test:species rdf:resource="http://www.w3.org/2007/OWL/testOntology#DL"/>
    <test:rdfXmlPremiseOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlPremiseOntology>
    <test:rdfXmlConclusionOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlConclusionOntology>
  </test:TestCase>
  <test:TestCase rdf:about="urn:test:aggregate-imports">
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#PositiveEntailmentTest"/>
    <test:identifier>aggregate-imports</test:identifier>
    <test:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <test:semantics rdf:resource="http://www.w3.org/2007/OWL/testOntology#DIRECT"/>
    <test:normativeSyntax rdf:resource="http://www.w3.org/2007/OWL/testOntology#RDFXML"/>
    <test:species rdf:resource="http://www.w3.org/2007/OWL/testOntology#DL"/>
    <test:rdfXmlPremiseOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:owl="http://www.w3.org/2002/07/owl#" xml:base="http://example.org/premise"><owl:Ontology rdf:about=""><owl:imports rdf:resource="http://example.org/imported"/></owl:Ontology></rdf:RDF>]]></test:rdfXmlPremiseOntology>
    <test:rdfXmlConclusionOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlConclusionOntology>
    <test:importedOntology rdf:resource="urn:test:import-node"/>
  </test:TestCase>
  <test:TestCase rdf:about="urn:test:aggregate-unresolved-import">
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#PositiveEntailmentTest"/>
    <test:identifier>aggregate-unresolved-import</test:identifier>
    <test:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <test:semantics rdf:resource="http://www.w3.org/2007/OWL/testOntology#DIRECT"/>
    <test:normativeSyntax rdf:resource="http://www.w3.org/2007/OWL/testOntology#RDFXML"/>
    <test:species rdf:resource="http://www.w3.org/2007/OWL/testOntology#DL"/>
    <test:rdfXmlPremiseOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlPremiseOntology>
    <test:rdfXmlConclusionOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>]]></test:rdfXmlConclusionOntology>
    <test:importedOntology rdf:resource="urn:test:missing-import-node"/>
  </test:TestCase>
  <owl:Thing rdf:about="urn:test:import-node">
    <test:importedOntologyIRI rdf:resource="http://example.org/imported"/>
    <test:rdfXmlInputOntology><![CDATA[<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#" xmlns:owl="http://www.w3.org/2002/07/owl#" xml:base="http://example.org/imported"><owl:Ontology rdf:about=""/><owl:Class rdf:ID="Man"><rdfs:subClassOf rdf:resource="#Mortal"/></owl:Class><owl:Class rdf:ID="Mortal"/></rdf:RDF>]]></test:rdfXmlInputOntology>
  </owl:Thing>
</rdf:RDF>
CASE

cat > "$TEST_DIR/fake-reasoner" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
operation="${2:-}"
case "$operation" in
  consistency) semantic="consistent" ;;
  inconsistency) semantic="true" ;;
  entailment)
    case "${4:-}" in
      *non_conclusion*) semantic="false" ;;
      *) semantic="true" ;;
    esac
    ;;
  *) semantic="unknown" ;;
esac
printf '[phase] parse_start file=fake.owl\n'
jq -n \
  --arg operation "$operation" \
  --arg semantic "$semantic" \
  '{
    execution_status: "success",
    status: "not_evaluated",
    operation: $operation,
    semantic_result: $semantic,
    parse_time_ms: 1,
    reason_time_ms: 2,
    duration_ms: 3,
    runtime_metadata: {}
  }'
FAKE
chmod +x "$TEST_DIR/fake-reasoner"

W3C_OWL2_SOURCE_DIR="$TEST_DIR/source" \
W3C_OWL2_STAGED_DIR="$TEST_DIR/staged" \
"$LANE_DIR/prepare.sh"

manifest="$TEST_DIR/staged/manifest.tsv"
assert_eq "12" "$(tail -n +2 "$manifest" | wc -l | tr -d '[:space:]')" "manifest semantic operation count"
assert_eq "10" "$(awk -F '\t' 'NR > 1 && $6 == "baseline" { count++ } END { print count + 0 }' "$manifest")" "baseline count"
assert_eq "2" "$(awk -F '\t' 'NR > 1 && $6 == "unsupported" { count++ } END { print count + 0 }' "$manifest")" "unsupported count"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $7 == "not_direct_semantics" { count++ } END { print count + 0 }' "$manifest")" "rdf-based separation"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $7 == "syntax_or_profile_test" { count++ } END { print count + 0 }' "$manifest")" "profile separation"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $1 ~ /profile$/ && $5 == "error" { count++ } END { print count + 0 }' "$manifest")" "declared error result preservation"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $1 ~ /aggregate-one$/ && $3 == "consistency" && $6 == "baseline" { count++ } END { print count + 0 }' "$manifest")" "first aggregate case remains isolated"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $1 ~ /aggregate-two$/ && $3 == "inconsistency" && $6 == "baseline" { count++ } END { print count + 0 }' "$manifest")" "second aggregate case remains isolated"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $1 ~ /aggregate-three/ && $3 == "consistency" && $6 == "baseline" { count++ } END { print count + 0 }' "$manifest")" "multi-type consistency operation"
assert_eq "1" "$(awk -F '\t' 'NR > 1 && $1 ~ /aggregate-three/ && $3 == "entailment" && $6 == "baseline" { count++ } END { print count + 0 }' "$manifest")" "multi-type entailment operation"

imported_premise="$(ls "$TEST_DIR"/staged/cases/*_aggregate-imports/premise.owl)"
assert_eq "1" "$(grep -c 'owl:imports' "$imported_premise")" "original import declaration preserved"
assert_eq "2" "$(grep -c 'rdf:ID="Man\|rdf:ID="Mortal' "$imported_premise")" "imported class definitions inlined"
assert_eq "3" "$(grep -c 'xml:base="http://example.org/imported"' "$imported_premise")" "imported elements carry their own xml:base"
assert_eq "1" "$(grep -c 'xml:base="http://example.org/premise"' "$imported_premise")" "premise keeps its own base"
assert_eq "0" "$(grep -c 'Man' "$TEST_DIR"/staged/cases/*_aggregate-imports/conclusion.owl)" "conclusion untouched by inlining"
unresolved_premise="$(ls "$TEST_DIR"/staged/cases/*_aggregate-unresolved-import/premise.owl)"
assert_eq "0" "$(grep -c 'Man' "$unresolved_premise")" "unresolved import reference stages without inlining"
W3C_OWL2_SOURCE_DIR="$TEST_DIR/source" \
W3C_OWL2_STAGED_DIR="$TEST_DIR/staged" \
  "$LANE_DIR/prepare.sh" >/dev/null 2>"$TEST_DIR/prepare-stderr.log"
assert_eq "1" "$(grep -c 'missing-import-node' "$TEST_DIR/prepare-stderr.log")" "unresolved import reference warns on stderr"

RUN_ID=w3c_owl2_test \
W3C_OWL2_MANIFEST="$manifest" \
W3C_OWL2_RESULTS_ROOT="$TEST_DIR/results" \
W3C_OWL2_REASONER_BIN="$TEST_DIR/fake-reasoner" \
SKIP_BUILD=1 \
"$LANE_DIR/run.sh"

run_dir="$TEST_DIR/results/w3c_owl2_test"
assert_eq "10" "$(jq -r '.result_status_counts.passed' "$run_dir/summary.json")" "passed baseline count"
assert_eq "2" "$(jq -r '.result_status_counts.unsupported' "$run_dir/summary.json")" "unsupported count"
assert_eq "6" "$(jq -r '.operation_counts.entailment' "$run_dir/summary.json")" "all entailment operation count"
assert_eq "5" "$(jq -r '.w3c_test_type_counts.PositiveEntailmentTest' "$run_dir/summary.json")" "all positive entailment type count"
assert_eq "4" "$(jq -r '.baseline_w3c_test_type_counts.PositiveEntailmentTest' "$run_dir/summary.json")" "baseline positive entailment type count"
assert_eq "1" "$(jq -r '.baseline_w3c_test_type_counts.NegativeEntailmentTest' "$run_dir/summary.json")" "baseline negative entailment type count"
assert_eq "10" "$(jq -r '.w3c_expected_result_counts.true' "$run_dir/summary.json")" "w3c true count"
assert_eq "1" "$(jq -r '.w3c_expected_result_counts.false' "$run_dir/summary.json")" "w3c false count"

W3C_OWL2_RESULTS_ROOT="$TEST_DIR/results" "$LANE_DIR/report.sh" w3c_owl2_test >/dev/null
test -f "$run_dir/results.csv"
test -f "$run_dir/report.md"
W3C_OWL2_RESULTS_ROOT="$TEST_DIR/results" "$LANE_DIR/report.sh" w3c_owl2_test >/dev/null
assert_eq "12" "$(jq -r '.total_results' "$run_dir/summary.json")" "report remains idempotent"

echo "w3c owl2 harness tests passed"
