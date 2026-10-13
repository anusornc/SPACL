#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPORTER="$SCRIPT_DIR/report.sh"
ATTESTOR="$SCRIPT_DIR/openllet_ql_runtime_identity.py"
IDENTITY="$SCRIPT_DIR/openllet_ql_runtime_identity.json"
ACCEPTANCE_LOCK="$SCRIPT_DIR/openllet_ql_acceptance_lock.json"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

RUN_ROOT="$TMP_DIR/results"
RUN_ID="report-fixture"
RUN_DIR="$RUN_ROOT/$RUN_ID"
mkdir -p "$RUN_DIR"
touch "$RUN_DIR/.owl2bench-query-owned"

cat > "$RUN_DIR/run_metadata.json" <<'JSON'
{
  "schema_version": 1,
  "run_id": "report-fixture",
  "suite": "owl2bench",
  "lane": "query",
  "engine": "/opt/konclude-unified",
  "engine_status": "available",
  "engine_image": "",
  "engine_image_digest": "sha256:engine-image",
  "capabilities_file": "/opt/konclude-unified-capabilities.json",
  "canonicalizer": {
    "name": "owl2bench-query-canonicalizer",
    "version": "owl2bench-query-canonicalizer/2"
  },
  "component_manifest": {"requested": true},
  "materialization": {"requested": true},
  "variant_control": {
    "implementation_completeness": "caller-lie",
    "release_readiness": "caller-lie",
    "execution_availability": "caller-lie",
    "semantic_correctness": "caller-lie"
  }
}
JSON

