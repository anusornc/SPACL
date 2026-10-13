#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.nq"
PROFILE_IDENTITY="$TMP_DIR/profile-identity.json"
QUERY_DIR="$TMP_DIR/queries"
IDENTITY="$TMP_DIR/graphdb-rl-identity.json"
RUN_DIR="$TMP_DIR/run"
mkdir -p "$QUERY_DIR" "$TMP_DIR/profile/RL" "$TMP_DIR/generator"

python3 - "$TMP_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
source = root / "source.nq"
source.write_text(
    "\n".join(
        [
            "<https://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Person> .",
            "<https://example.test/alice> <https://example.test/name> \"Alice\" .",
            "<https://example.test/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.test/Agent> .",
        ]
    )
    + "\n",
    encoding="utf-8",
)
generator = root / "generator" / "OWL2Bench.jar"
generator_config = root / "generator" / "config.properties"
tbox = root / "profile" / "RL" / "profile-tbox.owl"
abox = root / "profile" / "RL" / "abox.nq"
for path, text in (
    (generator, "generator\n"),
    (generator_config, "seed=1\n"),
    (tbox, "tbox\n"),
    (abox, "abox\n"),
):
    path.write_text(text, encoding="utf-8")

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

queries = root / "queries"
for index in range(1, 19):
    (queries / f"query-{index:03d}.rq").write_text(
        "PREFIX ex: <https://example.test/>\nSELECT ?x WHERE { ?x a ex:Person }\n",
        encoding="utf-8",
    )

