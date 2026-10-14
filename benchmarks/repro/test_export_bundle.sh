#!/usr/bin/env bash
#
# Fixture test for the reproduction export pipeline (issue #203, ADR 0028).
#
# Builds a synthetic mini-tree fixture (a throwaway git repository with a
# small selection manifest and fake run directories) and exercises the
# export script end to end: selection, evidence copy, hash-inventory shape,
# lane claim headers, fail-closed modes, and the self-check. No real
# evidence run directories are touched.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXPORT="$SCRIPT_DIR/export_bundle.py"
COMMITTED_CONFIG="$SCRIPT_DIR/export_selection.json"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

pass_count=0
fail_count=0

assert_ok() {
    local message="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        pass_count=$((pass_count + 1))
        printf '[ok] %s\n' "$message"
    else
        fail_count=$((fail_count + 1))
        printf 'assertion failed: %s\n' "$message" >&2
    fi
}

assert_fails() {
    local message="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        fail_count=$((fail_count + 1))
        printf 'assertion failed (expected failure): %s\n' "$message" >&2
    else
        pass_count=$((pass_count + 1))
        printf '[ok] %s\n' "$message"
    fi
}

assert_eq() {
    local expected="$1"
    local actual="$2"
    local message="$3"
    if [[ "$expected" == "$actual" ]]; then
        pass_count=$((pass_count + 1))
        printf '[ok] %s\n' "$message"
    else
        fail_count=$((fail_count + 1))
        printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
    fi
}

assert_file_contains() {
    local file="$1"
    local needle="$2"
    if grep -Fq -- "$needle" "$file"; then
        pass_count=$((pass_count + 1))
        printf '[ok] %s contains needle\n' "$file"
    else
        fail_count=$((fail_count + 1))
        printf 'assertion failed: %s does not contain: %s\n' "$file" "$needle" >&2
    fi
}

# ---------------------------------------------------------------------------
# Fixture: synthetic mini-tree repository with fake tracked content, fake
# ADRs carrying the fixture claim sentences, and fake run directories.
# ---------------------------------------------------------------------------

FIXTURE="$TMP_DIR/repo"
mkdir -p "$FIXTURE"/{src,docs/adr,docs/research,runs/w3c_run,runs/agg_dl,runs/agg_missing_id,runs/ladder_final,runs/fixture_agg_point}
git -C "$FIXTURE" init -q
git -C "$FIXTURE" -c user.name=fixture -c user.email=fixture@example.com commit -q --allow-empty -m "fixture root"

cat > "$FIXTURE/src/main.rs" <<'RS'
fn main() {
    println!("fixture engine");
}
RS
cat > "$FIXTURE/Cargo.toml" <<'TOML'
[package]
name = "fixture-engine"
version = "0.1.0"
TOML
cat > "$FIXTURE/CONTEXT.md" <<'MD'
# Fixture context

Fixture glossary for the export bundle test.
MD

W3C_CLAIM="Each lane report records its own source hash, semantics-specific oracle hash, and status counts; a lane passes only with zero wrong, failed, and unsupported rows, while timeout or unavailable oracle evidence blocks the lane."
ORE_CLAIM="Its published claims are execution performance and completability only: per-engine times, timeouts, and completed-run counts on the pinned corpus."
LADDER_CLAIM="The sweep is a performance lane under the ADR 0024 terms: execution performance and completability claims only, no correctness claims — no oracle exists for OWL2Bench consistency."

{
    echo "# Fixture ADR 0002"
    echo
    echo "The fixture gate sentence follows, hard-wrapped like a real ADR:"
    echo "$W3C_CLAIM" | fold -s -w 60
} > "$FIXTURE/docs/adr/0002-fixture-gate.md"
{
    echo "# Fixture ADR 0024"
    echo
    echo "$ORE_CLAIM" | fold -s -w 60
} > "$FIXTURE/docs/adr/0024-fixture-lane.md"
{
    echo "# Fixture ADR 0026"
    echo
    echo "$LADDER_CLAIM" | fold -s -w 60
} > "$FIXTURE/docs/adr/0026-fixture-ladder.md"
printf 'lane\tfilename\trow_count\ttsv_sha256\n' > "$FIXTURE/docs/research/fixture_manifest.tsv"

