#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MATERIALIZER="$SCRIPT_DIR/hermit_dl_materializer.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

ONTOLOGY="$TMP_DIR/anchor.owl"
QUERY_DIR="$TMP_DIR/queries"
CONFIG="$TMP_DIR/hermit-dl.properties"
QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv"
GENERATOR_ARTIFACT="$TMP_DIR/OWL2Bench.jar"
QUERY_BACKEND="$TMP_DIR/query-backend"
QUERY_BACKEND_MANIFEST="$TMP_DIR/query-backend.json"
PROFILE_CONTEXT="$TMP_DIR/DL"
PROFILE_TBOX="$TMP_DIR/profile-tbox.owl"
PROFILE_ABOX="$TMP_DIR/profile-abox.owl"
GENERATOR_CONFIG="$TMP_DIR/generator-config.properties"
MATERIALIZATION_RUN="$TMP_DIR/materialization-run"
MATERIALIZATION_RUN_2="$TMP_DIR/materialization-run-2"
MISSING_OUTPUT="$TMP_DIR/missing-output.nq"
MISSING_STDERR="$TMP_DIR/missing-stderr"
WITNESS_QUERY_DIR="$TMP_DIR/witness-queries"
WITNESS_SOURCE_GRAPH="$TMP_DIR/witness-source.nq"
WITNESS_CANDIDATE_GRAPH="$TMP_DIR/witness-candidate.nq"
PREFIX_QUERY_DIR="$TMP_DIR/prefix-queries"
PREFIX_CONFIG="$TMP_DIR/prefix-hermit-dl.properties"
PREFIX_CANDIDATE_GRAPH="$TMP_DIR/prefix-candidate.nq"
DATA_WITNESS_QUERY_DIR="$TMP_DIR/data-witness-queries"
DATA_WITNESS_SOURCE_GRAPH="$TMP_DIR/data-witness-source.nq"
DATA_WITNESS_CANDIDATE_GRAPH="$TMP_DIR/data-witness-candidate.nq"
CLASS_WITNESS_QUERY_DIR="$TMP_DIR/class-witness-queries"
CLASS_WITNESS_SOURCE_GRAPH="$TMP_DIR/class-witness-source.nq"
CLASS_WITNESS_CANDIDATE_GRAPH="$TMP_DIR/class-witness-candidate.nq"
TBOX_QUERY_DIR="$TMP_DIR/tbox-queries"
TBOX_CANDIDATE_GRAPH="$TMP_DIR/tbox-candidate.nq"
mkdir -p "$QUERY_DIR" "$PROFILE_CONTEXT"

cat > "$ONTOLOGY" <<'OWL'
<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:owl="http://www.w3.org/2002/07/owl#"
         xmlns:xsd="http://www.w3.org/2001/XMLSchema#"
         xmlns:ex="http://example.test/">
  <owl:Ontology rdf:about="http://example.test/ontology"/>
  <owl:Class rdf:about="http://example.test/A"/>
  <owl:Class rdf:about="http://example.test/B">
    <rdfs:subClassOf rdf:resource="http://example.test/C"/>
  </owl:Class>
  <owl:ObjectProperty rdf:about="http://example.test/p"/>
  <owl:ObjectProperty rdf:about="http://example.test/q"/>
  <owl:ObjectProperty rdf:about="http://example.test/witness"/>
  <owl:DatatypeProperty rdf:about="http://example.test/age">
    <rdfs:range rdf:resource="http://www.w3.org/2001/XMLSchema#integer"/>
  </owl:DatatypeProperty>
  <owl:DatatypeProperty rdf:about="http://example.test/parentAge">
    <rdfs:subPropertyOf rdf:resource="http://example.test/age"/>
  </owl:DatatypeProperty>
  <owl:DatatypeProperty rdf:about="http://example.test/score"/>
  <owl:Class rdf:about="http://example.test/A">
    <rdfs:subClassOf rdf:resource="http://example.test/B"/>
    <rdfs:subClassOf>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.test/witness"/>
        <owl:someValuesFrom rdf:resource="http://example.test/D"/>
      </owl:Restriction>
    </rdfs:subClassOf>
    <rdfs:subClassOf>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.test/score"/>
        <owl:someValuesFrom rdf:resource="http://www.w3.org/2001/XMLSchema#integer"/>
      </owl:Restriction>
    </rdfs:subClassOf>
  </owl:Class>
  <owl:Class rdf:about="http://example.test/C"/>
  <owl:Class rdf:about="http://example.test/D"/>
  <rdf:Description rdf:about="http://example.test/p">
    <rdfs:subPropertyOf rdf:resource="http://example.test/q"/>
  </rdf:Description>
  <owl:NamedIndividual rdf:about="http://example.test/alice">
    <rdf:type rdf:resource="http://example.test/A"/>
    <ex:p rdf:resource="http://example.test/bob"/>
    <ex:p rdf:nodeID="anonymous"/>
    <ex:age rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">42</ex:age>
  </owl:NamedIndividual>
  <owl:NamedIndividual rdf:about="http://example.test/bob"/>
  <rdf:Description rdf:nodeID="anonymous">
    <rdf:type rdf:resource="http://example.test/A"/>
  </rdf:Description>
  <rdf:Description rdf:nodeID="unrelated">
    <rdf:type rdf:resource="http://example.test/C"/>
  </rdf:Description>
