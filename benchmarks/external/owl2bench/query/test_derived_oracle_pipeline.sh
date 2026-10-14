#!/usr/bin/env bash
# Derivation-and-audit contract tests for the RL derived-oracle pilot (#81).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

DATASET="$WORK/mini.owl"
QUERY="$WORK/mini.rq"
OUT="$WORK/oracle"
mkdir -p "$OUT"

cat >"$DATASET" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/m" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#" xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="#Student"/>
  <owl:Class rdf:about="#Person"/>
  <rdfs:Class rdf:about="#GradStudent">
    <rdfs:subClassOf rdf:resource="#Student"/>
  </rdfs:Class>
  <owl:Class rdf:about="#Student">
    <rdfs:subClassOf rdf:resource="#Person"/>
  </owl:Class>
  <owl:NamedIndividual rdf:about="#Bob">
    <rdf:type rdf:resource="#GradStudent"/>
  </owl:NamedIndividual>
</rdf:RDF>
XML

printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :Person }' >"$QUERY"

DATASET_SHA="$(sha256sum "$DATASET" | cut -d' ' -f1)"
QUERY_SHA="$(sha256sum "$QUERY" | cut -d' ' -f1)"

derive() {
  python3 "$HERE/derived_oracle_pipeline.py" derive \
    --dataset "$DATASET" --query "$QUERY" --output-dir "$1" \
    --case-id "owl2bench_query_012:RL" --query-id "owl2bench_query_012" \
    --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
    --source-query-sha256 "$QUERY_SHA" --source-corpus "test-corpus" \
    --source-revision "test-revision" --base "http://ex.org/m"
}

RESULT="$(derive "$OUT")"
echo "$RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
ROWS="$(echo "$RESULT" | jq -r '.answer_rows')"
[[ "$ROWS" == "1" ]] || { echo "expected 1 Person row (Bob via subsumption), got $ROWS"; exit 1; }

# Determinism: a second run reproduces identical answer and proof hashes.
derive "$WORK/second" >/dev/null
diff <(jq -S . "$OUT/derived-oracle-manifest.json") <(jq -S . "$WORK/second/derived-oracle-manifest.json") >/dev/null
[[ "$(sha256sum "$OUT/answers/owl2bench_query_012:RL.srx" | cut -d' ' -f1)" == "$(sha256sum "$WORK/second/answers/owl2bench_query_012:RL.srx" | cut -d' ' -f1)" ]]

# Candidate rows must fail closed in the existing oracle lookup.
LOOKUP="$(python3 "$HERE/oracle_lookup.py" --oracle-dir "$OUT" \
  --manifest "$OUT/derived-oracle-manifest.json" \
  --case-id "owl2bench_query_012:RL" --query-id "owl2bench_query_012" --profile RL \
  --source-query-sha256 "$QUERY_SHA" --source-corpus "test-corpus" --source-revision "test-revision" \
  --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
  --canonicalizer-version "owl2bench-query-canonicalizer/2")"
echo "$LOOKUP" | jq -e '.status == "unavailable"' >/dev/null

# Identity mismatch fails closed too.
LOOKUP_BAD="$(python3 "$HERE/oracle_lookup.py" --oracle-dir "$OUT" \
  --manifest "$OUT/derived-oracle-manifest.json" \
  --case-id "owl2bench_query_012:RL" --query-id "owl2bench_query_012" --profile RL \
  --source-query-sha256 "$QUERY_SHA" --source-corpus "other-corpus" --source-revision "test-revision" \
  --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
  --canonicalizer-version "owl2bench-query-canonicalizer/2")"
echo "$LOOKUP_BAD" | jq -e '.status == "unavailable" and (.reason | startswith("identity_mismatch"))' >/dev/null

# --- Audit ---------------------------------------------------------------
AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$OUT" --base "http://ex.org/m")"
echo "$AUDIT" | jq -e '.rows[0].verdict == "pass"' >/dev/null

# After a passing audit the oracle becomes available through the lookup.
LOOKUP_OK="$(python3 "$HERE/oracle_lookup.py" --oracle-dir "$OUT" \
  --manifest "$OUT/derived-oracle-manifest.json" \
  --case-id "owl2bench_query_012:RL" --query-id "owl2bench_query_012" --profile RL \
  --source-query-sha256 "$QUERY_SHA" --source-corpus "test-corpus" --source-revision "test-revision" \
  --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
  --canonicalizer-version "owl2bench-query-canonicalizer/2")"
echo "$LOOKUP_OK" | jq -e '.status == "available" and .audit_status == "independently_audited" and .oracle_class == "derived"' >/dev/null

# The query lane composes case ids as "<query_id>__<profile>"; that form must
# resolve the same accepted derived oracle as the manifest's canonical form.
LOOKUP_LANE_FORM="$(python3 "$HERE/oracle_lookup.py" --oracle-dir "$OUT" \
  --manifest "$OUT/derived-oracle-manifest.json" \
  --case-id "owl2bench_query_012__RL" --query-id "owl2bench_query_012" --profile RL \
  --source-query-sha256 "$QUERY_SHA" --source-corpus "test-corpus" --source-revision "test-revision" \
  --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
  --canonicalizer-version "owl2bench-query-canonicalizer/2")"
echo "$LOOKUP_LANE_FORM" | jq -e '.status == "available" and .audit_status == "independently_audited"' >/dev/null