cat > "$FIXTURE/runs/w3c_run/run_metadata.json" <<'JSON'
{"run_id": "fixture-w3c-r1", "timestamp": "2026-10-13T00:00:00+00:00"}
JSON
printf '{"total_results": 2, "result_status_counts": {"passed": 2}}\n' > "$FIXTURE/runs/w3c_run/summary.json"
printf '# Fixture W3C report\n\n- passed: 2\n' > "$FIXTURE/runs/w3c_run/report.md"

write_agg_files() {
    local dir="$1"
    local run_id="$2"
    printf '{"aggregation_type": "comparative_repetition_median", "pass_run_ids": ["%s_p1", "%s_p2"], "pass_count": 2}\n' "$run_id" "$run_id" > "$dir/aggregation_metadata.json"
    printf '# Fixture paper table\n' > "$dir/comparative_paper_table.md"
    printf '# Fixture engine summary\n' > "$dir/comparative_engine_summary.md"
    printf 'engine\tcases\nfixture\t1\n' > "$dir/comparative_engine_summary.tsv"
    printf 'case_id\tengine\tmedian_wall_time_ms\nc1\tfixture\t10\n' > "$dir/comparative_median_times.tsv"
}
write_agg_files "$FIXTURE/runs/agg_dl" "fixture_agg_dl"

# Aggregation whose identity file lacks the pass_run_ids field.
printf '{"aggregation_type": "comparative_repetition_median"}\n' > "$FIXTURE/runs/agg_missing_id/aggregation_metadata.json"
printf '# Broken paper table\n' > "$FIXTURE/runs/agg_missing_id/comparative_paper_table.md"

cat > "$FIXTURE/runs/ladder_final/ladder_metadata.json" <<'JSON'
{
  "ladder_type": "fixture_scalability",
  "profiles": {
    "DL": {
      "engines": ["fixture"],
      "table": "ladder_dl.md",
      "points": [
        {"universities": 1, "aggregation_dir": "fixture_agg_point"}
      ]
    }
  }
}
JSON
printf '# Fixture ladder table\n' > "$FIXTURE/runs/ladder_final/ladder_dl.md"
write_agg_files "$FIXTURE/runs/fixture_agg_point" "fixture_agg_point"

git -C "$FIXTURE" add -A
git -C "$FIXTURE" -c user.name=fixture -c user.email=fixture@example.com commit -q -m "fixture content"

# An untracked file that must never silently enter the bundle.
printf 'untracked scratch\n' > "$FIXTURE/UNTRACKED.txt"

FIXTURE_CONFIG="$TMP_DIR/fixture_config.json"
python3 - "$FIXTURE_CONFIG" "$FIXTURE" <<'PY'
import json
import sys