</rdf:RDF>
OWL
printf '%s\n' 'profile TBox fixture' > "$PROFILE_TBOX"
printf '%s\n' 'profile ABox fixture' > "$PROFILE_ABOX"
printf '%s\n' 'generator configuration fixture' > "$GENERATOR_CONFIG"
printf '%s\n' \
  'SELECT DISTINCT ?x WHERE { ?x rdf:type :B }' \
  'SELECT DISTINCT ?x ?y WHERE { ?x :p ?y }' \
  'SELECT DISTINCT ?x ?y WHERE { ?x :q ?y }' \
  'SELECT DISTINCT ?x ?y WHERE { ?x :age ?y }' \
  'SELECT DISTINCT ?x ?y WHERE { ?x :parentAge ?y }' > "$QUERY_DIR/001.rq"
cat > "$CONFIG" <<'PROPERTIES'
engine.id=HermiT
hermit.version=1.3.8.1
materialization.mode=finite_query_complete
query.namespace=http://example.test/
PROPERTIES
printf '%s\n' \
  $'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision' \
  "$QUERY_DIR/001.rq"$'\tofficial-query\tdl_query_001\tDL\ttest\ttest-corpus\ttest-revision' \
  > "$QUERY_MANIFEST"
printf '%s\n' 'test OWL2Bench generator artifact' > "$GENERATOR_ARTIFACT"
cat > "$QUERY_BACKEND" <<'BACKEND'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" -eq 4 && "$1" == "query" ]]
graph_hash="sha256:$(sha256sum "$3" | awk '{print $1}')"
jq -e --arg hash "$graph_hash" \
  '.status == "ready" and .read_only == true and .graph_sha256 == $hash' "$4" >/dev/null
BACKEND
chmod +x "$QUERY_BACKEND"
BACKEND_HASH="sha256:$(sha256sum "$QUERY_BACKEND" | awk '{print $1}')"
jq -n \
  --arg path "$QUERY_BACKEND" \
  --arg hash "$BACKEND_HASH" \
  '{name:"fake-read-only",version:"1.0.0",revision:"test-query-backend",path:$path,sha256:$hash,configuration:{status:"not_required"},dependencies:[]}' \
  > "$QUERY_BACKEND_MANIFEST"

set +e
HERMIT_DL_JAR="$TMP_DIR/missing-hermit.jar" \
  HERMIT_DL_CONFIG="$CONFIG" \
  "$MATERIALIZER" source "$ONTOLOGY" >"$MISSING_OUTPUT" 2>"$MISSING_STDERR"
missing_status="$?"
set -e
if [[ "$missing_status" -eq 0 ]]; then
  echo "assertion failed: missing HermiT jar returned success" >&2
  exit 1
fi
test "$missing_status" = "125"
test ! -s "$MISSING_OUTPUT"

TEST_JAR="${HERMIT_DL_TEST_JAR:-}"
if [[ -z "$TEST_JAR" || ! -f "$TEST_JAR" ]]; then
  echo "[test-owl2bench-hermit-dl-materializer] unavailable artifact boundary ok"
  exit 0
fi

EXTRA_JAR="$TMP_DIR/support-1.0.0.jar"
mkdir -p "$TMP_DIR/empty-jar"
printf '%s\n' \
  'Manifest-Version: 1.0' \
  'Implementation-Version: 1.0.0' \
  > "$TMP_DIR/support-manifest.mf"