# Tamper: corrupt the answer artifact, re-derive the candidate, audit must reject.
rm -rf "$WORK/tamper"; mkdir -p "$WORK/tamper"
derive "$WORK/tamper" >/dev/null
echo "<!-- tampered -->" >>"$WORK/tamper/answers/owl2bench_query_012:RL.srx"
AUDIT_BAD="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/tamper" --base "http://ex.org/m")"
echo "$AUDIT_BAD" | jq -e '.rows[0].verdict == "fail"' >/dev/null
AFTER="$(python3 "$HERE/oracle_lookup.py" --oracle-dir "$WORK/tamper" \
  --manifest "$WORK/tamper/derived-oracle-manifest.json" \
  --case-id "owl2bench_query_012:RL" --query-id "owl2bench_query_012" --profile RL \
  --source-query-sha256 "$QUERY_SHA" --source-corpus "test-corpus" --source-revision "test-revision" \
  --dataset-id "mini-rl-seed1" --dataset-sha256 "$DATASET_SHA" \
  --canonicalizer-version "owl2bench-query-canonicalizer/2")"
echo "$AFTER" | jq -e '.status == "unavailable"' >/dev/null

# --- Namespace split (the OWL2Bench anchor shape) -------------------------
# The generator serializes the TBox under the document xml:base while every
# instance typing and property assertion uses its fixed prefix. Query terms
# must follow the dominant ABox vocabulary, not the xml:base.
SPLIT="$WORK/split"
mkdir -p "$SPLIT"
cat >"$SPLIT/dataset.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/tbox"
     xmlns:voc="http://ex.org/voc#"
     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
     xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
     xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:Class rdf:about="http://ex.org/tbox#Person"/>
    <owl:Class rdf:about="http://ex.org/tbox#Man">
        <rdfs:subClassOf rdf:resource="http://ex.org/tbox#Person"/>
    </owl:Class>
    <owl:NamedIndividual rdf:about="http://ex.org/voc#bob">
        <rdf:type rdf:resource="http://ex.org/voc#Man"/>
        <voc:knows rdf:resource="http://ex.org/voc#alice"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/voc#alice">
        <rdf:type rdf:resource="http://ex.org/voc#Man"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :Man }' >"$SPLIT/query.rq"

SPLIT_DATASET_SHA="$(sha256sum "$SPLIT/dataset.owl" | cut -d' ' -f1)"
SPLIT_QUERY_SHA="$(sha256sum "$SPLIT/query.rq" | cut -d' ' -f1)"

derive_split() {
  python3 "$HERE/derived_oracle_pipeline.py" derive \
    --dataset "$SPLIT/dataset.owl" --query "$SPLIT/query.rq" --output-dir "$1" \
    --case-id "owl2bench_query_005:RL" --query-id "owl2bench_query_005" \
    --dataset-id "split-rl" --dataset-sha256 "$SPLIT_DATASET_SHA" \
    --source-query-sha256 "$SPLIT_QUERY_SHA" --source-corpus "test-corpus" \
    --source-revision "test-revision" --base "http://ex.org/tbox"
}

SPLIT_RESULT="$(derive_split "$SPLIT/oracle")"
echo "$SPLIT_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
SPLIT_ROWS="$(echo "$SPLIT_RESULT" | jq -r '.answer_rows')"
[[ "$SPLIT_ROWS" == "2" ]] || { echo "expected 2 :Man rows (bob, alice) from the ABox vocabulary, got $SPLIT_ROWS"; exit 1; }

# v1 golden pin: the shipped-track RL derivation over the first mini fixture
# must stay byte-identical while the patched track evolves beside it.
V1_GOLDEN="9cb31dfc4bc11aec651539a12f5666c504424ab235677d46ce06dbe384a87aae"
[[ "$(sha256sum "$OUT/answers/owl2bench_query_012:RL.srx" | cut -d' ' -f1)" == "$V1_GOLDEN" ]] \
  || { echo "shipped-track v1 answer hash drifted from the pinned golden value"; exit 1; }

# Provenance must record which namespace the query was expanded into and that
# the dataset vocabulary differs from the xml:base side.
jq -e '.query_namespace == "http://ex.org/voc#" and .vocabulary_split == true' \
  "$SPLIT/oracle/provenance/owl2bench_query_005:RL.json" >/dev/null

SPLIT_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$SPLIT/oracle" --base "http://ex.org/tbox")"
echo "$SPLIT_AUDIT" | jq -e '.rows[0].verdict == "pass"' >/dev/null

# --- QL profile: inverse-of derivation through the QL rule table -----------
QL_DIR="$WORK/ql"
mkdir -p "$QL_DIR"
cat >"$QL_DIR/mini-ql.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/q" xmlns:voc="http://ex.org/voc#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:Class rdf:about="http://ex.org/voc#Person"/>
    <owl:Class rdf:about="http://ex.org/voc#Student">
        <rdfs:subClassOf rdf:resource="http://ex.org/voc#Person"/>
    </owl:Class>
    <owl:ObjectProperty rdf:about="http://ex.org/voc#evaluates"/>
    <owl:ObjectProperty rdf:about="http://ex.org/voc#evaluatedBy">
        <owl:inverseOf rdf:resource="http://ex.org/voc#evaluates"/>
        <rdfs:domain rdf:resource="http://ex.org/voc#Student"/>
    </owl:ObjectProperty>
    <owl:NamedIndividual rdf:about="http://ex.org/voc#profX">
        <voc:evaluates rdf:resource="http://ex.org/voc#alice"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :Person }' >"$QL_DIR/mini-ql.rq"