config_path, fixture = sys.argv[1], sys.argv[2]
agg_artifacts = [
    "aggregation_metadata.json",
    "comparative_paper_table.md",
    "comparative_engine_summary.md",
    "comparative_engine_summary.tsv",
    "comparative_median_times.tsv",
]
config = {
    "schema_version": 1,
    "bundle_name": "fixture-bundle",
    "max_artifact_bytes": 4096,
    "forbidden_suffixes": [".owl", ".jar"],
    "tracked_selection": [
        "src",
        "Cargo.toml",
        "CONTEXT.md",
        "docs/adr",
        "docs/research/fixture_manifest.tsv",
    ],
    "lanes": [
        {
            "name": "w3c-like",
            "title": "Fixture W3C-like conformance lane",
            "claims": [
                {
                    "adr": "docs/adr/0002-fixture-gate.md",
                    "sentence": "Each lane report records its own source hash, semantics-specific oracle hash, and status counts; a lane passes only with zero wrong, failed, and unsupported rows, while timeout or unavailable oracle evidence blocks the lane.",
                }
            ],
            "exports": [
                {
                    "source_dir": "runs/w3c_run",
                    "dest": ".",
                    "artifacts": ["summary.json", "report.md"],
                    "identity": {"file": "run_metadata.json", "run_id_fields": ["run_id"]},
                }
            ],
        },
        {
            "name": "ore-like",
            "title": "Fixture ORE-like comparative lane",
            "claims": [
                {
                    "adr": "docs/adr/0024-fixture-lane.md",
                    "sentence": "Its published claims are execution performance and completability only: per-engine times, timeouts, and completed-run counts on the pinned corpus.",
                }
            ],
            "exports": [
                {
                    "source_dir": "runs/agg_dl",
                    "dest": "pure_dl",
                    "artifacts": agg_artifacts,
                    "identity": {"file": "aggregation_metadata.json", "run_id_fields": ["pass_run_ids"]},
                }
            ],
        },
        {
            "name": "ladder-like",
            "title": "Fixture ladder-like lane",
            "claims": [
                {
                    "adr": "docs/adr/0026-fixture-ladder.md",
                    "sentence": "The sweep is a performance lane under the ADR 0024 terms: execution performance and completability claims only, no correctness claims — no oracle exists for OWL2Bench consistency.",
                }
            ],
            "cross_check_ladder": True,
            "exports": [
                {
                    "source_dir": "runs/ladder_final",
                    "dest": "ladder_final",
                    "artifacts": ["ladder_metadata.json", "ladder_dl.md"],
                    "identity": {"file": "ladder_metadata.json", "run_id_fields": []},
                },
                {
                    "source_dir": "runs/fixture_agg_point",
                    "dest": "agg/fixture_point",
                    "artifacts": agg_artifacts,
                    "identity": {"file": "aggregation_metadata.json", "run_id_fields": ["pass_run_ids"]},
                },
            ],
        },
    ],
}
with open(config_path, "w") as handle:
    json.dump(config, handle, indent=2)
PY

BUNDLE="$TMP_DIR/bundle"

run_export() {
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$FIXTURE_CONFIG" "$@"
}

# ---------------------------------------------------------------------------
# 1. Build: selection, evidence copy, claim headers, inventory.
# ---------------------------------------------------------------------------

assert_ok "export builds the fixture bundle" run_export --output "$BUNDLE"

assert_ok "tracked source file copied at repo-relative path" test -f "$BUNDLE/src/main.rs"
assert_ok "tracked single file copied" test -f "$BUNDLE/Cargo.toml"
assert_ok "untracked file excluded from bundle" test ! -e "$BUNDLE/UNTRACKED.txt"
assert_ok "W3C lane artifact copied" test -f "$BUNDLE/evidence/w3c-like/summary.json"
assert_ok "ORE lane aggregation copied" test -f "$BUNDLE/evidence/ore-like/pure_dl/comparative_median_times.tsv"
assert_ok "ladder final view copied" test -f "$BUNDLE/evidence/ladder-like/ladder_final/ladder_metadata.json"
assert_ok "ladder per-point aggregation copied" test -f "$BUNDLE/evidence/ladder-like/agg/fixture_point/comparative_paper_table.md"

assert_file_contains "$BUNDLE/evidence/w3c-like/README.md" "$W3C_CLAIM"
assert_file_contains "$BUNDLE/evidence/w3c-like/README.md" "fixture-w3c-r1"
assert_file_contains "$BUNDLE/evidence/ore-like/README.md" "$ORE_CLAIM"
assert_file_contains "$BUNDLE/evidence/ore-like/README.md" "fixture_agg_dl_p1"
assert_file_contains "$BUNDLE/evidence/ladder-like/README.md" "$LADDER_CLAIM"
assert_file_contains "$BUNDLE/README.md" "fixture-bundle"