source_hash = digest(source)
profile_identity = {
    "schema_version": 1,
    "profile": "RL",
    "status": "complete",
    "conversion_artifact": {"path": str(source), "sha256": source_hash},
    "dataset": {
        "id": "owl2bench:RL:universities=1:seed=1",
        "profile": "RL",
        "sha256": source_hash,
        "source_ontology_sha256": source_hash,
        "artifact_id": "profile-source-nquads:test",
        "artifact_path": str(source),
        "artifact_revision": "721cb0c",
        "tbox_id": "owl2bench-tbox:RL",
        "tbox_sha256": digest(tbox),
        "abox_id": "owl2bench-abox:RL",
        "abox_sha256": digest(abox),
        "seed": 1,
    },
    "dataset_artifact": {
        "id": "profile-source-nquads:test",
        "path": str(source),
        "revision": "721cb0c",
        "sha256": source_hash,
    },
    "generator": {
        "id": "OWL2Bench-generator",
        "path": str(generator),
        "revision": "721cb0c",
        "seed": 1,
        "artifact_path": str(generator),
        "artifact_digest": digest(generator),
    },
    "generator_configuration": {
        "id": "OWL2Bench-generator-config",
        "path": str(generator_config),
        "revision": "721cb0c",
        "sha256": digest(generator_config),
    },
    "profile_tbox": {
        "id": "owl2bench-tbox:RL",
        "path": str(tbox),
        "revision": "721cb0c",
        "sha256": digest(tbox),
    },
    "profile_abox": {
        "id": "owl2bench-abox:RL",
        "path": str(abox),
        "revision": "721cb0c",
        "sha256": digest(abox),
    },
    "profile_context": {
        "id": "owl2bench-context:RL:universities=1:seed=1",
        "directory": str(root / "profile" / "RL"),
    },
    "source_ontology": {
        "id": "generated-dataset:test",
        "path": str(source),
        "revision": "721cb0c",
        "sha256": source_hash,
    },
}
(root / "profile-identity.json").write_text(
    json.dumps(profile_identity, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

python3 "$SCRIPT_DIR/graphdb_rl_identity.py" build \
  --profile-identity "$PROFILE_IDENTITY" \
  --source "$SOURCE" \
  --query-dir "$QUERY_DIR" \
  --output "$IDENTITY"

jq -e '
  .profile == "RL"
  and .component.implementation == "GraphDB"
  and .component.version == "11.5.0"
  and .component.container_image.platform == "linux/amd64"
  and .component.container_image.digest == "sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
  and .configuration.repository_id == "owl2bench-graphdb-rl"
  and .configuration.ruleset == "owl2-rl"
  and .ruleset.id == "graphdb-11.5.0-built-in-owl2-rl"
  and (.ruleset.conformance_boundary | contains("differential execution evidence only"))
  and .query_workload.query_count == 18
  and (.query_workload.queries | length) == 18
  and .resource_limits.timeout_seconds == 600
  and .resource_limits.memory == "24GiB"
  and .resource_limits.threads == 1
  and .resource_limits.variant == "owl2bench-comparable-baseline"
  and .license.status == "not_available"
  and (.license.digest | not)
  and (.oracle_policy | contains("oracle_unavailable"))
  and ((tostring | contains("9.0.0")) | not)
  and ((has("oracle")) | not)
  and ((has("answer_key")) | not)
' "$IDENTITY" >/dev/null

WRONG_SOURCE="$TMP_DIR/wrong-source.nq"
printf '%s\n' '<https://example.test/wrong> <https://example.test/p> <https://example.test/value> .' > "$WRONG_SOURCE"
set +e
python3 "$SCRIPT_DIR/graphdb_rl_identity.py" build \
  --profile-identity "$PROFILE_IDENTITY" \
  --source "$WRONG_SOURCE" \
  --query-dir "$QUERY_DIR" \
  --output "$TMP_DIR/wrong-identity.json" > "$TMP_DIR/wrong-identity.out" 2> "$TMP_DIR/wrong-identity.err"
wrong_identity_status="$?"
set -e
test "$wrong_identity_status" = 1
grep -Fq 'source path does not match the frozen conversion artifact' "$TMP_DIR/wrong-identity.err"

python3 "$SCRIPT_DIR/graphdb_rl_identity.py" build \
  --profile-identity "$PROFILE_IDENTITY" \
  --source "$SOURCE" \
  --query-dir "$QUERY_DIR" \
  --threads 2 --cpu 2 \
  --output "$TMP_DIR/custom-identity-one.json"
python3 "$SCRIPT_DIR/graphdb_rl_identity.py" build \
  --profile-identity "$PROFILE_IDENTITY" \
  --source "$SOURCE" \
  --query-dir "$QUERY_DIR" \
  --threads 3 --cpu 2 \
  --output "$TMP_DIR/custom-identity-two.json"
jq -e '.resource_limits.variant | startswith("owl2bench-custom-")' \
  "$TMP_DIR/custom-identity-one.json" "$TMP_DIR/custom-identity-two.json" >/dev/null
test "$(jq -r '.resource_limits.variant' "$TMP_DIR/custom-identity-one.json")" != \
  "$(jq -r '.resource_limits.variant' "$TMP_DIR/custom-identity-two.json")"

if GRAPHDB_RL_DOCKER_BIN="$TMP_DIR/missing-docker" "$SCRIPT_DIR/graphdb_rl_materializer.sh" version \
  >"$TMP_DIR/version.stdout" 2>"$TMP_DIR/version.stderr"; then
  echo "assertion failed: missing Docker runtime was accepted" >&2
  exit 1
else
  test "$?" = 125
fi
grep -Fq '[graphdb-rl][not_available]' "$TMP_DIR/version.stderr"

cat > "$TMP_DIR/license-docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  printf '%s\n' 'sha256:config|amd64|linux|["ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"]'
  exit 0
fi
printf '%s\n' 'No license was set' >&2
exit 1
EOF
chmod +x "$TMP_DIR/license-docker"
set +e
GRAPHDB_RL_DOCKER_BIN="$TMP_DIR/license-docker" "$SCRIPT_DIR/graphdb_rl_materializer.sh" export "$SOURCE" \
  >"$TMP_DIR/license.stdout" 2>"$TMP_DIR/license.stderr"
license_status="$?"
set -e
test "$license_status" = 125
grep -Fq 'GraphDB runtime license or authorization is unavailable' "$TMP_DIR/license.stderr"

cat > "$TMP_DIR/failed-docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  printf '%s\n' 'sha256:config|amd64|linux|["ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"]'
  exit 0
fi
printf '%s\n' 'repository import failed' >&2
exit 7
EOF
chmod +x "$TMP_DIR/failed-docker"
set +e
GRAPHDB_RL_DOCKER_BIN="$TMP_DIR/failed-docker" "$SCRIPT_DIR/graphdb_rl_materializer.sh" export "$SOURCE" \
  >"$TMP_DIR/failed.stdout" 2>"$TMP_DIR/failed.stderr"
failed_status="$?"
set -e
test "$failed_status" = 1
grep -Fq '[graphdb-rl][failed]' "$TMP_DIR/failed.stderr"

cat > "$TMP_DIR/timeout-docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  printf '%s\n' 'sha256:config|amd64|linux|["ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"]'
  exit 0
fi
sleep 2
EOF
chmod +x "$TMP_DIR/timeout-docker"
set +e
GRAPHDB_RL_DOCKER_BIN="$TMP_DIR/timeout-docker" GRAPHDB_RL_TIMEOUT_SECONDS=1 \
  "$SCRIPT_DIR/graphdb_rl_materializer.sh" export "$SOURCE" \
  >"$TMP_DIR/timeout.stdout" 2>"$TMP_DIR/timeout.stderr"
timeout_status="$?"
set -e
test "$timeout_status" = 124
grep -Fq '[graphdb-rl][timeout]' "$TMP_DIR/timeout.stderr"

if GRAPHDB_RL_DOCKER_BIN="$TMP_DIR/missing-docker" python3 "$SCRIPT_DIR/profile_materialization.py" run \
  --profile RL \
  --source "$SOURCE" \
  --exporter "$SCRIPT_DIR/graphdb_rl_materializer.sh" \
  --identity "$IDENTITY" \
  --output-dir "$RUN_DIR" >"$TMP_DIR/profile.stdout"; then
  echo "assertion failed: unavailable GraphDB runtime completed materialization" >&2
  exit 1
else
  test "$?" = 1
fi

jq -e '
  .profile == "RL"
  and .execution_status == "not_available"
  and .status == "not_available"
  and .pipeline == "owl2bench-rl-materialization/1"
' "$TMP_DIR/profile.stdout" >/dev/null
jq -e '
  .profiles.RL.status == "not_available"
  and (.profiles.RL.reason | contains("unavailable"))
  and ((has("oracle")) | not)
  and ((has("answer_key")) | not)
' "$RUN_DIR/materialization.json" >/dev/null

echo "owl2bench GraphDB RL boundary tests passed"