write_row() {
  local profile="$1"
  local number="$2"
  local case_id="${profile}-case-${number}"
  local source_hash
  local oracle_hash
  local canonical_hash
  local dataset_hash
  local provenance_hash
  local provenance_artifact_hash
  local engine_binary_hash
  local engine_configuration_hash
  source_hash="$(printf '%s' "source/${profile}/${number}" | sha256sum | awk '{print $1}')"
  oracle_hash="$(printf '%s' "oracle/${profile}/${number}" | sha256sum | awk '{print $1}')"
  canonical_hash="sha256:$(printf '%s' "canonical/${profile}/${number}" | sha256sum | awk '{print $1}')"
  dataset_hash="$(printf '%s' "dataset/${profile}" | sha256sum | awk '{print $1}')"
  provenance_hash="$(printf '%s' 'official-answer-key' | sha256sum | awk '{print $1}')"
  provenance_artifact_hash="$(printf '%s' 'official-provenance-artifact' | sha256sum | awk '{print $1}')"
  engine_binary_hash="$(printf '%s' 'konclude-binary' | sha256sum | awk '{print $1}')"
  engine_configuration_hash="$(printf '%s' 'konclude-config' | sha256sum | awk '{print $1}')"
  jq -n \
    --arg profile "$profile" \
    --arg number "$number" \
    --arg case_id "$case_id" \
    --arg source_hash "$source_hash" \
    --arg oracle_hash "$oracle_hash" \
    --arg canonical_hash "$canonical_hash" \
    --arg dataset_hash "$dataset_hash" \
    --arg provenance_hash "$provenance_hash" \
    --arg provenance_artifact_hash "$provenance_artifact_hash" \
    --arg engine_binary_hash "$engine_binary_hash" \
    --arg engine_configuration_hash "$engine_configuration_hash" \
    '{
      schema_version: 1,
      run_id: "report-fixture",
      ontology_suite: "owl2bench-query",
      case_id: $case_id,
      query_id: $case_id,
      query_profile: $profile,
      source_query: ("queries/" + ($profile | ascii_downcase) + "/" + $case_id + ".rq"),
      source_query_sha256: $source_hash,
      source_corpus: "owl2bench-official",
      source_revision: "revision-2026-08-28",
      workload: "official-query-workload",
      engine_detail: "available",
      execution_status: "success",
      result_status: "passed",
      correctness_status: "passed",
      load_status: "success",
      load_time_ms: 11,
      query_time_ms: 22,
      dataset_identity: {
        dataset_id: ("dataset-" + $profile),
        dataset_sha256: ("sha256:" + $dataset_hash),
        source: "fixture"
      },
      engine_provenance: {
        engine: "konclude-unified",
        version: "v0.7.0-1138",
        binary_sha256: ("sha256:" + $engine_binary_hash),
        configuration_sha256: ("sha256:" + $engine_configuration_hash)
      },
      canonicalization: {
        status: "available",
        canonicalizer: "owl2bench-query-canonicalizer",
        canonicalizer_version: "owl2bench-query-canonicalizer/2",
        format: "typed-rdf-results-v1",
        result_hash: $canonical_hash
      },
      semantic_result: $canonical_hash,
      reasoning_result: $canonical_hash,
      component_manifest: {
        profile: $profile,
        status: "available",
        manifest_identity: ("sha256:" + $engine_binary_hash),
        components: {
          engine: {name: "HermiT", version: "1.4.3.517", revision: "engine-revision", artifact_digest: ("sha256:" + $engine_binary_hash)},
          materializer: {name: "profile-materializer", version: "1.0", revision: "materializer-revision", artifact_digest: ("sha256:" + $engine_configuration_hash)},
          query: {name: "profile-query", version: "1.0", revision: "query-revision", artifact_digest: ("sha256:" + $engine_binary_hash)}
        },
        ruleset: (if $profile == "QL" or $profile == "RL" then {id: ("ruleset-" + $profile), version: "1.0", revision: "ruleset-revision", artifact_digest: ("sha256:" + $engine_configuration_hash), conformance_boundary: "fixture"} else {} end),
        configuration: {id: ("config-" + $profile), digest: ("sha256:" + $engine_configuration_hash)},
        runtime: {id: "runtime-fixture", dependencies: [{name: "runtime", version: "1.0", digest: ("sha256:" + $engine_binary_hash)}]},
        credentials: {status: "not_required"},
        license: {status: "not_required"},
        capability: {status: "supported", supports_query: true},
        resource_limits: {timeout_seconds: 600, memory: "24GiB", threads: 1},
        dataset: {id: ("dataset-" + $profile), sha256: ("sha256:" + $dataset_hash), tbox_id: ("tbox-" + $profile), tbox_sha256: ("sha256:" + $dataset_hash), abox_id: ("abox-" + $profile), abox_sha256: ("sha256:" + $dataset_hash)},
        generator: {id: "owl2bench-generator", revision: "generator-revision", seed: 1},
        profile_context: {id: ("context-" + $profile), directory: ("/data/" + $profile)}
      },
      materialization: {
        profile: $profile,
        status: "complete",
        materialization_identity: ("sha256:" + $oracle_hash),
        source_ontology: {id: ("source-" + $profile), sha256: ("sha256:" + $dataset_hash)},
        component: {name: "profile-materializer", version: "1.0", revision: "materializer-revision", artifact_digest: ("sha256:" + $engine_configuration_hash)},
        configuration: {id: ("materialization-config-" + $profile), sha256: ("sha256:" + $engine_configuration_hash)},
        output_graph: {id: ("closure-" + $profile), sha256: ("sha256:" + $dataset_hash)},
        derivation: {version: "materialization-1.0", revision: "materialization-revision"},
        resource_limits: {timeout_seconds: 600, memory: "24GiB", threads: 1},
        input_sha256: ("sha256:" + $dataset_hash),
        output_sha256: ("sha256:" + $dataset_hash),
        profile_context: {id: ("context-" + $profile), directory: ("/data/" + $profile)}
      },
      oracle: {
        status: "available",
        class: "official",
        oracle_id: ("official:" + $case_id),
        provenance: "official-answer-key",
        provenance_sha256: $provenance_hash,
        provenance_artifact: "oracle/provenance/official.json",
        provenance_artifact_sha256: $provenance_artifact_hash,
        artifact_sha256: $oracle_hash,
        canonical_result_hash: $canonical_hash,
        match_reason: "accepted"
      },
      profile_context: {
        profile: $profile,
        identity: ("profile=" + $profile + ";directory=/data/" + $profile),
        status: "available",
        load_status: "success"
      }
    }' > "$RUN_DIR/${case_id}.json"
}

