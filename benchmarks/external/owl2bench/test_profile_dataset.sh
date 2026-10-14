#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FREEZER="$SCRIPT_DIR/profile_dataset.py"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-profile-dataset-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE_DIR="$TMP_DIR/source"
PACKAGE_ONE="$TMP_DIR/package-one"
PACKAGE_TWO="$TMP_DIR/package-two"
mkdir -p "$SOURCE_DIR/OWL2Bench"
printf '# test source\n' > "$SOURCE_DIR/README.md"
printf 'test-revision\n' > "$SOURCE_DIR/.git-revision"
for profile in DL EL QL RL; do
  printf '<rdf:RDF/>\n' > "$SOURCE_DIR/UNIV-BENCH-OWL2${profile}.owl"
done
printf 'not-a-real-generator\n' > "$SOURCE_DIR/OWL2Bench.jar"
printf 'ontologyFormat=rdf\n' > "$SOURCE_DIR/config.properties"
printf 'random\n' > "$SOURCE_DIR/RandomNames.xlsx"

# Validate a complete package contract without invoking the multi-gigabyte
# official generator. The fixture is assembled through the public manifest
# format and uses real file hashes, then the validator checks every identity.
python3 - "$PACKAGE_ONE" "$SOURCE_DIR" "$SCRIPT_DIR" <<'PY'
import hashlib
import importlib.util
import json
import pathlib
import sys

package = pathlib.Path(sys.argv[1])
source = pathlib.Path(sys.argv[2])
script_dir = pathlib.Path(sys.argv[3])
sys.path.insert(0, str(script_dir))
spec = importlib.util.spec_from_file_location("profile_dataset", script_dir / "profile_dataset.py")
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)
package.mkdir(parents=True)
(package / ".owl2bench-profile-dataset-owned").write_text("owned\n")

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

def artifact(path, role):
    return {"id": f"{role}:{digest(path)}", "role": role, "path": str(path), "sha256": digest(path), "bytes": path.stat().st_size}

profiles = {}
for profile in ("DL", "EL", "QL", "RL"):
    directory = package / "profiles" / profile
    directory.mkdir(parents=True)
    dataset = directory / "dataset.owl"
    tbox = directory / "profile-tbox.owl"
    abox = directory / "abox.nq"
    source_nq = directory / "source.nq"
    diagnostics = directory / "generator.log"
    for path, content in ((dataset, b"dataset\n"), (tbox, b"tbox\n"), (abox, b"<a> <b> <c> .\n"), (source_nq, b"<a> <b> <c> .\n"), (diagnostics, b"ok\n")):
        path.write_bytes(content)
    source_record = artifact(source_nq, "profile-source-nquads")
    profile_record = {
        "profile": profile,
        "status": "complete",
        "source_ontology": artifact(dataset, "generated-dataset"),
        "profile_tbox": artifact(tbox, "profile-tbox"),
        "profile_abox": artifact(abox, "profile-abox"),
        "generated_dataset": artifact(dataset, "generated-dataset"),
        "conversion_artifact": source_record,
        "dataset_artifact": source_record,
        "diagnostics": artifact(diagnostics, "generator-diagnostics"),
        "dataset": {
            "id": f"dataset-{profile}", "profile": profile,
            "artifact_id": source_record["id"], "artifact_path": source_record["path"],
            "artifact_revision": "test-revision", "sha256": source_record["sha256"],
            "source_ontology_sha256": digest(dataset), "tbox_id": f"tbox-{profile}",
            "tbox_sha256": digest(tbox), "abox_id": f"abox-{profile}",
            "abox_sha256": digest(abox),
        },
        "profile_context": {"id": f"context-{profile}", "directory": str(directory)},
    }
    profile_record["identity_sha256"] = module.json_digest(module.stable_profile_identity(profile_record))
    profiles[profile] = profile_record

def generator_artifact(path, role):
    return artifact(path, role)

generator = source / "OWL2Bench.jar"
config = source / "config.properties"
random_names = source / "RandomNames.xlsx"
manifest = {
    "schema_version": 1,
    "manifest_id": "test-package",
    "status": "complete",
    "source": {
        "revision": "test-revision",
        "identity_sha256": "source",
        "artifacts": {
            f"tbox_{profile}": artifact(source / f"UNIV-BENCH-OWL2{profile}.owl", "official-source")
            for profile in ("DL", "EL", "QL", "RL")
        },
    },
    "generator": {
        "revision": "test-revision",
        "artifact_digest": digest(generator),
        "artifacts": {
            name: artifact(source / name, "generator-input")
            for name in ("OWL2Bench.jar", "config.properties", "RandomNames.xlsx")
        },
    },
    "resource_limits": {"timeout_seconds": 600, "memory": "24GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "disabled-after-source-acquisition", "variant": "owl2bench-comparable-baseline"},
    "profiles": profiles,
    "no_answer_keys": True,
}
(package / "dataset-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY

python3 "$FREEZER" validate --manifest "$PACKAGE_ONE/dataset-manifest.json" >/dev/null
python3 "$FREEZER" validate --manifest "$PACKAGE_ONE/dataset-manifest.json" >/dev/null

mkdir -p "$PACKAGE_TWO"
cp "$PACKAGE_ONE/dataset-manifest.json" "$PACKAGE_TWO/dataset-manifest.json"
python3 "$FREEZER" compare \
  --first "$PACKAGE_ONE/dataset-manifest.json" \
  --second "$PACKAGE_TWO/dataset-manifest.json" \
  --output "$TMP_DIR/reproducibility.json" >/dev/null
jq -e '.status == "complete" and (.differences | length) == 0' \
  "$TMP_DIR/reproducibility.json" >/dev/null

jq '.profiles.EL.identity_sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$PACKAGE_TWO/dataset-manifest.json" > "$TMP_DIR/changed-manifest.json"
if python3 "$FREEZER" compare \
  --first "$PACKAGE_ONE/dataset-manifest.json" \
  --second "$TMP_DIR/changed-manifest.json" \
  --output "$TMP_DIR/changed-reproducibility.json" >/dev/null; then
  echo "changed package unexpectedly passed reproducibility" >&2
  exit 1
fi
jq -e '.status == "failed" and (.differences | index("profile_identity:EL")) != null' \
  "$TMP_DIR/changed-reproducibility.json" >/dev/null

if python3 "$FREEZER" validate --manifest "$PACKAGE_ONE/dataset-manifest.json" >/dev/null; then
  :
else
  echo "complete fixture validation failed" >&2
  exit 1
fi

MISSING_SOURCE_DIR="$TMP_DIR/missing-source"
set +e
python3 "$FREEZER" prepare \
  --source-dir "$MISSING_SOURCE_DIR" \
  --output-dir "$TMP_DIR/missing-package" \
  > "$TMP_DIR/missing-source.out" 2> "$TMP_DIR/missing-source.err"
missing_source_status="$?"
set -e
test "$missing_source_status" = 125
jq -e '.status == "not_available" and (.reason | contains("OWL2Bench source README"))' \
  "$TMP_DIR/missing-source.out" >/dev/null

printf '%s\n' '[test-owl2bench-profile-dataset] ok'