jar --create --file "$EXTRA_JAR" --manifest "$TMP_DIR/support-manifest.mf" \
  -C "$TMP_DIR/empty-jar" .

export HERMIT_DL_JAR="$TEST_JAR"
export HERMIT_DL_CONFIG="$CONFIG"
export HERMIT_DL_SOURCE_ONTOLOGY="$ONTOLOGY"
export HERMIT_DL_QUERY_DIR="$QUERY_DIR"
export HERMIT_DL_BUILD_DIR="$TMP_DIR/build"

VERSION="$($MATERIALIZER version "$ONTOLOGY")"
test "$VERSION" = "HermiT 1.3.8.1"
SOURCE_GRAPH="$TMP_DIR/source.nq"
CANDIDATE_GRAPH="$TMP_DIR/candidate.nq"
"$MATERIALIZER" source "$ONTOLOGY" > "$SOURCE_GRAPH"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$SOURCE_GRAPH" --output "$PROFILE_CONTEXT/source.nq" >/dev/null
"$MATERIALIZER" export "$SOURCE_GRAPH" > "$CANDIDATE_GRAPH"
test -s "$SOURCE_GRAPH"
test -s "$CANDIDATE_GRAPH"
grep -Fq '<http://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/B> .' "$CANDIDATE_GRAPH"
grep -Fq '<http://example.test/alice> <http://example.test/p> <http://example.test/bob> .' "$CANDIDATE_GRAPH"
grep -Fq '<http://example.test/alice> <http://example.test/q> _:anonymous .' "$CANDIDATE_GRAPH"
grep -Fq '<http://example.test/alice> <http://example.test/age> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .' "$CANDIDATE_GRAPH"
grep -Fq '<http://example.test/parentAge> <http://www.w3.org/2000/01/rdf-schema#range> <http://www.w3.org/2001/XMLSchema#integer> .' "$CANDIDATE_GRAPH"
grep -Fq '_:anonymous <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/B> .' "$CANDIDATE_GRAPH"
if grep -Fq '_:unrelated <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/B> .' "$CANDIDATE_GRAPH"; then
  echo "assertion failed: anonymous class entailment escaped its source individual" >&2
  exit 1
fi

mkdir -p "$PREFIX_QUERY_DIR"
printf '%s\n' \
  'PREFIX ex: <http://example.test/>' \
  'SELECT DISTINCT ?x WHERE { ?x a ex:B }' > "$PREFIX_QUERY_DIR/001.rq"
cat > "$PREFIX_CONFIG" <<'PROPERTIES'
engine.id=HermiT
hermit.version=1.3.8.1
materialization.mode=finite_query_complete
query.namespace=http://unused.example/
PROPERTIES
export HERMIT_DL_CONFIG="$PREFIX_CONFIG"
export HERMIT_DL_QUERY_DIR="$PREFIX_QUERY_DIR"
set +e
"$MATERIALIZER" export "$SOURCE_GRAPH" > "$PREFIX_CANDIDATE_GRAPH" 2> "$TMP_DIR/prefix-stderr"
prefix_status="$?"
set -e
test "$prefix_status" = "0"
grep -Fq '<http://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/B> .' "$PREFIX_CANDIDATE_GRAPH"

mkdir -p "$TBOX_QUERY_DIR"
printf '%s\n' \
  'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>' \
  'PREFIX ex: <http://example.test/>' \
  'SELECT DISTINCT ?x WHERE { ?x rdfs:subClassOf ex:C }' > "$TBOX_QUERY_DIR/001.rq"
export HERMIT_DL_QUERY_DIR="$TBOX_QUERY_DIR"
set +e
"$MATERIALIZER" export "$SOURCE_GRAPH" > "$TBOX_CANDIDATE_GRAPH" 2> "$TMP_DIR/tbox-stderr"
tbox_status="$?"
set -e
test "$tbox_status" = "0"
grep -Fq '<http://example.test/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.test/C> .' "$TBOX_CANDIDATE_GRAPH"