for profile_count in "DL:22" "EL:10" "QL:10" "RL:18"; do
  profile="${profile_count%%:*}"
  count="${profile_count##*:}"
  for number in $(seq -w 1 "$count"); do
    write_row "$profile" "$number"
  done
done

OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '
  .variant_control.status == "not_available"
  and .variant_control.revision_id == "not_available"
  and .variant_control.category_sha256 == "not_available"
  and .variant_control.implementation_completeness == "not_available"
  and .variant_control.release_readiness == "not_available"
  and .variant_control.execution_availability == "not_available"
  and .variant_control.semantic_correctness == "not_available"
' "$RUN_DIR/summary.json" >/dev/null

python3 - "$TMP_DIR" "$IDENTITY" "$ATTESTOR" "$ACCEPTANCE_LOCK" "$RUN_DIR" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

tmp = pathlib.Path(sys.argv[1])
identity = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
attestor = pathlib.Path(sys.argv[3])
acceptance = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
workload = acceptance["query_workload"]
run_dir = pathlib.Path(sys.argv[5])
graph_sha256 = "sha256:" + "9" * 64

def write_json(name, value):
    path = tmp / name
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

def write_run_json(name, value):
    path = run_dir / name
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path

diagnostic = write_json("diagnostic.json", {
    "status": "complete",
    "independent_obligations": {
        "expected_inferred_count": 2,
        "expected_inferred_assertion_counts": {"class": 1, "object_property": 1},
    },
    "observations": {
        "class": {"status": "observed"},
        "object_property": {"status": "observed"},
    },
})
runtime_attestation = write_json("runtime-attestation.json", {
    "status": "available",
    "component": identity["component"],
    "runtime": identity["runtime"],
    "classpath": identity["classpath"],
    "inputs": identity["inputs"],
    "resource_limits": identity["resource_limits"],
})
component_manifest = write_json("component-manifest.json", {
    "schema_version": 1,
    "profiles": {
        "QL": {
            "execution_variant": "openllet-ql",
            "components": {
                "engine": {
                    "name": "Openllet",
                    "version": identity["component"]["openllet_version"],
                }
            },
            "configuration": {
                "digest": identity["inputs"]["configuration"]["sha256"]
            },
            "ruleset": {
                "revision": identity["runtime"]["variant_revision"],
                "artifact_digest": identity["runtime"]["image"]["digest"],
            },
            "runtime": {
                "variant_revision": identity["runtime"]["variant_revision"],
                "base_image": identity["runtime"]["base_image"],
                "image": identity["runtime"]["image"],
                "dependencies": identity["runtime"]["dependencies"],
                "classpath": {
                    "lock_sha256": identity["classpath"]["lock_sha256"],
                    "jar_count": identity["classpath"]["jar_count"],
                },
                "attestation": {
                    "path": str(runtime_attestation),
                    "sha256": digest(runtime_attestation),
                },
            },
        }
    },
})
materialization = write_json("materialization.json", {
    "status": "complete",
    "execution_variant": "openllet-ql",
    "official": {
        "execution_status": "complete",
        "status": "oracle_unavailable",
        "canonical_graph": {"sha256": graph_sha256},
    },
    "diagnostic": json.loads(diagnostic.read_text(encoding="utf-8")),
    "component_manifest": {
        "path": str(component_manifest),
        "sha256": digest(component_manifest),
    },
    "runtime_attestation": {
        "path": str(runtime_attestation),
        "sha256": digest(runtime_attestation),
    },
})
result_contract = write_run_json("variant-result-contract.evidence", {
    "status": "valid",
    "errors": [],
    "validator": "owl2bench-profile-result-contract/1",
    "run_dir": str(run_dir),
    "result_count": 60,
    "result_status_counts": {"not_available": 50, "oracle_unavailable": 10},
})
ql_results = []
for index, query in enumerate(workload["queries"]):
    path = write_run_json(f"variant-ql-{index:02d}.evidence", {
        "case_id": f"{query['id']}__QL",
        "query_id": query["id"],
        "query_profile": "QL",
        "source_query_sha256": query["sha256"].removeprefix("sha256:"),
        "source_corpus": workload["source_corpus"],
        "source_revision": workload["source_revision"],
        "execution_status": "success",
        "result_status": "oracle_unavailable",
        "engine_provenance": {"adapter": "openllet-ql"},
        "component_manifest": {
            "path": str(component_manifest),
            "sha256": digest(component_manifest).removeprefix("sha256:"),
            "execution_variant": "openllet-ql",
            "components": {
                "engine": {
                    "name": "Openllet",
                    "version": identity["component"]["openllet_version"],
                }
            },
            "configuration": {
                "digest": identity["inputs"]["configuration"]["sha256"]
            },
            "ruleset": {
                "revision": identity["runtime"]["variant_revision"],
                "artifact_digest": identity["runtime"]["image"]["digest"],
            },
            "runtime": {
                "image": identity["runtime"]["image"],
                "dependencies": identity["runtime"]["dependencies"],
            },
        },
        "materialization": {
            "execution_variant": "openllet-ql",
            "output_graph": {"sha256": graph_sha256},
        },
        "canonicalization": {"status": "available"},
        "query_time_ms": index,
    })
    ql_results.append({"path": str(path), "sha256": digest(path)})