QL_DATASET_SHA="$(sha256sum "$QL_DIR/mini-ql.owl" | cut -d' ' -f1)"
QL_QUERY_SHA="$(sha256sum "$QL_DIR/mini-ql.rq" | cut -d' ' -f1)"

QL_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$QL_DIR/mini-ql.owl" --query "$QL_DIR/mini-ql.rq" --output-dir "$WORK/ql-oracle" \
  --case-id "owl2bench_query_001:QL" --query-id "owl2bench_query_001" --profile QL \
  --dataset-id "mini-ql-seed1" --dataset-sha256 "$QL_DATASET_SHA" \
  --source-query-sha256 "$QL_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/q")"
echo "$QL_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
QL_ROWS="$(echo "$QL_RESULT" | jq -r '.answer_rows')"
[[ "$QL_ROWS" == "1" ]] || { echo "expected 1 Person row (alice via prp-inv2 + domain), got $QL_ROWS"; exit 1; }
jq -e '.rule_table_version == "owl2-ql-rule-table/1" and .profile == "QL"' \
  "$WORK/ql-oracle/provenance/owl2bench_query_001:QL.json" >/dev/null

QL_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/ql-oracle" --base "http://ex.org/q")"
echo "$QL_AUDIT" | jq -e '.rows[0].verdict == "pass"' >/dev/null

# --- EL profile: hasValue derivation + property-characteristic boundary ----
EL_DIR="$WORK/el"
mkdir -p "$EL_DIR"
cat >"$EL_DIR/mini-el.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/e" xmlns:voc="http://ex.org/voc#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:Restriction rdf:nodeID="fanRestriction">
        <owl:onProperty rdf:resource="http://ex.org/voc#isCrazyAbout"/>
        <owl:hasValue rdf:resource="http://ex.org/voc#T20"/>
    </owl:Restriction>
    <owl:Class rdf:about="http://ex.org/voc#Fan">
        <rdfs:subClassOf rdf:nodeID="fanRestriction"/>
    </owl:Class>
    <owl:ObjectProperty rdf:about="http://ex.org/voc#knows">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#SymmetricProperty"/>
    </owl:ObjectProperty>
    <owl:NamedIndividual rdf:about="http://ex.org/voc#bob">
        <voc:isCrazyAbout rdf:resource="http://ex.org/voc#T20"/>
        <voc:knows rdf:resource="http://ex.org/voc#alice"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :Fan }' >"$EL_DIR/type-query.rq"
printf 'SELECT DISTINCT ?x ?y WHERE { ?x :knows ?y }' >"$EL_DIR/knows-query.rq"
EL_DATASET_SHA="$(sha256sum "$EL_DIR/mini-el.owl" | cut -d' ' -f1)"
EL_TYPE_QUERY_SHA="$(sha256sum "$EL_DIR/type-query.rq" | cut -d' ' -f1)"
EL_KNOWS_QUERY_SHA="$(sha256sum "$EL_DIR/knows-query.rq" | cut -d' ' -f1)"

EL_TYPE_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$EL_DIR/mini-el.owl" --query "$EL_DIR/type-query.rq" --output-dir "$WORK/el-oracle" \
  --case-id "owl2bench_query_005:EL" --query-id "owl2bench_query_005" --profile EL \
  --dataset-id "mini-el-seed1" --dataset-sha256 "$EL_DATASET_SHA" \
  --source-query-sha256 "$EL_TYPE_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/e")"
echo "$EL_TYPE_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
EL_TYPE_ROWS="$(echo "$EL_TYPE_RESULT" | jq -r '.answer_rows')"
[[ "$EL_TYPE_ROWS" == "1" ]] || { echo "expected 1 Fan row (bob via cls-hv2 over the subClassOf edge), got $EL_TYPE_ROWS"; exit 1; }
jq -e '.rule_table_version == "owl2-el-rule-table/1" and .profile == "EL"' \
  "$WORK/el-oracle/provenance/owl2bench_query_005:EL.json" >/dev/null

EL_KNOWS_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$EL_DIR/mini-el.owl" --query "$EL_DIR/knows-query.rq" --output-dir "$WORK/el-oracle" \
  --case-id "owl2bench_query_001:EL" --query-id "owl2bench_query_001" --profile EL \
  --dataset-id "mini-el-seed1" --dataset-sha256 "$EL_DATASET_SHA" \
  --source-query-sha256 "$EL_KNOWS_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/e")"
echo "$EL_KNOWS_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
EL_KNOWS_ROWS="$(echo "$EL_KNOWS_RESULT" | jq -r '.answer_rows')"
[[ "$EL_KNOWS_ROWS" == "1" ]] || { echo "expected 1 knows row (symmetric characteristic outside the EL table must not derive the reverse assertion), got $EL_KNOWS_ROWS"; exit 1; }

EL_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/el-oracle" --base "http://ex.org/e")"
echo "$EL_AUDIT" | jq -e '[.rows[] | select(.verdict == "pass")] | length == 2' >/dev/null