mkdir -p "$DATA_WITNESS_QUERY_DIR"
printf '%s\n' 'SELECT DISTINCT ?x ?y WHERE { ?x :score ?y }' > "$DATA_WITNESS_QUERY_DIR/001.rq"
export HERMIT_DL_CONFIG="$CONFIG"
export HERMIT_DL_QUERY_DIR="$DATA_WITNESS_QUERY_DIR"
"$MATERIALIZER" source "$ONTOLOGY" > "$DATA_WITNESS_SOURCE_GRAPH"
set +e
"$MATERIALIZER" export "$DATA_WITNESS_SOURCE_GRAPH" > "$DATA_WITNESS_CANDIDATE_GRAPH" 2> "$TMP_DIR/data-witness-stderr"
data_witness_status="$?"
set -e
test "$data_witness_status" = "125"
test ! -s "$DATA_WITNESS_CANDIDATE_GRAPH"
grep -Fq 'generated data witness' "$TMP_DIR/data-witness-stderr"

mkdir -p "$CLASS_WITNESS_QUERY_DIR"
printf '%s\n' 'SELECT DISTINCT ?x WHERE { ?x rdf:type :D }' > "$CLASS_WITNESS_QUERY_DIR/001.rq"
export HERMIT_DL_QUERY_DIR="$CLASS_WITNESS_QUERY_DIR"
"$MATERIALIZER" source "$ONTOLOGY" > "$CLASS_WITNESS_SOURCE_GRAPH"
set +e
"$MATERIALIZER" export "$CLASS_WITNESS_SOURCE_GRAPH" > "$CLASS_WITNESS_CANDIDATE_GRAPH" 2> "$TMP_DIR/class-witness-stderr"
class_witness_status="$?"
set -e
test "$class_witness_status" = "125"
test ! -s "$CLASS_WITNESS_CANDIDATE_GRAPH"
grep -Fq 'generated anonymous witness' "$TMP_DIR/class-witness-stderr"

export HERMIT_DL_CONFIG="$CONFIG"
export HERMIT_DL_QUERY_DIR="$QUERY_DIR"

mkdir -p "$WITNESS_QUERY_DIR"
printf '%s\n' 'SELECT DISTINCT ?x ?y WHERE { ?x :witness ?y }' > "$WITNESS_QUERY_DIR/001.rq"
export HERMIT_DL_QUERY_DIR="$WITNESS_QUERY_DIR"
"$MATERIALIZER" source "$ONTOLOGY" > "$WITNESS_SOURCE_GRAPH"
set +e
"$MATERIALIZER" export "$WITNESS_SOURCE_GRAPH" > "$WITNESS_CANDIDATE_GRAPH" 2> "$TMP_DIR/witness-stderr"
witness_status="$?"
set -e
test "$witness_status" = "125"
test ! -s "$WITNESS_CANDIDATE_GRAPH"
grep -Fq 'generated anonymous witness' "$TMP_DIR/witness-stderr"

export HERMIT_DL_QUERY_DIR="$QUERY_DIR"

python3 "$SCRIPT_DIR/hermit_dl_materialization.py" run \
  --ontology "$ONTOLOGY" \
  --query-dir "$QUERY_DIR" \
  --query-manifest "$QUERY_MANIFEST" \
  --hermit-jar "$TEST_JAR" \
  --hermit-classpath "$TEST_JAR:$EXTRA_JAR" \
  --config "$CONFIG" \
  --query-backend "$QUERY_BACKEND" \
  --query-backend-manifest "$QUERY_BACKEND_MANIFEST" \
  --query-backend-name fake-read-only \
  --query-backend-version 1.0.0 \
  --query-backend-revision test-query-backend \
  --profile-context "$PROFILE_CONTEXT" \
  --profile-tbox "$PROFILE_TBOX" \
  --profile-tbox-revision test-profile-tbox \
  --profile-abox "$PROFILE_ABOX" \
  --profile-abox-revision test-profile-abox \
  --generator-artifact "$GENERATOR_ARTIFACT" \
  --generator-revision test-generator-revision \
  --generator-config "$GENERATOR_CONFIG" \
  --generator-config-revision test-generator-config \
  --generator-seed 1 \
  --university-count 1 \
  --dataset-id test-dl-anchor \
  --dataset-artifact "$ONTOLOGY" \
  --dataset-artifact-revision test-dataset-artifact \
  --hermit-revision test-hermit-revision \
  --expected-query-count 1 \
  --output-dir "$MATERIALIZATION_RUN" \
  > "$TMP_DIR/materialization-summary.json" || {
  cat "$TMP_DIR/materialization-summary.json" >&2 || true
  cat "$MATERIALIZATION_RUN/validation.json" >&2 2>/dev/null || true
  find "$MATERIALIZATION_RUN" -maxdepth 2 -type f -print >&2 2>/dev/null || true
  exit 1
}
jq -e '
  .status == "oracle_unavailable"
  and .execution_status == "complete"
  and .profile == "DL"
  and .handoff != null
  and ([paths | select(.[-1] == "oracle" or .[-1] == "oracles" or .[-1] == "answer_key" or .[-1] == "answer_keys")] | length) == 0