# Inventory shape and completeness against the files on disk.
if python3 - "$BUNDLE" <<'PY'
import hashlib
import json
import os
import sys

bundle = sys.argv[1]
with open(os.path.join(bundle, "hash_inventory.json")) as handle:
    inventory = json.load(handle)

assert inventory["schema_version"] == 1, "inventory schema_version"
entries = inventory["entries"]
assert entries, "inventory has entries"
paths = set()
for entry in entries:
    for key in ("path", "sha256", "source_kind", "source_path", "run_ids"):
        assert key in entry, f"entry missing {key}: {entry}"
    assert entry["source_kind"] in ("tracked_selection", "run_dir", "generated"), entry
    assert os.path.isfile(os.path.join(bundle, entry["path"])), f"missing file: {entry['path']}"
    digest = hashlib.sha256(
        open(os.path.join(bundle, entry["path"]), "rb").read()
    ).hexdigest()
    assert digest == entry["sha256"], f"hash mismatch: {entry['path']}"
    paths.add(entry["path"])
    if entry["source_kind"] == "run_dir":
        assert entry["run_ids"], f"evidence entry without run ids: {entry['path']}"
        assert entry["source_path"], f"evidence entry without source path: {entry['path']}"

on_disk = set()
for root, _dirs, files in os.walk(bundle):
    for name in files:
        on_disk.add(os.path.relpath(os.path.join(root, name), bundle))
unlisted = on_disk - paths - {"hash_inventory.json"}
missing = paths - on_disk
assert not unlisted, f"files on disk not inventoried: {sorted(unlisted)}"
assert not missing, f"inventory entries missing on disk: {sorted(missing)}"

counts = inventory["counts"]
assert counts["total"] == len(entries), "counts.total matches entries"
kinds = {}
for entry in entries:
    kinds[entry["source_kind"]] = kinds.get(entry["source_kind"], 0) + 1
for kind, count in kinds.items():
    assert counts.get(kind) == count, f"counts mismatch for {kind}"
assert inventory["source_commit"], "inventory records source commit"
print("[ok] inventory shape, hashes, completeness, counts")
PY
then
    pass_count=$((pass_count + 1))
else
    fail_count=$((fail_count + 1))
    printf 'assertion failed: inventory shape check\n' >&2
fi

# Evidence bytes must be identical copies of their sources.
assert_ok "evidence copy is byte-identical to source" cmp -s "$FIXTURE/runs/w3c_run/summary.json" "$BUNDLE/evidence/w3c-like/summary.json"

# ---------------------------------------------------------------------------
# 2. Fail-closed modes (each aborts the export).
# ---------------------------------------------------------------------------

assert_fails "refuses non-empty output directory without --force" \
    run_export --output "$BUNDLE"

OUT2="$TMP_DIR/bundle2"
CFG_MISSING="$TMP_DIR/config_missing_artifact.json"
python3 - "$FIXTURE_CONFIG" "$CFG_MISSING" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["lanes"][0]["exports"][0]["artifacts"] = ["summary.json", "report.md", "vanished.md"]
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts when a headline artifact is absent" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_MISSING" --output "$OUT2"

CFG_UNTRACKED="$TMP_DIR/config_untracked.json"
python3 - "$FIXTURE_CONFIG" "$CFG_UNTRACKED" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["tracked_selection"].append("UNTRACKED.txt")
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts when a selection entry is untracked" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_UNTRACKED" --output "$OUT2"

mv "$FIXTURE/src" "$TMP_DIR/src-hidden"
assert_fails "aborts when a selection entry vanished from disk" \
    run_export --output "$OUT2"
mv "$TMP_DIR/src-hidden" "$FIXTURE/src"

CFG_CLAIM="$TMP_DIR/config_claim_drift.json"
python3 - "$FIXTURE_CONFIG" "$CFG_CLAIM" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["lanes"][0]["claims"][0]["sentence"] = (
    "Each lane report records its own source hash and passes whenever it feels right."
)
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts when a claim sentence is not verbatim in its ADR" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_CLAIM" --output "$OUT2"