manifest = write_json("evidence-manifest.json", {
    "revision_id": identity["variant_control"]["revision_id"],
    "category_sha256": identity["variant_control"]["category_sha256"],
    "diagnostic": {"path": str(diagnostic), "sha256": digest(diagnostic)},
    "runtime_attestation": {
        "path": str(runtime_attestation),
        "sha256": digest(runtime_attestation),
    },
    "component_manifest": {
        "path": str(component_manifest),
        "sha256": digest(component_manifest),
    },
    "materialization": {"path": str(materialization), "sha256": digest(materialization)},
    "result_contract": {"path": str(result_contract), "sha256": digest(result_contract)},
    "ql_results": ql_results,
})
validation = tmp / "variant-control-preflight.json"
status = subprocess.run([
    "python3",
    str(attestor),
    "validate-variant",
    "--identity",
    sys.argv[2],
    "--evidence-manifest",
    str(manifest),
    "--output",
    str(validation),
], check=False).returncode
preflight = json.loads(validation.read_text(encoding="utf-8"))
if status != 1 or preflight["evidence"]["core_status"] != "complete":
    raise SystemExit(validation.read_text(encoding="utf-8"))
if "report_markdown.path_missing" not in preflight["evidence"]["reasons"]:
    raise SystemExit(validation.read_text(encoding="utf-8"))
(tmp / "variant-control-preflight.path").write_text(str(validation), encoding="utf-8")
(tmp / "evidence-manifest.path").write_text(str(manifest), encoding="utf-8")
PY

PREFLIGHT_CONTROL_FILE="$(<"$TMP_DIR/variant-control-preflight.path")"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_VARIANT_CONTROL_FILE="$PREFLIGHT_CONTROL_FILE" \
  "$REPORTER" "$RUN_ID" >/dev/null

report_before="$(sha256sum "$RUN_DIR/report.md" | awk '{print $1}')"
EVIDENCE_MANIFEST="$(<"$TMP_DIR/evidence-manifest.path")"
report_sha256="sha256:$report_before"
jq --arg path "$RUN_DIR/report.md" --arg sha256 "$report_sha256" \
  '.report_markdown = {path: $path, sha256: $sha256}' \
  "$EVIDENCE_MANIFEST" > "$TMP_DIR/evidence-manifest-final.json"
mv "$TMP_DIR/evidence-manifest-final.json" "$EVIDENCE_MANIFEST"

VARIANT_CONTROL_FILE="$TMP_DIR/variant-control-validation.json"
python3 "$ATTESTOR" validate-variant \
  --identity "$IDENTITY" \
  --evidence-manifest "$EVIDENCE_MANIFEST" \
  --output "$VARIANT_CONTROL_FILE"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_VARIANT_CONTROL_FILE="$VARIANT_CONTROL_FILE" \
  "$REPORTER" "$RUN_ID" >/dev/null
test "$(sha256sum "$RUN_DIR/report.md" | awk '{print $1}')" = "$report_before"