' "$TMP_DIR/materialization-summary.json" >/dev/null
jq -e '
  .profiles.DL.status == "complete"
  and .profiles.DL.component.name == "HermiT"
  and .profiles.DL.component.version == "1.3.8.1"
  and .profiles.DL.source_document.format == "RDF/XML"
  and (.profiles.DL.query_backend.identity_manifest.sha256 | startswith("sha256:"))
  and .profiles.DL.query_backend.configuration.status == "not_required"
  and (.profiles.DL.query_backend.dependencies | length) == 0
  and .profiles.DL.query_workload.query_count == 1
  and (.profiles.DL.query_workload.queries | length) == 1
  and (.profiles.DL.query_workload.queries[0].sha256 | startswith("sha256:"))
  and .profiles.DL.generator.seed == 1
  and (.profiles.DL.profile_tbox.revision == "test-profile-tbox")
  and (.profiles.DL.profile_tbox.sha256 | startswith("sha256:"))
  and (.profiles.DL.profile_abox.revision == "test-profile-abox")
  and (.profiles.DL.profile_abox.sha256 | startswith("sha256:"))
  and (.profiles.DL.generator_configuration.revision == "test-generator-config")
  and (.profiles.DL.generator_configuration.sha256 | startswith("sha256:"))
  and (.profiles.DL.dataset.sha256 | startswith("sha256:"))
  and (.profiles.DL.dataset.tbox_sha256 | startswith("sha256:"))
  and (.profiles.DL.dataset.abox_sha256 | startswith("sha256:"))
  and (.profiles.DL.profile_context.sha256 | startswith("sha256:"))
  and ((.profiles.DL.runtime_classpath.artifacts | length) == 2)
  and (.profiles.DL.runtime_classpath.artifacts[0].sha256 | startswith("sha256:"))
  and (.profiles.DL.runtime_classpath.artifacts[1].revision | startswith("sha256:"))
  and .profiles.DL.graph_contract.two_run_determinism_required == true
  and .profiles.DL.determinism.status == "complete"
  and .profiles.DL.determinism.runs == 2
  and (.profiles.DL.determinism.first_graph.sha256 == .profiles.DL.determinism.second_graph.sha256)
  and (.profiles.DL.determinism.first_source_graph.sha256 | startswith("sha256:"))
  and (.profiles.DL.determinism.second_source_graph.sha256 | startswith("sha256:"))
  and (.profiles.DL.determinism.second_obligation_graph.sha256 | startswith("sha256:"))
  and (.profiles.DL.determinism.second_obligation_manifest.sha256 | startswith("sha256:"))
  and .profiles.DL.obligations != null
  and ([paths | select(.[-1] == "oracle" or .[-1] == "oracles" or .[-1] == "answer_key" or .[-1] == "answer_keys")] | length) == 0
' "$MATERIALIZATION_RUN/materialization.json" >/dev/null
jq -e '
  .profiles.DL.obligations.manifest_path
  and .profiles.DL.obligations.graph_path
' "$MATERIALIZATION_RUN/materialization.json" >/dev/null
jq -e '.status == "complete" and .query_handoff.status == "ready"' \
  "$MATERIALIZATION_RUN/validation.json" >/dev/null
jq -e '(.obligations.count > 0) and (.obligations.sha256 | startswith("sha256:"))' \
  "$MATERIALIZATION_RUN/validation.json" >/dev/null
jq -e '.status == "complete" and .profile == "DL" and .obligation_graph.count > 0' \
  "$MATERIALIZATION_RUN/obligations.json" >/dev/null
jq -e '
  .coverage.strategy == "finite_source_domain_query_scope"
  and .coverage.exhaustive == true
  and .coverage.source_term_count > 0
  and .coverage.query_iri_count > 0
