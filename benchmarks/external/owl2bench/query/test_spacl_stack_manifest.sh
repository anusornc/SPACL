#!/usr/bin/env bash
set -euo pipefail

# Contract tests for the SPACL split-stack declaration (#201).
#
# spacl_stack_manifest.py builds the component and materialization manifests
# the query lane needs for a split-stack SPACL verdict, using only the pinned
# provenance the SPACL adapters already write (.spacl-*/provenance.json,
# handoff.json), the dataset anchor metadata, and the pinned capabilities
# file. The end-to-end leg then runs run.sh in split-stack mode with the real
# SPACL adapters (fake reasoner/backend, no docker) and asserts the run
# summary emits the split-stack declaration the campaign contract validates.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

BUILDER="$SCRIPT_DIR/spacl_stack_manifest.py"
CAPABILITIES="$SCRIPT_DIR/spacl-stack-capabilities.json"

fail() { echo "[test-spacl-stack-manifest][assert] $1" >&2; exit 1; }

[[ -f "$BUILDER" ]] || fail "builder script is missing: $BUILDER"
[[ -f "$CAPABILITIES" ]] || fail "capabilities file is missing: $CAPABILITIES"

STAGED_DIR="$TMP_DIR/staged"
DATA_DIR="$TMP_DIR/data"
STACK_DIR="$TMP_DIR/stack"
RESULTS_ROOT="$TMP_DIR/results"
mkdir -p "$STAGED_DIR" "$STACK_DIR" "$DATA_DIR"/{DL,EL,QL,RL}

# --- Fixture datasets: one distinct tiny ontology per profile ---------------
for profile in DL EL QL RL; do
  cat > "$DATA_DIR/$profile/seed1.owl" <<OWL
<?xml version="1.0"?>
<rdf:RDF xml:base="http://example.org/$profile" xmlns="http://example.org/$profile#"
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:NamedIndividual rdf:about="#individual-$profile"/>
</rdf:RDF>
OWL
done

DATASET_METADATA="$TMP_DIR/dataset-metadata.json"
DATASET_METADATA="$DATASET_METADATA" DATA_DIR="$DATA_DIR" python3 - <<'PY'
import hashlib
import json
import os
import pathlib

data = pathlib.Path(os.environ["DATA_DIR"])
metadata = {}
for profile in ("DL", "EL", "QL", "RL"):
    digest = hashlib.sha256((data / profile / "seed1.owl").read_bytes()).hexdigest()
    metadata[profile] = {
        "dataset_id": f"owl2bench-{profile.lower()}:seed1-u1:sha256-{digest}",
        "dataset_sha256": digest,
    }
pathlib.Path(os.environ["DATASET_METADATA"]).write_text(json.dumps(metadata, indent=2) + "\n")
PY

# --- Fixture query matrix: exactly 22/10/10/18 -------------------------------
QUERY_FILE="$STAGED_DIR/query.rq"
printf 'SELECT DISTINCT ?x WHERE { ?x ?p ?o }\n' > "$QUERY_FILE"
QUERY_MANIFEST="$STAGED_DIR/manifest.tsv"
QUERY_MANIFEST="$QUERY_MANIFEST" QUERY_FILE="$QUERY_FILE" python3 - <<'PY'
import os
import pathlib

query = os.environ["QUERY_FILE"]
profiles = ["DL"] * 22 + ["EL"] * 10 + ["QL"] * 10 + ["RL"] * 18
lines = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for index, profile in enumerate(profiles, start=1):
    lines.append(
        "\t".join(
            [query, f"official#{index:03d}", f"case_{index:03d}", profile, "fixture", "owl2bench-fixture", "fixturerevision1"]
        )
    )
pathlib.Path(os.environ["QUERY_MANIFEST"]).write_text("\n".join(lines) + "\n")
PY

# --- Fake reasoner/backend: the real adapters run unmodified ------------------
cat > "$TMP_DIR/fake-reasoner" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "materialize" ]]
input="$2"; output="$3"
profile="DL"
if [[ "${4:-}" == "--profile" ]]; then profile="${5:-DL}"; fi
{
  printf '<urn:spacl:%s:s> <urn:spacl:p> <urn:spacl:%s:o> .\n' "$profile" "$profile"
  printf '<urn:spacl:%s:s> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:spacl:%s:C> .\n' "$profile" "$profile"
} > "$output"
printf '{"command":"materialize","base_triples":2,"derived_triples":0,"closure_triples":2,"out_of_fragment_axioms":0,"parse_time_ms":1,"materialize_time_ms":1}\n'
ENGINE
chmod +x "$TMP_DIR/fake-reasoner"

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
grep -q "urn:spacl" "$graph" || { echo "graph lacks SPACL triples" >&2; exit 1; }
printf '?x\n<urn:spacl:answer>\n'
BACKEND
chmod +x "$TMP_DIR/fake-backend"

export SPACL_RL_REASONER_BIN="$TMP_DIR/fake-reasoner"
export SPACL_RL_QUERY_BACKEND="$TMP_DIR/fake-backend"