# --- DL profile: uncovered cardinality construct, exact-empty answer -------
DL_DIR="$WORK/dl"
mkdir -p "$DL_DIR"
cat >"$DL_DIR/mini-dl.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/d" xmlns:voc="http://ex.org/voc#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:Class rdf:about="http://ex.org/t#UGStudent">
        <owl:equivalentClass>
            <owl:Restriction>
                <owl:onProperty rdf:resource="http://ex.org/t#enrollFor"/>
                <owl:qualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:qualifiedCardinality>
                <owl:onClass rdf:resource="http://ex.org/t#UGProgram"/>
            </owl:Restriction>
        </owl:equivalentClass>
    </owl:Class>
    <owl:NamedIndividual rdf:about="http://ex.org/voc#bob">
        <voc:enrollFor rdf:resource="http://ex.org/voc#prog1"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :UGStudent }' >"$DL_DIR/ug-query.rq"
DL_DATASET_SHA="$(sha256sum "$DL_DIR/mini-dl.owl" | cut -d' ' -f1)"
DL_QUERY_SHA="$(sha256sum "$DL_DIR/ug-query.rq" | cut -d' ' -f1)"

DL_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$DL_DIR/mini-dl.owl" --query "$DL_DIR/ug-query.rq" --output-dir "$WORK/dl-oracle" \
  --case-id "owl2bench_query_017:DL" --query-id "owl2bench_query_017" --profile DL \
  --dataset-id "mini-dl-seed1" --dataset-sha256 "$DL_DATASET_SHA" \
  --source-query-sha256 "$DL_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/d")"
echo "$DL_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
DL_ROWS="$(echo "$DL_RESULT" | jq -r '.answer_rows')"
[[ "$DL_ROWS" == "0" ]] || { echo "expected 0 UGStudent rows (queried IRI is axiom-absent), got $DL_ROWS"; exit 1; }
jq -e '.rule_table_version == "owl2-dl-rule-table/1" and .profile == "DL"' \
  "$WORK/dl-oracle/provenance/owl2bench_query_017:DL.json" >/dev/null
jq -e '[.uncovered_axiom_inventory[] | select(.predicate == "http://www.w3.org/2002/07/owl#qualifiedCardinality")] | length >= 1' \
  "$WORK/dl-oracle/proof/owl2bench_query_017:DL.json" >/dev/null

DL_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/dl-oracle" --base "http://ex.org/d")"
echo "$DL_AUDIT" | jq -e '.rows[0].verdict == "pass"' >/dev/null

# --- patched track (v2): connected expressions + prp-ref -------------------
V2_DIR="$WORK/v2"
mkdir -p "$V2_DIR"
cat >"$V2_DIR/mini-v2.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/v2" xmlns:voc="http://ex.org/v#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:ObjectProperty rdf:about="http://ex.org/v#knows">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#ReflexiveProperty"/>
    </owl:ObjectProperty>
    <owl:ObjectProperty rdf:about="http://ex.org/v#hasCollaborationWith">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#SymmetricProperty"/>
    </owl:ObjectProperty>
    <owl:Class rdf:about="http://ex.org/v#Fan">
        <owl:equivalentClass>
            <owl:Restriction>
                <owl:onProperty rdf:resource="http://ex.org/v#isCrazyAbout"/>
                <owl:hasValue rdf:resource="http://ex.org/v#T20"/>
            </owl:Restriction>
        </owl:equivalentClass>
    </owl:Class>
    <owl:NamedIndividual rdf:about="http://ex.org/v#bob">
        <voc:isCrazyAbout rdf:resource="http://ex.org/v#T20"/>
        <voc:knows rdf:resource="http://ex.org/v#alice"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/v#alice">
        <voc:hasCollaborationWith rdf:resource="http://ex.org/v#bob"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :Fan }' >"$V2_DIR/fan.rq"
printf 'SELECT DISTINCT ?x ?y WHERE { ?x :knows ?y }' >"$V2_DIR/knows.rq"
V2_DATASET_SHA="$(sha256sum "$V2_DIR/mini-v2.owl" | cut -d' ' -f1)"
V2_FAN_QUERY_SHA="$(sha256sum "$V2_DIR/fan.rq" | cut -d' ' -f1)"
V2_KNOWS_QUERY_SHA="$(sha256sum "$V2_DIR/knows.rq" | cut -d' ' -f1)"

# Shipped track first: the wrapper severs the equivalentClass edge and no
# declared individuals exist, so both queries stay at their asserted minimum.
V2_SHIPPED_FAN="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2_DIR/mini-v2.owl" --query "$V2_DIR/fan.rq" --output-dir "$WORK/v2-shipped" \
  --case-id "owl2bench_query_005:EL" --query-id "owl2bench_query_005" --profile EL \
  --dataset-id "mini-v2" --dataset-sha256 "$V2_DATASET_SHA" \
  --source-query-sha256 "$V2_FAN_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2")"
[[ "$(echo "$V2_SHIPPED_FAN" | jq -r '.answer_rows')" == "0" ]] \
  || { echo "shipped track must not see through the wrapper"; exit 1; }

# Patched track: the restriction is connected (scm-eqc1 + cls-hv2 make bob a
# Fan) and prp-ref adds the declared individuals' self-pairs.
V2_FAN_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2_DIR/mini-v2.owl" --query "$V2_DIR/fan.rq" --output-dir "$WORK/v2-oracle" \
  --case-id "owl2bench_query_005:EL" --query-id "owl2bench_query_005" --profile EL --track patched \
  --dataset-id "mini-v2" --dataset-sha256 "$V2_DATASET_SHA" \
  --source-query-sha256 "$V2_FAN_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2")"