test "$(find "$RUN_DIR"/profiles -type f -name report.md | wc -l)" -eq 4
test -f "$RUN_DIR/report.md"
jq -e '
  .query_count == 60
  and .expected_query_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .split_stack_requested == true
  and .component_status_counts.available == 60
  and .materialization_status_counts.complete == 60
  and .profile_gates == {DL:"passed",EL:"passed",QL:"passed",RL:"passed"}
  and .query_gate == "passed"
  and .correctness_gate == "passed"
  and .performance_claims_allowed == true
  and .variant_control.implementation_completeness == "complete"
  and .variant_control.release_readiness == "blocked"
  and .variant_control.execution_availability == "complete"
  and .variant_control.semantic_correctness == "blocked"
  and .variant_control.revision_id == "openllet-ql/revision-1"
  and (.variant_control.category_sha256 | startswith("sha256:"))
  and .variant_control.official_status_counts == {oracle_unavailable:10}
  and .variant_control.sha256
  and .variant_control.validation_file == $validation_file
' --arg validation_file "$VARIANT_CONTROL_FILE" "$RUN_DIR/summary.json" >/dev/null

OTHER_RUN_ID="report-fixture-foreign-lineage"
OTHER_RUN_DIR="$RUN_ROOT/$OTHER_RUN_ID"
cp -R "$RUN_DIR" "$OTHER_RUN_DIR"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_VARIANT_CONTROL_FILE="$VARIANT_CONTROL_FILE" \
  "$REPORTER" "$OTHER_RUN_ID" >/dev/null
jq -e '
  .variant_control.status == "failed"
  and .variant_control.reason == "variant_control_run_lineage_mismatch"
  and .variant_control.revision_id == "not_available"
  and .variant_control.category_sha256 == "not_available"
' "$OTHER_RUN_DIR/summary.json" >/dev/null

for profile in DL EL QL RL; do
  jq -e --arg profile "$profile" '
    .profile == $profile
    and .correctness_gate == "passed"
    and (.source_hashes | length) > 0
    and (.oracle_hashes | length) > 0
    and .result_status_counts.passed > 0
    and .oracle_status_counts.available > 0
    and .split_stack_requested == true
    and .component_status_counts.available == .expected_case_count
    and .materialization_status_counts.complete == .expected_case_count
    and (.component_identities | length) > 0
    and (.materialization_identities | length) > 0
    and .timing_provenance.load.recorded_count == .expected_case_count
    and .timing_provenance.query.recorded_count == .expected_case_count
  ' "$RUN_DIR/profiles/$profile/summary.json" >/dev/null
  expected_count="$(jq -r --arg profile "$profile" '.expected_case_count' "$RUN_DIR/profiles/$profile/summary.json")"
  test "$(grep -c "^| ${profile}-case-" "$RUN_DIR/profiles/$profile/report.md")" -eq "$expected_count"
  jq -e --argjson expected "$expected_count" \
    '.execution_status_counts.success == $expected and .engine_provenance_status_counts.available == $expected' \
    "$RUN_DIR/profiles/$profile/summary.json" >/dev/null
  grep -F '| Execution status | Count |' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
  grep -F '| Engine provenance status | Count |' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
  grep -F 'Source hash' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
  grep -F 'Oracle artifact hash' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
  grep -F 'Oracle provenance hash' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
  grep -F 'Materialization identities' "$RUN_DIR/profiles/$profile/report.md" >/dev/null
done
grep -F 'DL **passed** AND EL **passed** AND QL **passed** AND RL **passed**' \
  "$RUN_DIR/report.md" >/dev/null
grep -F 'Materialization status counts' "$RUN_DIR/report.md" >/dev/null
grep -F 'Engine provenance status counts' "$RUN_DIR/report.md" >/dev/null
grep -F 'Implementation completeness: **complete**' "$RUN_DIR/report.md" >/dev/null
grep -F 'Release readiness: **blocked**' "$RUN_DIR/report.md" >/dev/null
grep -F 'Execution availability: **complete**' "$RUN_DIR/report.md" >/dev/null
grep -F 'Semantic correctness: **blocked**' "$RUN_DIR/report.md" >/dev/null

# Report projections must not copy credential or extension fields from result
# records into generated summaries or Markdown artifacts.
jq '
  .component_manifest.credentials.token = "secret-token"
  | .component_manifest.license.private_key = "secret-license"
  | .component_manifest.runtime.dependencies[0].password = "secret-password"
  | .materialization.source_ontology.secret = "secret-materialization"