' "$MATERIALIZATION_RUN/obligations.json" >/dev/null
jq -e '.status == "ready" and .read_only == true' \
  "$MATERIALIZATION_RUN/handoff.json" >/dev/null

MISSING_OBLIGATIONS_MANIFEST="$TMP_DIR/missing-obligations-manifest.json"
jq 'del(.profiles.DL.obligations)' \
  "$MATERIALIZATION_RUN/materialization.json" > "$MISSING_OBLIGATIONS_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$MATERIALIZATION_RUN/candidate_graph.nq" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$MISSING_OBLIGATIONS_MANIFEST" \
  --output "$TMP_DIR/missing-obligations-validation.json"; then
  echo "assertion failed: HermiT graph without obligations passed validation" >&2
  exit 1
fi
jq -e '
  .status == "not_available"
  and ([.reasons[] | select(. == "artifact_missing:obligations_record")] | length) == 1
' "$TMP_DIR/missing-obligations-validation.json" >/dev/null

MISSING_DETERMINISM_MANIFEST="$TMP_DIR/missing-determinism-manifest.json"
jq 'del(.profiles.DL.determinism)' \
  "$MATERIALIZATION_RUN/materialization.json" > "$MISSING_DETERMINISM_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$MATERIALIZATION_RUN/candidate_graph.nq" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$MISSING_DETERMINISM_MANIFEST" \
  --output "$TMP_DIR/missing-determinism-validation.json"; then
  echo "assertion failed: HermiT graph without determinism evidence passed validation" >&2
  exit 1
fi
jq -e '
  .status == "not_available"
  and ([.reasons[] | select(. == "artifact_missing:determinism_record")] | length) == 1
' "$TMP_DIR/missing-determinism-validation.json" >/dev/null

MISSING_BACKEND_MANIFEST="$TMP_DIR/missing-backend-manifest.json"
jq --arg path "$TMP_DIR/missing-query-backend.json" \
  '.profiles.DL.query_backend.identity_manifest.path = $path' \
  "$MATERIALIZATION_RUN/materialization.json" > "$MISSING_BACKEND_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$MATERIALIZATION_RUN/candidate_graph.nq" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$MISSING_BACKEND_MANIFEST" \
  --output "$TMP_DIR/missing-backend-validation.json"; then
  echo "assertion failed: missing query backend manifest passed validation" >&2
  exit 1
fi
jq -e '
  .status == "not_available"
  and ([.reasons[] | select(startswith("artifact_missing:query_backend_identity_manifest:"))] | length) == 1
' "$TMP_DIR/missing-backend-validation.json" >/dev/null

OMITTED_LINE='<http://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/B> .'
grep -Fqx "$OMITTED_LINE" "$MATERIALIZATION_RUN/candidate_graph.nq"
grep -Fqx "$OMITTED_LINE" "$MATERIALIZATION_RUN/obligations.nq"
OMITTED_CANDIDATE="$TMP_DIR/omitted-candidate.nq"
OMITTED_OBLIGATION_GRAPH="$TMP_DIR/omitted-obligations.nq"
grep -Fvx "$OMITTED_LINE" "$MATERIALIZATION_RUN/candidate_graph.nq" > "$OMITTED_CANDIDATE"
grep -Fvx "$OMITTED_LINE" "$MATERIALIZATION_RUN/obligations.nq" > "$OMITTED_OBLIGATION_GRAPH"
OMITTED_OBLIGATION_MANIFEST="$TMP_DIR/omitted-obligations.json"
OMITTED_OBLIGATION_HASH="sha256:$(sha256sum "$OMITTED_OBLIGATION_GRAPH" | awk '{print $1}')"
OMITTED_OBLIGATION_COUNT="$(grep -cve '^$' "$OMITTED_OBLIGATION_GRAPH")"
jq --arg path "$OMITTED_OBLIGATION_GRAPH" \
  --arg hash "$OMITTED_OBLIGATION_HASH" \
  --argjson count "$OMITTED_OBLIGATION_COUNT" \
  '.obligation_graph.path = $path
   | .obligation_graph.sha256 = $hash
   | .obligation_graph.count = $count' \
  "$MATERIALIZATION_RUN/obligations.json" > "$OMITTED_OBLIGATION_MANIFEST"