echo "$V2_FAN_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
[[ "$(echo "$V2_FAN_RESULT" | jq -r '.answer_rows')" == "1" ]] \
  || { echo "patched track must derive bob as a Fan through the connected restriction, got $(echo "$V2_FAN_RESULT" | jq -r '.answer_rows')"; exit 1; }
jq -e '.track == "patched" and .rule_table_version == "owl2-el-rule-table/2" and .pipeline_version == "derived-oracle-pipeline/2"' \
  "$WORK/v2-oracle/provenance/owl2bench_query_005:EL.json" >/dev/null

V2_KNOWS_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2_DIR/mini-v2.owl" --query "$V2_DIR/knows.rq" --output-dir "$WORK/v2-oracle" \
  --case-id "owl2bench_query_001:EL" --query-id "owl2bench_query_001" --profile EL --track patched \
  --dataset-id "mini-v2" --dataset-sha256 "$V2_DATASET_SHA" \
  --source-query-sha256 "$V2_KNOWS_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2")"
echo "$V2_KNOWS_RESULT" | jq -e '.status == "derived" and .acceptance_status == "candidate"' >/dev/null
V2_KNOWS_ROWS="$(echo "$V2_KNOWS_RESULT" | jq -r '.answer_rows')"
[[ "$V2_KNOWS_ROWS" == "3" ]] || { echo "patched track must derive the asserted pair plus two prp-ref self-pairs (3 rows), got $V2_KNOWS_ROWS"; exit 1; }

V2_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/v2-oracle" --base "http://ex.org/v2")"
echo "$V2_AUDIT" | jq -e '[.rows[] | select(.verdict == "pass")] | length == 2' >/dev/null

# QL under the patched track keeps its rule set (reflexivity is outside
# DL-Lite_R) but must still carry the patched identity, never a /1 version.
V2_QL_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2_DIR/mini-v2.owl" --query "$V2_DIR/knows.rq" --output-dir "$WORK/v2-ql" \
  --case-id "owl2bench_query_001:QL" --query-id "owl2bench_query_001" --profile QL --track patched \
  --dataset-id "mini-v2" --dataset-sha256 "$V2_DATASET_SHA" \
  --source-query-sha256 "$V2_KNOWS_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2")"
[[ "$(echo "$V2_QL_RESULT" | jq -r '.answer_rows')" == "1" ]] \
  || { echo "QL patched track must not add reflexive pairs (1 row expected), got $(echo "$V2_QL_RESULT" | jq -r '.answer_rows')"; exit 1; }
jq -e '.track == "patched" and .rule_table_version == "owl2-ql-rule-table/2"' \
  "$WORK/v2-ql/provenance/owl2bench_query_001:QL.json" >/dev/null

# Symmetry is QL-expressible: the QL tables derive the reverse assertion,
# while EL (which forbids symmetric properties) leaves it underived.
printf 'SELECT DISTINCT ?x ?y WHERE { ?x :hasCollaborationWith ?y }' >"$V2_DIR/collab.rq"
V2_COLLAB_QUERY_SHA="$(sha256sum "$V2_DIR/collab.rq" | cut -d' ' -f1)"
for prof in QL EL; do
  V2_SYMMETRIC_RESULT="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
    --dataset "$V2_DIR/mini-v2.owl" --query "$V2_DIR/collab.rq" --output-dir "$WORK/v2-symmetric-$prof" \
    --case-id "owl2bench_query_010:$prof" --query-id "owl2bench_query_010" --profile "$prof" --track patched \
    --dataset-id "mini-v2" --dataset-sha256 "$V2_DATASET_SHA" \
    --source-query-sha256 "$V2_COLLAB_QUERY_SHA" --source-corpus "test-corpus" \
    --source-revision "test-revision" --base "http://ex.org/v2")"
  expected=2; [[ "$prof" == "EL" ]] && expected=1
  rows="$(echo "$V2_SYMMETRIC_RESULT" | jq -r '.answer_rows')"
  [[ "$rows" == "$expected" ]] \
    || { echo "$prof patched track symmetric handling expected $expected rows, got $rows"; exit 1; }
done