export OWL2BENCH_SPLIT_DL_ENGINE="$SCRIPT_DIR/spacl_dl_engine.sh"
export OWL2BENCH_SPLIT_EL_ENGINE="$SCRIPT_DIR/spacl_el_engine.sh"
export OWL2BENCH_SPLIT_QL_ENGINE="$SCRIPT_DIR/spacl_ql_engine.sh"
export OWL2BENCH_SPLIT_RL_ENGINE="$SCRIPT_DIR/spacl_rl_engine.sh"

# --- Loads: the real adapters write the pinned provenance and handoffs --------
for profile in DL EL QL RL; do
  "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_DIR/$profile" \
    > "$TMP_DIR/load-$profile.output" 2> "$TMP_DIR/load-$profile.diagnostics" \
    || fail "profile $profile load failed: $(cat "$TMP_DIR/load-$profile.diagnostics")"
done

# --- Build: manifests sourced only from pinned provenance ---------------------
python3 "$BUILDER" build \
  --data-dir "$DATA_DIR" \
  --capabilities "$CAPABILITIES" \
  --dataset-metadata "$DATASET_METADATA" \
  --reasoner-bin "$TMP_DIR/fake-reasoner" \
  --reasoner-version "9.9.9-fixture" \
  --generator-revision "fixturerevision1" \
  --timeout-seconds 600 \
  --memory "host" \
  --threads 1 \
  --cpu "host" \
  --disk "host" \
  --network "local-only" \
  --output-dir "$STACK_DIR" \
  || fail "manifest build failed"

COMPONENT_MANIFEST="$STACK_DIR/component.json"
MATERIALIZATION_MANIFEST="$STACK_DIR/materialization.json"
[[ -f "$COMPONENT_MANIFEST" && -f "$MATERIALIZATION_MANIFEST" ]] \
  || fail "builder did not emit both manifests"

IMAGE_DIGEST="$(jq -r '.engine_image_digest' "$CAPABILITIES")"
DL_ANCHOR="$(jq -r '.DL.dataset_sha256' "$DATASET_METADATA")"

# --- Component manifest: identities from pinned provenance --------------------
python3 "$SCRIPT_DIR/split_stack_manifest.py" validate \
  --component-manifest "$COMPONENT_MANIFEST" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/component-validation.json"
jq -e '
  .status == "available"
  and (.profiles | [to_entries[] | .value.status == "available"] | all)
  and ([.profiles[] | .manifest_identity | test("^sha256:[0-9a-f]{64}$")] | all)
  and .profiles.DL.components.query.artifact_digest == $image_digest
  and .profiles.QL.components.query.artifact_digest == $image_digest
  and .profiles.DL.dataset.sha256 == $anchor
  and .profiles.DL.dataset.tbox_sha256 == $anchor
  and .profiles.DL.dataset.abox_sha256 == $anchor
  and .profiles.DL.generator.id == "owl2bench-generator"
  and .profiles.DL.generator.revision == "fixturerevision1"
  and .profiles.DL.generator.seed == 1
  and .profiles.DL.profile_context.directory == ($data + "/DL")
  and .profiles.DL.execution_variant == null
  and .profiles.QL.ruleset.id == "owl2-ql-profile-ruleset"
  and .profiles.RL.ruleset.id == "owl2-rl-profile-ruleset"
  and .profiles.DL.ruleset == {}
' \
  --arg image_digest "$IMAGE_DIGEST" \
  --arg anchor "$DL_ANCHOR" \
  --arg data "$DATA_DIR" \
  "$TMP_DIR/component-validation.json" >/dev/null \
  || fail "component manifest identities violate the pinned provenance"

# --- Materialization manifest: complete graphs from the handoffs --------------
python3 "$SCRIPT_DIR/materialization_manifest.py" validate \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --output "$TMP_DIR/materialization-validation.json"
jq -e '
  .status == "complete"
  and .fatal == false
  and ([.profiles[] | .status == "complete"] | all)
  and ([.profiles[] | .materialization_identity | test("^sha256:[0-9a-f]{64}$")] | all)
  and .profiles.DL.source_ontology.sha256 == $anchor
  and .profiles.DL.input_sha256 == $anchor
  and .profiles.QL.ruleset.id == "owl2-ql-profile-ruleset"
' \
  --arg anchor "$DL_ANCHOR" \
  "$TMP_DIR/materialization-validation.json" >/dev/null \
  || fail "materialization manifest is not the complete four-profile stack"

# --- verify-stack: the cross contract holds ------------------------------------
python3 "$SCRIPT_DIR/materialization_manifest.py" verify-stack \
  --component-manifest "$COMPONENT_MANIFEST" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/stack-validation.json" \
  || fail "verify-stack rejected the SPACL split stack"
jq -e '.status == "complete" and (.errors | length == 0)' \
  "$TMP_DIR/stack-validation.json" >/dev/null \
  || fail "split stack validation is not clean"