OMITTED_MANIFEST="$TMP_DIR/omitted-materialization.json"
OMITTED_CANDIDATE_HASH="sha256:$(sha256sum "$OMITTED_CANDIDATE" | awk '{print $1}')"
jq --arg candidate "$OMITTED_CANDIDATE" \
  --arg candidate_hash "$OMITTED_CANDIDATE_HASH" \
  --arg obligation_manifest "$OMITTED_OBLIGATION_MANIFEST" \
  --arg obligation_graph "$OMITTED_OBLIGATION_GRAPH" \
  '.profiles.DL.candidate_graph.path = $candidate
   | .profiles.DL.candidate_graph.sha256 = $candidate_hash
   | .profiles.DL.obligations.manifest_path = $obligation_manifest
   | .profiles.DL.obligations.graph_path = $obligation_graph' \
  "$MATERIALIZATION_RUN/materialization.json" > "$OMITTED_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$OMITTED_CANDIDATE" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$OMITTED_MANIFEST" \
  --output "$TMP_DIR/omitted-validation.json"; then
  echo "assertion failed: omitted candidate and obligation passed validation" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and ([.reasons[] | select(startswith("independent_obligation_missing_from_candidate:"))] | length) == 1
  and ([.reasons[] | select(startswith("independent_obligation_missing_from_manifest:"))] | length) == 1
' "$TMP_DIR/omitted-validation.json" >/dev/null

EXTRA_CANDIDATE="$TMP_DIR/extra-candidate.nq"
printf '%s\n' '<http://example.test/alice> <http://example.test/q> <http://example.test/C> .' \
  > "$EXTRA_CANDIDATE"
cat "$MATERIALIZATION_RUN/candidate_graph.nq" >> "$EXTRA_CANDIDATE"
EXTRA_MANIFEST="$TMP_DIR/extra-materialization.json"
EXTRA_CANDIDATE_HASH="sha256:$(sha256sum "$EXTRA_CANDIDATE" | awk '{print $1}')"
jq --arg candidate "$EXTRA_CANDIDATE" \
  --arg candidate_hash "$EXTRA_CANDIDATE_HASH" \
  '.profiles.DL.candidate_graph.path = $candidate
   | .profiles.DL.candidate_graph.sha256 = $candidate_hash' \
  "$MATERIALIZATION_RUN/materialization.json" > "$EXTRA_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$EXTRA_CANDIDATE" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$EXTRA_MANIFEST" \
  --output "$TMP_DIR/extra-validation.json"; then
  echo "assertion failed: candidate inference outside obligations passed validation" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and ([.reasons[] | select(. == "candidate_inference_missing_from_obligations")] | length) == 1
' "$TMP_DIR/extra-validation.json" >/dev/null

SOURCE_OBLIGATION_LINE="$(head -n 1 "$MATERIALIZATION_RUN/source_graph.nq")"
SOURCE_IN_OBLIGATION_GRAPH="$TMP_DIR/source-in-obligations.nq"
{
  printf '%s\n' "$SOURCE_OBLIGATION_LINE"
  cat "$MATERIALIZATION_RUN/obligations.nq"
} > "$SOURCE_IN_OBLIGATION_GRAPH"
SOURCE_IN_OBLIGATION_MANIFEST="$TMP_DIR/source-in-obligations.json"
SOURCE_IN_OBLIGATION_HASH="sha256:$(sha256sum "$SOURCE_IN_OBLIGATION_GRAPH" | awk '{print $1}')"
SOURCE_IN_OBLIGATION_COUNT="$(grep -cve '^$' "$SOURCE_IN_OBLIGATION_GRAPH")"
jq --arg path "$SOURCE_IN_OBLIGATION_GRAPH" \
  --arg hash "$SOURCE_IN_OBLIGATION_HASH" \
  --argjson count "$SOURCE_IN_OBLIGATION_COUNT" \
  '.obligation_graph.path = $path
   | .obligation_graph.sha256 = $hash
   | .obligation_graph.count = $count' \
  "$MATERIALIZATION_RUN/obligations.json" > "$SOURCE_IN_OBLIGATION_MANIFEST"