' "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/secret.json"
mv "$TMP_DIR/secret.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
if grep -R -E 'secret-token|secret-license|secret-password|secret-materialization' \
  "$RUN_DIR/summary.json" "$RUN_DIR/report.md" "$RUN_DIR/profiles" >/dev/null; then
  echo "assertion failed: secret-bearing extension leaked into report artifacts" >&2
  exit 1
fi

# A stale result_status must not bypass the explicit oracle gate.
jq '.oracle.status = "unavailable"' "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
  "$REPORTER" "$RUN_ID" >/dev/null
jq -e '
  .profile_gates.DL == "blocked"
  and .profile_gates.EL == "passed"
  and .query_gate == "blocked"
  and .correctness_gate == "blocked"
  and .performance_claims_allowed == false
' "$RUN_DIR/summary.json" >/dev/null
jq -e '.oracle_status_counts.unavailable == 1 and .correctness_gate == "blocked"' \
  "$RUN_DIR/profiles/DL/summary.json" >/dev/null

# A non-accepted oracle row and a semantic/oracle hash mismatch are both
# blocked even when the stale status fields still say passed.
jq '.oracle.status = "available" | .oracle.match_reason = "not_accepted"' \
  "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '.profiles.DL.gate_blocker_counts["oracle_match:not_accepted"] == 1 and .query_gate == "blocked"' \
  "$RUN_DIR/summary.json" >/dev/null

jq '.oracle.match_reason = "accepted" | .semantic_result = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '.profiles.DL.gate_blocker_counts.semantic_result_not_equal_to_oracle == 1 and .query_gate == "blocked"' \
  "$RUN_DIR/summary.json" >/dev/null

# Derived rows require the same accepted evidence contract as oracle_lookup.
jq '.semantic_result = .canonicalization.result_hash | .oracle.class = "derived" | del(.oracle.audit_status, .oracle.derivation_deterministic, .oracle.sound_complete, .oracle.oracle_pipeline_version, .oracle.proof_artifact, .oracle.proof_artifact_sha256, .oracle.audit_artifact, .oracle.audit_artifact_sha256)' \
  "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '.profiles.DL.gate_blocker_counts.derived_oracle_not_independently_audited == 1 and .profiles.DL.gate_blocker_counts.derived_oracle_proof_missing == 1 and .query_gate == "blocked"' \
  "$RUN_DIR/summary.json" >/dev/null

# Timing provenance is required for every profile row (with explicit
# load-stage not_applicable allowed only for query-file-only engines).
jq 'del(.load_time_ms)' "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '.profiles.DL.gate_blocker_counts.load_time_ms_missing_or_invalid == 1 and .query_gate == "blocked"' \
  "$RUN_DIR/summary.json" >/dev/null

# A readable path cannot substitute for the persisted source hash contract.
jq 'del(.source_query_sha256)' "$RUN_DIR/EL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/EL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '.profiles.EL.gate_blocker_counts.source_query_hash_missing_or_invalid == 1 and .query_gate == "blocked"' \
  "$RUN_DIR/summary.json" >/dev/null

# Split-stack component and materialization status must remain visible in the
# profile gate; a stale passed row cannot hide either blocker.
jq '.component_manifest.status = "not_available"' \
  "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '
  .profiles.DL.gate_blocker_counts["component_status:not_available"] == 1
  and .profiles.DL.component_status_counts.not_available == 1
  and .query_gate == "blocked"
' "$RUN_DIR/summary.json" >/dev/null

jq '.component_manifest.status = "available" | .materialization.status = "partial"' \
  "$RUN_DIR/DL-case-01.json" > "$TMP_DIR/blocked.json"
mv "$TMP_DIR/blocked.json" "$RUN_DIR/DL-case-01.json"
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" "$REPORTER" "$RUN_ID" >/dev/null
jq -e '
  .profiles.DL.gate_blocker_counts["materialization_status:partial"] == 1
  and .profiles.DL.materialization_status_counts.partial == 1
  and .query_gate == "blocked"
' "$RUN_DIR/summary.json" >/dev/null

echo "owl2bench query report tests passed"