# --- End to end: run.sh emits the split-stack declaration ----------------------
env \
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$COMPONENT_MANIFEST" \
  OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  RUN_ID="spacl-stack-split-declaration" \
  "$SCRIPT_DIR/run.sh" >/dev/null \
  || fail "split-stack run failed: $(jq '.errors // .' "$RESULTS_ROOT/spacl-stack-split-declaration/provenance/profile_result_contract.json" 2>/dev/null || true)"

RUN_DIR="$RESULTS_ROOT/spacl-stack-split-declaration"
jq -e '
  .query_count == 60
  and .split_stack_requested == true
  and .component_status_counts == {"available": 60}
  and .materialization_status_counts == {"complete": 60}
  and (.component_identities | length == 4)
  and ([.component_identities[] | .manifest_identity | test("^sha256:[0-9a-f]{64}$")] | all)
  and ([.component_identities[].profile] | sort == ["DL", "EL", "QL", "RL"])
  and (.materialization_identities | length == 4)
  and ([.materialization_identities[] | .materialization_identity | test("^sha256:[0-9a-f]{64}$")] | all)
  and .result_status_counts.oracle_unavailable == 60
  and .engine_provenance_status_counts.available == 60
  and .profiles.DL.split_stack_requested == true
  and .profiles.DL.component_status_counts == {"available": 22}
  and .profiles.DL.materialization_status_counts == {"complete": 22}
  and (.profiles.DL.component_identities | length == 1)
  and (.profiles.DL.component_identities[0].profile == "DL")
  and (.profiles.DL.materialization_identities | length == 1)
  and .profiles.QL.component_status_counts == {"available": 10}
  and .profiles.RL.materialization_status_counts == {"complete": 18}
' "$RUN_DIR/summary.json" >/dev/null \
  || fail "run summary does not emit the split-stack declaration"

jq -e '
  .component_manifest.requested == true
  and .materialization.requested == true
  and .split_stack_materialization.requested == true
  and .split_stack_materialization.status == "complete"
' "$RUN_DIR/run_metadata.json" >/dev/null \
  || fail "run metadata does not declare the split stack"

jq -e '.split_stack_requested == true and .component_status_counts == {"available": 10}' \
  "$RUN_DIR/profiles/EL/summary.json" >/dev/null \
  || fail "generated profile summary does not carry the declaration"

# --- Negative: a mutated dataset anchor must fail the build --------------------
jq '.DL.dataset_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"' \
  "$DATASET_METADATA" > "$TMP_DIR/bad-anchor-metadata.json"
if python3 "$BUILDER" build \
  --data-dir "$DATA_DIR" \
  --capabilities "$CAPABILITIES" \
  --dataset-metadata "$TMP_DIR/bad-anchor-metadata.json" \
  --reasoner-bin "$TMP_DIR/fake-reasoner" \
  --reasoner-version "9.9.9-fixture" \
  --generator-revision "fixturerevision1" \
  --output-dir "$TMP_DIR/bad-anchor-stack" >/dev/null 2>"$TMP_DIR/bad-anchor.stderr"; then
  fail "builder accepted a mismatched dataset anchor"
fi
grep -q "anchor" "$TMP_DIR/bad-anchor.stderr" || fail "anchor-mismatch failure reason is missing"

# --- Negative: a stale handoff graph must fail the build -----------------------
printf '<urn:spacl:stale> <urn:spacl:p> <urn:spacl:stale-o> .\n' >> "$DATA_DIR/DL/.spacl-dl/materialized.nt"
if python3 "$BUILDER" build \
  --data-dir "$DATA_DIR" \
  --capabilities "$CAPABILITIES" \
  --dataset-metadata "$DATASET_METADATA" \
  --reasoner-bin "$TMP_DIR/fake-reasoner" \
  --reasoner-version "9.9.9-fixture" \
  --generator-revision "fixturerevision1" \
  --output-dir "$TMP_DIR/stale-stack" >/dev/null 2>"$TMP_DIR/stale-build.stderr"; then
  fail "builder accepted a stale materialization handoff"
fi
grep -q "handoff" "$TMP_DIR/stale-build.stderr" || fail "stale-handoff failure reason is missing"

# --- Negative: a rewritten reasoner binary must fail the build -----------------
printf '# binary drifted after the load\n' >> "$TMP_DIR/fake-reasoner"
if python3 "$BUILDER" build \
  --data-dir "$DATA_DIR" \
  --capabilities "$CAPABILITIES" \
  --dataset-metadata "$DATASET_METADATA" \
  --reasoner-bin "$TMP_DIR/fake-reasoner" \
  --reasoner-version "9.9.9-fixture" \
  --generator-revision "fixturerevision1" \
  --output-dir "$TMP_DIR/drifted-stack" >/dev/null 2>"$TMP_DIR/drifted.stderr"; then
  fail "builder accepted a drifted reasoner binary"
fi
grep -q "reasoner" "$TMP_DIR/drifted.stderr" || fail "reasoner-drift failure reason is missing"

echo "[test-spacl-stack-manifest] ok"