# --- patched track constructs: hasSelf (006), exact/min qualified ----------
# cardinality (017/018) in the generator's nested serialization -------------
V2C_DIR="$WORK/v2c"
mkdir -p "$V2C_DIR"
cat >"$V2C_DIR/mini-v2c.owl" <<'XML'
<?xml version="1.0"?>
<rdf:RDF xml:base="http://ex.org/v2c" xmlns:voc="http://ex.org/w#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
    <owl:ObjectProperty rdf:about="http://ex.org/w#knows">
        <rdf:type rdf:resource="http://www.w3.org/2002/07/owl#ReflexiveProperty"/>
    </owl:ObjectProperty>
    <owl:ObjectProperty rdf:about="http://ex.org/w#enrollFor">
        <rdfs:domain rdf:resource="http://ex.org/w#Student"/>
    </owl:ObjectProperty>
    <owl:ObjectProperty rdf:about="http://ex.org/w#likes"/>
    <owl:Class rdf:about="http://ex.org/w#SelfAwarePerson">
        <owl:equivalentClass>
            <owl:Class>
                <owl:intersectionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://ex.org/w#Person"/>
                    <owl:Restriction>
                        <owl:onProperty rdf:resource="http://ex.org/w#knows"/>
                        <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
                    </owl:Restriction>
                </owl:intersectionOf>
            </owl:Class>
        </owl:equivalentClass>
    </owl:Class>
    <owl:Class rdf:about="http://ex.org/w#UGStudent">
        <owl:equivalentClass>
            <owl:Class>
                <owl:intersectionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://ex.org/w#Student"/>
                    <owl:Restriction>
                        <owl:onProperty rdf:resource="http://ex.org/w#enrollFor"/>
                        <owl:qualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:qualifiedCardinality>
                        <owl:onClass rdf:resource="http://ex.org/w#UGProgram"/>
                    </owl:Restriction>
                </owl:intersectionOf>
            </owl:Class>
        </owl:equivalentClass>
    </owl:Class>
    <owl:Class rdf:about="http://ex.org/w#PeopleWithManyHobbies">
        <owl:equivalentClass>
            <owl:Class>
                <owl:intersectionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://ex.org/w#Person"/>
                    <owl:Restriction>
                        <owl:onProperty rdf:resource="http://ex.org/w#likes"/>
                        <owl:minQualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">3</owl:minQualifiedCardinality>
                        <owl:onClass rdf:resource="http://ex.org/w#Interest"/>
                    </owl:Restriction>
                </owl:intersectionOf>
            </owl:Class>
        </owl:equivalentClass>
    </owl:Class>
    <owl:NamedIndividual rdf:about="http://ex.org/w#i1"/>
    <owl:NamedIndividual rdf:about="http://ex.org/w#i2"/>
    <owl:NamedIndividual rdf:about="http://ex.org/w#i3"/>
    <owl:NamedIndividual rdf:about="http://ex.org/w#prog1"/>
    <owl:NamedIndividual rdf:about="http://ex.org/w#hobbyA">
        <rdf:type rdf:resource="http://ex.org/w#Interest"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#hobbyB">
        <rdf:type rdf:resource="http://ex.org/w#Interest"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#hobbyC">
        <rdf:type rdf:resource="http://ex.org/w#Interest"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#alice">
        <rdf:type rdf:resource="http://ex.org/w#Person"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#bob">
        <rdf:type rdf:resource="http://ex.org/w#Person"/>
        <voc:likes rdf:resource="http://ex.org/w#hobbyA"/>
        <voc:likes rdf:resource="http://ex.org/w#hobbyB"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#carol">
        <rdf:type rdf:resource="http://ex.org/w#Person"/>
        <voc:likes rdf:resource="http://ex.org/w#hobbyA"/>
        <voc:likes rdf:resource="http://ex.org/w#hobbyB"/>
        <voc:likes rdf:resource="http://ex.org/w#hobbyC"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#student1">
        <voc:enrollFor rdf:resource="http://ex.org/w#prog1"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#prog1">
        <rdf:type rdf:resource="http://ex.org/w#UGProgram"/>
    </owl:NamedIndividual>
    <owl:NamedIndividual rdf:about="http://ex.org/w#dave">
        <rdf:type rdf:resource="http://ex.org/w#Person"/>
        <voc:knows rdf:resource="http://ex.org/w#alice"/>
    </owl:NamedIndividual>
</rdf:RDF>
XML
V2C_DATASET_SHA="$(sha256sum "$V2C_DIR/mini-v2c.owl" | cut -d' ' -f1)"

v2c_derive() {
  local query_file="$1" case_id="$2" profile="$3" out_dir="$4"
  local query_sha
  query_sha="$(sha256sum "$query_file" | cut -d' ' -f1)"
  python3 "$HERE/derived_oracle_pipeline.py" derive \
    --dataset "$V2C_DIR/mini-v2c.owl" --query "$query_file" --output-dir "$out_dir" \
    --case-id "$case_id" --query-id "${case_id%%:*}" --profile "$profile" --track patched \
    --dataset-id "mini-v2c" --dataset-sha256 "$V2C_DATASET_SHA" \
    --source-query-sha256 "$query_sha" --source-corpus "test-corpus" \
    --source-revision "test-revision" --base "http://ex.org/v2c"
}

# 006: knows is reflexive and every declared individual knows itself, so the
# SelfAwarePerson members are exactly the Persons (alice, bob, carol) plus
# dave (Person via knows' domain).
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :SelfAwarePerson }' >"$V2C_DIR/q006.rq"
V2C_Q006="$(v2c_derive "$V2C_DIR/q006.rq" "owl2bench_query_006:DL" DL "$WORK/v2c-oracle")"
[[ "$(echo "$V2C_Q006" | jq -r '.answer_rows')" == "4" ]] \
  || { echo "expected 4 SelfAwarePerson rows (prp-ref + cls-hs + cls-intro), got $(echo "$V2C_Q006" | jq -r '.answer_rows')"; exit 1; }

# 017: student1 (Student via enrollFor's domain) has exactly one qualified
# enrollment; dave has one knows edge but enrollFor is what counts.
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :UGStudent }' >"$V2C_DIR/q017.rq"
V2C_Q017="$(v2c_derive "$V2C_DIR/q017.rq" "owl2bench_query_017:DL" DL "$WORK/v2c-oracle")"
[[ "$(echo "$V2C_Q017" | jq -r '.answer_rows')" == "1" ]] \
  || { echo "expected 1 UGStudent row (cls-qc1 + cls-intro under the guard), got $(echo "$V2C_Q017" | jq -r '.answer_rows')"; exit 1; }