CFG_NO_ID="$TMP_DIR/config_missing_identity.json"
python3 - "$FIXTURE_CONFIG" "$CFG_NO_ID" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
agg = list(config["lanes"][1]["exports"][0]["artifacts"])
config["lanes"][1]["exports"].append(
    {
        "source_dir": "runs/agg_missing_id",
        "dest": "broken",
        "artifacts": ["aggregation_metadata.json", "comparative_paper_table.md"],
        "identity": {"file": "aggregation_metadata.json", "run_id_fields": ["pass_run_ids"]},
    }
)
assert agg
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts when run identity fields are missing" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_NO_ID" --output "$OUT2"

CFG_LADDER="$TMP_DIR/config_ladder_mismatch.json"
python3 - "$FIXTURE_CONFIG" "$CFG_LADDER" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["lanes"][2]["exports"] = [config["lanes"][2]["exports"][0]]
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts when ladder metadata references an unconfigured aggregation" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_LADDER" --output "$OUT2"

printf 'fixture ontology blob\n' > "$FIXTURE/corpus.owl"
git -C "$FIXTURE" add corpus.owl
git -C "$FIXTURE" -c user.name=fixture -c user.email=fixture@example.com commit -q -m "blob"
CFG_BLOB="$TMP_DIR/config_blob.json"
python3 - "$FIXTURE_CONFIG" "$CFG_BLOB" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["tracked_selection"].append("corpus.owl")
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts on pointers-not-blobs forbidden suffix" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_BLOB" --output "$OUT2"

# A selected, tiny harness-config file may be allowlisted by explicit path
# (deliberate exception); a dead allowlist entry fails closed.
OUT_ALLOW="$TMP_DIR/bundle_allow"
CFG_ALLOW="$TMP_DIR/config_allowlist.json"
python3 - "$FIXTURE_CONFIG" "$CFG_ALLOW" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["tracked_selection"].append("corpus.owl")
config["forbidden_suffix_allowlist"] = ["corpus.owl"]
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_ok "allowlisted tracked config file passes the suffix guard" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_ALLOW" --output "$OUT_ALLOW"
assert_ok "self-check honors the allowlist consistently" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_ALLOW" --self-check "$OUT_ALLOW"

CFG_DEAD_ALLOW="$TMP_DIR/config_dead_allowlist.json"
python3 - "$FIXTURE_CONFIG" "$CFG_DEAD_ALLOW" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["forbidden_suffix_allowlist"] = ["corpus.owl"]
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts on an allowlist entry outside the selection" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_DEAD_ALLOW" --output "$OUT_ALLOW"

python3 - "$FIXTURE" <<'PY'
import sys
path = sys.argv[1] + "/big.bin"
with open(path, "wb") as handle:
    handle.write(b"\0" * 8192)
PY
CFG_BIG="$TMP_DIR/config_big.json"
python3 - "$FIXTURE_CONFIG" "$CFG_BIG" <<'PY'
import json
import sys
config = json.load(open(sys.argv[1]))
config["tracked_selection"].append("big.bin")
json.dump(config, open(sys.argv[2], "w"), indent=2)
PY
assert_fails "aborts on pointers-not-blobs size cap" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$CFG_BIG" --output "$OUT2"
rm -f "$FIXTURE/corpus.owl" "$FIXTURE/big.bin"
git -C "$FIXTURE" add -A
git -C "$FIXTURE" -c user.name=fixture -c user.email=fixture@example.com commit -q -m "drop blobs"

# ---------------------------------------------------------------------------
# 3. Self-check: bundle re-hash and completeness.
# ---------------------------------------------------------------------------

run_self_check() {
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$FIXTURE_CONFIG" --self-check "$BUNDLE"
}
assert_ok "self-check passes on the intact bundle" run_self_check