SOURCE_IN_MANIFEST="$TMP_DIR/source-in-materialization.json"
jq --arg obligation_manifest "$SOURCE_IN_OBLIGATION_MANIFEST" \
  --arg obligation_graph "$SOURCE_IN_OBLIGATION_GRAPH" \
  '.profiles.DL.obligations.manifest_path = $obligation_manifest
   | .profiles.DL.obligations.graph_path = $obligation_graph' \
  "$MATERIALIZATION_RUN/materialization.json" > "$SOURCE_IN_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$MATERIALIZATION_RUN/candidate_graph.nq" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$SOURCE_IN_MANIFEST" \
  --output "$TMP_DIR/source-in-validation.json"; then
  echo "assertion failed: source triple in obligations passed validation" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and ([.reasons[] | select(. == "obligation_contains_source_quad")] | length) == 1
' "$TMP_DIR/source-in-validation.json" >/dev/null
"$QUERY_BACKEND" query "$QUERY_DIR/001.rq" \
  "$MATERIALIZATION_RUN/graph.nq" "$MATERIALIZATION_RUN/handoff.json"

OBLIGATION_LINE="$(head -n 1 "$MATERIALIZATION_RUN/obligations.nq")"
MISSING_OBLIGATION_CANDIDATE="$TMP_DIR/missing-obligation-candidate.nq"
grep -Fvx "$OBLIGATION_LINE" "$MATERIALIZATION_RUN/candidate_graph.nq" \
  > "$MISSING_OBLIGATION_CANDIDATE"
MISSING_OBLIGATION_MANIFEST="$TMP_DIR/missing-obligation-manifest.json"
MISSING_OBLIGATION_HASH="sha256:$(sha256sum "$MISSING_OBLIGATION_CANDIDATE" | awk '{print $1}')"
jq --arg path "$MISSING_OBLIGATION_CANDIDATE" --arg hash "$MISSING_OBLIGATION_HASH" \
  '.profiles.DL.candidate_graph.path = $path
   | .profiles.DL.candidate_graph.sha256 = $hash' \
  "$MATERIALIZATION_RUN/materialization.json" > "$MISSING_OBLIGATION_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$MATERIALIZATION_RUN/source_graph.nq" \
  --candidate-graph "$MISSING_OBLIGATION_CANDIDATE" \
  --graph "$MATERIALIZATION_RUN/graph.nq" \
  --manifest "$MISSING_OBLIGATION_MANIFEST" \
  --output "$TMP_DIR/missing-obligation-validation.json"; then
  echo "assertion failed: missing obligation passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and ([.reasons[] | select(. == "obligation_missing_from_candidate")] | length) == 1' \
  "$TMP_DIR/missing-obligation-validation.json" >/dev/null

python3 "$SCRIPT_DIR/hermit_dl_materialization.py" run \
  --ontology "$ONTOLOGY" \
  --query-dir "$QUERY_DIR" \
  --query-manifest "$QUERY_MANIFEST" \
  --hermit-jar "$TEST_JAR" \
  --hermit-classpath "$TEST_JAR:$EXTRA_JAR" \
  --config "$CONFIG" \
  --query-backend "$QUERY_BACKEND" \
  --query-backend-manifest "$QUERY_BACKEND_MANIFEST" \
  --query-backend-name fake-read-only \
  --query-backend-version 1.0.0 \
  --query-backend-revision test-query-backend \
  --profile-context "$PROFILE_CONTEXT" \
  --profile-tbox "$PROFILE_TBOX" \
  --profile-tbox-revision test-profile-tbox \
  --profile-abox "$PROFILE_ABOX" \
  --profile-abox-revision test-profile-abox \
  --generator-artifact "$GENERATOR_ARTIFACT" \
  --generator-revision test-generator-revision \
  --generator-config "$GENERATOR_CONFIG" \
  --generator-config-revision test-generator-config \
  --generator-seed 1 \
  --university-count 1 \
  --dataset-id test-dl-anchor \
  --dataset-artifact "$ONTOLOGY" \
  --dataset-artifact-revision test-dataset-artifact \
  --hermit-revision test-hermit-revision \
  --expected-query-count 1 \
  --output-dir "$MATERIALIZATION_RUN_2" \
  > "$TMP_DIR/materialization-summary-2.json"
test "$(sha256sum "$MATERIALIZATION_RUN/graph.nq" | awk '{print $1}')" = \
  "$(sha256sum "$MATERIALIZATION_RUN_2/graph.nq" | awk '{print $1}')"

echo "[test-owl2bench-hermit-dl-materializer] ok"