# 018: cls-mqc3 stays in the EL/RL tables (their anchors declare no
# min-qualified restriction, so the rule is inert there but testable on
# fixtures); the DL table drops it — sameAs-merged witnesses make the
# distinct-witness reading unsound on DL data, so DL-patched fails 018
# closed and derives nothing for the class.
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :PeopleWithManyHobbies }' >"$V2C_DIR/q018.rq"
V2C_Q018="$(v2c_derive "$V2C_DIR/q018.rq" "owl2bench_query_018:EL" EL "$WORK/v2c-oracle")"
[[ "$(echo "$V2C_Q018" | jq -r '.answer_rows')" == "1" ]] \
  || { echo "expected 1 PeopleWithManyHobbies row under EL (cls-mqc3 distinct witnesses), got $(echo "$V2C_Q018" | jq -r '.answer_rows')"; exit 1; }
V2C_Q018_DL="$(v2c_derive "$V2C_DIR/q018.rq" "owl2bench_query_018:DL" DL "$WORK/v2c-dl")"
[[ "$(echo "$V2C_Q018_DL" | jq -r '.answer_rows')" == "0" ]] \
  || { echo "DL-patched must not derive PeopleWithManyHobbies (rule dropped), got $(echo "$V2C_Q018_DL" | jq -r '.answer_rows')"; exit 1; }
jq -e '.rule_table_version == "owl2-dl-rule-table/4"' \
  "$WORK/v2c-dl/provenance/owl2bench_query_018:DL.json" >/dev/null

# cls-hvn (#105): a class defined as an intersection containing a hasValue
# restriction derives its members under DL (rule dropped in /3 made this 0).
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :FanI }' >/dev/null 2>&1 || true
python3 - "$V2C_DIR" "$WORK" "$HERE" <<'PY' >/dev/null
import hashlib, json, subprocess, sys, pathlib
v2c, work, here = sys.argv[1], sys.argv[2], sys.argv[3]
text = pathlib.Path(v2c + "/mini-v2c.owl").read_text()
extra = (
    '    <owl:Class rdf:about="http://ex.org/w#FanI">\n'
    '        <owl:equivalentClass>\n'
    '            <owl:Class>\n'
    '                <owl:intersectionOf rdf:parseType="Collection">\n'
    '                    <rdf:Description rdf:about="http://ex.org/w#Person"/>\n'
    '                    <owl:Restriction>\n'
    '                        <owl:onProperty rdf:resource="http://ex.org/w#isCrazyAbout"/>\n'
    '                        <owl:hasValue rdf:resource="http://ex.org/w#T20"/>\n'
    '                    </owl:Restriction>\n'
    '                </owl:intersectionOf>\n'
    '            </owl:Class>\n'
    '        </owl:equivalentClass>\n'
    '    </owl:Class>\n'
    '    <owl:NamedIndividual rdf:about="http://ex.org/w#dave">\n'
    '        <voc:isCrazyAbout rdf:resource="http://ex.org/w#T20"/>\n'
    '    </owl:NamedIndividual>\n'
)
pathlib.Path(v2c + "/mini-v2c-hvn.owl").write_text(text.replace("</rdf:RDF>", extra + "</rdf:RDF>"))
pathlib.Path(v2c + "/hvn.rq").write_text("SELECT DISTINCT ?x WHERE { ?x rdf:type :FanI }")
dataset = v2c + "/mini-v2c-hvn.owl"
query = v2c + "/hvn.rq"
out = subprocess.run([
    "python3", here + "/derived_oracle_pipeline.py", "derive",
    "--dataset", dataset, "--query", query, "--output-dir", work + "/v2c-hvn",
    "--case-id", "owl2bench_query_005:DL", "--query-id", "owl2bench_query_005",
    "--profile", "DL", "--track", "patched",
    "--dataset-id", "mini-v2c-hvn",
    "--dataset-sha256", hashlib.sha256(pathlib.Path(dataset).read_bytes()).hexdigest(),
    "--source-query-sha256", hashlib.sha256(pathlib.Path(query).read_bytes()).hexdigest(),
    "--source-corpus", "test-corpus", "--source-revision", "test-revision",
    "--base", "http://ex.org/v2c",
], capture_output=True, text=True)
assert out.returncode == 0, out.stderr
rows = json.loads(out.stdout)["answer_rows"]
assert rows == 1, f"cls-hvn must derive dave as FanI through the intersection, got {rows}"
PY

V2C_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/v2c-oracle" --base "http://ex.org/v2c")"
echo "$V2C_AUDIT" | jq -e '[.rows[] | select(.verdict == "pass")] | length == 3' >/dev/null