cp "$BUNDLE/src/main.rs" "$TMP_DIR/main.rs.bak"
printf '// tampered\n' >> "$BUNDLE/src/main.rs"
assert_fails "self-check detects a tampered artifact" run_self_check
cp "$TMP_DIR/main.rs.bak" "$BUNDLE/src/main.rs"

rm "$BUNDLE/evidence/w3c-like/summary.json"
assert_fails "self-check detects a deleted artifact" run_self_check
printf '{"total_results": 2, "result_status_counts": {"passed": 2}}\n' > "$BUNDLE/evidence/w3c-like/summary.json"

printf 'rogue\n' > "$BUNDLE/rogue.txt"
assert_fails "self-check detects an uninventoried file" run_self_check
rm "$BUNDLE/rogue.txt"

mv "$BUNDLE/hash_inventory.json" "$TMP_DIR/hash_inventory.json.hidden"
assert_fails "self-check fails without a hash inventory" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$FIXTURE_CONFIG" --self-check "$BUNDLE"
mv "$TMP_DIR/hash_inventory.json.hidden" "$BUNDLE/hash_inventory.json"

mv "$BUNDLE/docs/adr/0002-fixture-gate.md" "$TMP_DIR/0002-fixture-gate.md.hidden"
printf '# swapped ADR\n' > "$BUNDLE/docs/adr/0002-fixture-gate.md"
assert_fails "self-check fails when a bundled ADR loses the claim sentence" \
    python3 "$EXPORT" --repo-root "$FIXTURE" --config "$FIXTURE_CONFIG" --self-check "$BUNDLE"
mv "$TMP_DIR/0002-fixture-gate.md.hidden" "$BUNDLE/docs/adr/0002-fixture-gate.md"

assert_ok "self-check passes again after restore" run_self_check

# ---------------------------------------------------------------------------
# 4. Committed manifest contract (no run directories needed): the committed
#    selection manifest parses, every claim sentence is verbatim in its ADR,
#    and every tracked_selection path exists in this repository.
# ---------------------------------------------------------------------------

if python3 - "$COMMITTED_CONFIG" "$SCRIPT_DIR/../.." <<'PY'
import json
import os
import re
import sys

config_path, repo_root = sys.argv[1], sys.argv[2]
config = json.load(open(config_path))
assert config["schema_version"] == 1, "committed config schema"
assert config["lanes"], "committed config lists lanes"

def collapse(text):
    return re.sub(r"\s+", " ", text).strip()

for lane in config["lanes"]:
    for claim in lane["claims"]:
        adr_path = os.path.join(repo_root, claim["adr"])
        assert os.path.isfile(adr_path), f"missing ADR {claim['adr']}"
        collapsed = collapse(open(adr_path).read())
        assert collapse(claim["sentence"]) in collapsed, (
            f"lane {lane['name']}: claim sentence not verbatim in {claim['adr']}"
        )
    for export in lane["exports"]:
        assert export["dest"] != "..", "dest escapes lane dir"
bundle_claim = config.get("bundle_claim")
if bundle_claim:
    adr_path = os.path.join(repo_root, bundle_claim["adr"])
    collapsed = collapse(open(adr_path).read())
    assert collapse(bundle_claim["sentence"]) in collapsed, "bundle claim not verbatim"

for path in config["tracked_selection"]:
    assert os.path.exists(os.path.join(repo_root, path)), f"missing selection path {path}"

expected_lanes = {"w3c-conformance", "owl2bench-query-verdict", "ore-comparative", "owl2bench-ladder"}
assert {lane["name"] for lane in config["lanes"]} == expected_lanes, "all four lanes configured"
print("[ok] committed manifest contract")
PY
then
    pass_count=$((pass_count + 1))
else
    fail_count=$((fail_count + 1))
    printf 'assertion failed: committed manifest contract\n' >&2
fi

# ---------------------------------------------------------------------------

printf '\nfixture export test: %d passed, %d failed\n' "$pass_count" "$fail_count"
if [[ "$fail_count" -ne 0 ]]; then
    exit 1
fi