# 017 guard: a second qualified enrollment fails the whole derivation closed,
# and a sameAs-merged witness pair under the min-3 restriction fails it too.
python3 - "$V2C_DIR/mini-v2c.owl" "$V2C_DIR/mini-v2c-violation.owl" <<'PY'
import sys
text = open(sys.argv[1], encoding="utf-8").read()
extra = (
    '    <owl:NamedIndividual rdf:about="http://ex.org/w#student1">\n'
    '        <voc:enrollFor rdf:resource="http://ex.org/w#prog2"/>\n'
    '    </owl:NamedIndividual>\n'
    '    <owl:NamedIndividual rdf:about="http://ex.org/w#prog2">\n'
    '        <rdf:type rdf:resource="http://ex.org/w#UGProgram"/>\n'
    '    </owl:NamedIndividual>\n'
    '    <owl:NamedIndividual rdf:about="http://ex.org/w#hobbyA">\n'
    '        <owl:sameAs rdf:resource="http://ex.org/w#hobbyC"/>\n'
    '    </owl:NamedIndividual>\n'
)
open(sys.argv[2], "w", encoding="utf-8").write(text.replace("</rdf:RDF>", extra + "</rdf:RDF>"))
PY
printf 'SELECT DISTINCT ?x WHERE { ?x rdf:type :UGStudent }' >"$V2C_DIR/q017v.rq"
V2C_VIOLATION_SHA="$(sha256sum "$V2C_DIR/mini-v2c-violation.owl" | cut -d' ' -f1)"
V2C_VIOLATION_QUERY_SHA="$(sha256sum "$V2C_DIR/q017v.rq" | cut -d' ' -f1)"
V2C_GUARD="$(python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2C_DIR/mini-v2c-violation.owl" --query "$V2C_DIR/q017v.rq" --output-dir "$WORK/v2c-guard" \
  --case-id "owl2bench_query_017:EL" --query-id "owl2bench_query_017" --profile EL --track patched \
  --dataset-id "mini-v2c-violation" --dataset-sha256 "$V2C_VIOLATION_SHA" \
  --source-query-sha256 "$V2C_VIOLATION_QUERY_SHA" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2c" || true)"
echo "$V2C_GUARD" | jq -e '.status == "error" and .reason == "exact_cardinality_guard_violated"' >/dev/null \
  || { echo "exact-cardinality guard must fail the derivation closed, got: $V2C_GUARD"; exit 1; }
echo "$V2C_GUARD" | jq -e '[.violations[] | select(startswith("cls-qc1-at-most-violated"))] | length >= 1' >/dev/null \
  || { echo "guard must name the at-most violation"; exit 1; }
echo "$V2C_GUARD" | jq -e '[.violations[] | select(startswith("cls-mqc3-sameAs-witnesses"))] | length >= 1' >/dev/null \
  || { echo "guard must name the sameAs-merged-witness violation"; exit 1; }

# Row budget: chains elide beyond the budget, witness triples stay, audit passes.
python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2C_DIR/mini-v2c.owl" --query "$V2C_DIR/q006.rq" --output-dir "$WORK/v2c-budget" \
  --case-id "owl2bench_query_006:DL" --query-id "owl2bench_query_006" --profile DL --track patched \
  --justification-row-budget 1 \
  --dataset-id "mini-v2c" --dataset-sha256 "$V2C_DATASET_SHA" \
  --source-query-sha256 "$(sha256sum "$V2C_DIR/q006.rq" | cut -d' ' -f1)" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2c" >/dev/null
jq -e '.justification_chains_elided == true and .justification_row_budget == 1' \
  "$WORK/v2c-budget/proof/owl2bench_query_006:DL.json" >/dev/null
jq -e '.rows[0].pattern_matches[0].derivation.rule == "elided:justification-row-budget" and (.rows[0].pattern_matches[0].triple | length == 3)' \
  "$WORK/v2c-budget/proof/owl2bench_query_006:DL.json" >/dev/null
V2C_BUDGET_AUDIT="$(python3 "$HERE/derived_oracle_audit.py" --oracle-dir "$WORK/v2c-budget" --base "http://ex.org/v2c")"
echo "$V2C_BUDGET_AUDIT" | jq -e '.rows[0].verdict == "pass"' >/dev/null

# Closure cache: two derives over one cache dir reproduce byte-identical artifacts.
python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2C_DIR/mini-v2c.owl" --query "$V2C_DIR/q017.rq" --output-dir "$WORK/v2c-cache-a" \
  --case-id "owl2bench_query_017:DL" --query-id "owl2bench_query_017" --profile DL --track patched \
  --closure-cache-dir "$WORK/closure-cache" \
  --dataset-id "mini-v2c" --dataset-sha256 "$V2C_DATASET_SHA" \
  --source-query-sha256 "$(sha256sum "$V2C_DIR/q017.rq" | cut -d' ' -f1)" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2c" >/dev/null
python3 "$HERE/derived_oracle_pipeline.py" derive \
  --dataset "$V2C_DIR/mini-v2c.owl" --query "$V2C_DIR/q017.rq" --output-dir "$WORK/v2c-cache-b" \
  --case-id "owl2bench_query_017:DL" --query-id "owl2bench_query_017" --profile DL --track patched \
  --closure-cache-dir "$WORK/closure-cache" \
  --dataset-id "mini-v2c" --dataset-sha256 "$V2C_DATASET_SHA" \
  --source-query-sha256 "$(sha256sum "$V2C_DIR/q017.rq" | cut -d' ' -f1)" --source-corpus "test-corpus" \
  --source-revision "test-revision" --base "http://ex.org/v2c" >/dev/null
cmp -s "$WORK/v2c-cache-a/proof/owl2bench_query_017:DL.json" "$WORK/v2c-cache-b/proof/owl2bench_query_017:DL.json" \
  || { echo "closure cache must reproduce byte-identical proofs"; exit 1; }
[[ "$(ls "$WORK/closure-cache" | wc -l)" == "1" ]] || { echo "expected exactly one cache entry per dataset+table"; exit 1; }

echo "[test-owl2bench-derived-oracle] ok"
