#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LADDER="$SCRIPT_DIR/ladder_dataset.py"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-ladder-dataset-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE_DIR="$TMP_DIR/source"
ANCHOR_STAGED="$TMP_DIR/staged"
OUT_DIR="$TMP_DIR/scalability"
BIN_DIR="$TMP_DIR/bin"
JAVA_SHIM="$BIN_DIR/java"
FLAKY_SHIM="$BIN_DIR/java-flaky"
TIME_SHIM="$BIN_DIR/time"
SHIM_LOG="$TMP_DIR/shim-invocations.log"
FLAKY_STATE="$TMP_DIR/flaky-state"

mkdir -p "$SOURCE_DIR" "$ANCHOR_STAGED" "$BIN_DIR"

printf '# test source\n' > "$SOURCE_DIR/README.md"
for profile in DL EL QL RL; do
  printf '<rdf:RDF/>\n' > "$SOURCE_DIR/UNIV-BENCH-OWL2${profile}.owl"
done
printf 'fake-generator-jar\n' > "$SOURCE_DIR/OWL2Bench.jar"
printf 'ontologyFormat=rdf\n' > "$SOURCE_DIR/config.properties"
printf 'random\n' > "$SOURCE_DIR/RandomNames.xlsx"
JAR_HASH="$(sha256sum "$SOURCE_DIR/OWL2Bench.jar" | awk '{print $1}')"

# Fake staged seed-1 university-1 anchors plus a staged manifest that pins the
# generator invocation for each anchor row, mirroring the real staged tree.
ANCHORS_TSV="$ANCHOR_STAGED/manifest.tsv"
printf 'staged_ontology\tsource_ontology\tprofile\tseed\tabox_size\tuniversity_count\tstage\n' > "$ANCHORS_TSV"
declare -A anchor_hash
for entry in "5 RL" "6 DL" "7 EL" "8 QL"; do
  idx="${entry%% *}"
  profile="${entry##* }"
  anchor_file="$ANCHOR_STAGED/000${idx}_UNIV-BENCH-OWL2${profile}-seed1-u1.owl"
  printf 'anchor-%s\n' "$profile" > "$anchor_file"
  anchor_hash[$profile]="$(sha256sum "$anchor_file" | awk '{print $1}')"
  printf '%s\tkracr/owl2bench@%s:OWL2Bench.jar 1 %s 1\t%s\t1\t50000\t1\tcorrectness\n' \
    "$anchor_file" "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef" "$profile" "$profile" >> "$ANCHORS_TSV"
done
FAKE_REV="deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"

# Deterministic stand-in for the official generator: same positional contract
# as `java -jar OWL2Bench.jar <universities> <PROFILE> <seed>`, output bytes
# are a pure function of the arguments and the jar contents.
cat > "$JAVA_SHIM" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == "-jar" ]] || { echo "unexpected generator args: $*" >&2; exit 2; }
jar="$2"; universities="$3"; profile="$4"; seed="$5"
if [[ -n "${SHIM_LOG:-}" ]]; then
  printf '%s %s %s\n' "$universities" "$profile" "$seed" >> "$SHIM_LOG"
fi
{
  printf 'universities=%s\n' "$universities"
  printf 'profile=%s\n' "$profile"
  printf 'seed=%s\n' "$seed"
  printf 'jar='
  cat "$jar"
} > "OWL2${profile}-${universities}.owl"
SH

# Flaky stand-in: identical to the deterministic shim except it embeds a
# monotonically increasing run counter for FLAKY_UNIVERSITIES, so the second
# run of that size produces different bytes.
cat > "$FLAKY_SHIM" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == "-jar" ]] || { echo "unexpected generator args: $*" >&2; exit 2; }
jar="$2"; universities="$3"; profile="$4"; seed="$5"
if [[ -n "${SHIM_LOG:-}" ]]; then
  printf '%s %s %s\n' "$universities" "$profile" "$seed" >> "$SHIM_LOG"
fi
{
  printf 'universities=%s\n' "$universities"
  printf 'profile=%s\n' "$profile"
  printf 'seed=%s\n' "$seed"
  printf 'jar='
  cat "$jar"
  if [[ "${FLAKY_UNIVERSITIES:-}" == "$universities" ]]; then
    count=0
    if [[ -f "${FLAKY_STATE:-}" ]]; then
      count="$(cat "${FLAKY_STATE:-}")"
    fi
    count=$((count + 1))
    printf '%s' "$count" > "${FLAKY_STATE:-}"
    printf 'flaky_run=%s\n' "$count"
  fi
} > "OWL2${profile}-${universities}.owl"
SH

# Stand-in for GNU time: fixed `-v -o FILE command...` shape, reports a
# constant peak RSS in the same format `/usr/bin/time -v` uses.
cat > "$TIME_SHIM" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == "-v" && "$2" == "-o" ]] || { echo "unexpected time args: $*" >&2; exit 2; }
report="$3"
shift 3
"$@"
printf 'Maximum resident set size (kbytes): 4096\n' > "$report"
SH
chmod +x "$JAVA_SHIM" "$FLAKY_SHIM" "$TIME_SHIM"

generate_fixture() {
  local output_dir="$1"
  shift
  SHIM_LOG="$SHIM_LOG" python3 "$LADDER" generate \
    --source-dir "$SOURCE_DIR" \
    --output-dir "$output_dir" \
    --anchors-manifest "$ANCHORS_TSV" \
    --java-bin "$JAVA_SHIM" \
    --time-bin "$TIME_SHIM" \
    --source-revision "$FAKE_REV" \
    --expected-revision "$FAKE_REV" \
    --expected-jar-sha256 "$JAR_HASH" \
    --anchor-pin "DL=${anchor_hash[DL]}" \
    --anchor-pin "EL=${anchor_hash[EL]}" \
    --anchor-pin "QL=${anchor_hash[QL]}" \
    --anchor-pin "RL=${anchor_hash[RL]}" \
    "$@"
}

# 1. A filtered ladder generation stages every requested dataset, reuses the
#    u1 anchors by hash without invoking the generator for them, and writes a
#    manifest whose generated points carry byte-identical double-run hashes.
rm -f "$SHIM_LOG"; touch "$SHIM_LOG"
generate_fixture "$OUT_DIR" --profiles DL,EL --sizes 2,5 --seed 1 --timeout-seconds 60 \
  > "$TMP_DIR/generate.out"

MANIFEST="$OUT_DIR/ladder-manifest.json"
test -f "$MANIFEST"
jq -e '.status == "complete"' "$MANIFEST" >/dev/null
jq -e '(.points | length) == 6' "$MANIFEST" >/dev/null
jq -e --arg h "sha256:${anchor_hash[DL]}" \
  '.points[0] | .profile == "DL" and .universities == 1 and .origin == "anchor-reuse"
     and .generated == false and .output.sha256 == $h and .actuals == null' "$MANIFEST" >/dev/null
jq -e --arg h "sha256:${anchor_hash[EL]}" \
  '.points[3] | .profile == "EL" and .universities == 1 and .origin == "anchor-reuse"
     and .generated == false and .output.sha256 == $h' "$MANIFEST" >/dev/null
generated_sha="$(jq -r '.points[1].output.sha256' "$MANIFEST")"
jq -e --arg h "$generated_sha" \
  '.points[1] | .profile == "DL" and .universities == 2 and .origin == "generated"
     and .generated == true
     and .determinism.runs == 2
     and .determinism.status == "byte-identical"
     and .determinism.run1_sha256 == $h
     and .determinism.run2_sha256 == $h
     and .actuals.wall_time_seconds.run1 > 0
     and .actuals.wall_time_seconds.run2 > 0
     and .actuals.peak_rss_kib.run1 == 4096
     and .actuals.peak_rss_kib.run2 == 4096
     and .actuals.rss_recorded == true' "$MANIFEST" >/dev/null
jq -e --arg rev "$FAKE_REV" --arg jar "sha256:$JAR_HASH" \
  '.generator.revision == $rev
     and .generator.jar_sha256 == $jar
     and .generator.command == "java -jar OWL2Bench.jar <universities> <PROFILE> <seed>"
     and .generator.seed == 1' "$MANIFEST" >/dev/null
jq -e --arg inv "kracr/owl2bench@$FAKE_REV:OWL2Bench.jar 2 DL 1" \
  '.points[1].invocation == $inv' "$MANIFEST" >/dev/null
test -f "$OUT_DIR/UNIV-BENCH-OWL2DL-seed1-universities2.owl"
test -f "$OUT_DIR/UNIV-BENCH-OWL2DL-seed1-universities5.owl"
test -f "$OUT_DIR/UNIV-BENCH-OWL2EL-seed1-universities2.owl"
test -f "$OUT_DIR/UNIV-BENCH-OWL2EL-seed1-universities5.owl"

# The generator ran exactly twice per generated dataset...
test "$(grep -cE '^2 (DL|EL) 1$' "$SHIM_LOG")" -eq 4
test "$(grep -cE '^5 (DL|EL) 1$' "$SHIM_LOG")" -eq 4
# ...and never for the reused u1 anchors.
if grep -qE '^1 (DL|EL) 1$' "$SHIM_LOG"; then
  echo "u1 anchor was regenerated instead of reused by hash" >&2
  exit 1
fi

# 2. validate accepts the fixture manifest and rejects tampered variants.
python3 "$LADDER" validate \
  --manifest "$MANIFEST" \
  --expect-profiles DL,EL --expect-sizes 2,5 \
  --expect-revision "$FAKE_REV" --expect-jar-sha256 "$JAR_HASH" \
  --anchor-pin "DL=${anchor_hash[DL]}" \
  --anchor-pin "EL=${anchor_hash[EL]}" \
  > "$TMP_DIR/validate.out"
jq -e '.status == "complete" and (.errors | length) == 0' "$TMP_DIR/validate.out" >/dev/null

jq '(.points[] | select(.profile == "DL" and .universities == 2)
      | .determinism.run2_sha256) = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$MANIFEST" > "$TMP_DIR/broken-determinism.json"
set +e
python3 "$LADDER" validate \
  --manifest "$TMP_DIR/broken-determinism.json" \
  --expect-profiles DL,EL --expect-sizes 2,5 \
  --expect-revision "$FAKE_REV" --expect-jar-sha256 "$JAR_HASH" \
  --anchor-pin "DL=${anchor_hash[DL]}" \
  --anchor-pin "EL=${anchor_hash[EL]}" \
  > "$TMP_DIR/broken-determinism.out" 2>&1
broken_status="$?"
set -e
test "$broken_status" -ne 0
jq -e '.status == "failed" and (.errors | index("determinism_mismatch:DL:2")) != null' \
  "$TMP_DIR/broken-determinism.out" >/dev/null

jq 'del(.points[] | select(.profile == "EL" and .universities == 5))' \
  "$MANIFEST" > "$TMP_DIR/broken-coverage.json"
set +e
python3 "$LADDER" validate \
  --manifest "$TMP_DIR/broken-coverage.json" \
  --expect-profiles DL,EL --expect-sizes 2,5 \
  --expect-revision "$FAKE_REV" --expect-jar-sha256 "$JAR_HASH" \
  --anchor-pin "DL=${anchor_hash[DL]}" \
  --anchor-pin "EL=${anchor_hash[EL]}" \
  > "$TMP_DIR/broken-coverage.out" 2>&1
coverage_status="$?"
set -e
test "$coverage_status" -ne 0
jq -e '.status == "failed" and (.errors | index("point_missing:EL:5")) != null' \
  "$TMP_DIR/broken-coverage.out" >/dev/null

# 3. Determinism gating fails closed: a byte mismatch stages nothing for the
#    mismatching dataset and no manifest is written, while the dataset whose
#    double run matched remains staged.
FLAKY_OUT="$TMP_DIR/flaky-scalability"
rm -f "$FLAKY_STATE"
set +e
FLAKY_UNIVERSITIES=5 FLAKY_STATE="$FLAKY_STATE" SHIM_LOG="$TMP_DIR/flaky.log" \
  python3 "$LADDER" generate \
    --source-dir "$SOURCE_DIR" \
    --output-dir "$FLAKY_OUT" \
    --anchors-manifest "$ANCHORS_TSV" \
    --profiles DL --sizes 2,5 --seed 1 --timeout-seconds 60 \
    --java-bin "$FLAKY_SHIM" --time-bin "$TIME_SHIM" \
    --source-revision "$FAKE_REV" --expected-revision "$FAKE_REV" \
    --expected-jar-sha256 "$JAR_HASH" \
    --anchor-pin "DL=${anchor_hash[DL]}" \
    > "$TMP_DIR/flaky.out" 2> "$TMP_DIR/flaky.err"
flaky_status="$?"
set -e
test "$flaky_status" -ne 0
grep -q "byte-determinism" "$TMP_DIR/flaky.err"
grep -q "DL" "$TMP_DIR/flaky.err"
grep -q "universities=5" "$TMP_DIR/flaky.err"
test -f "$FLAKY_OUT/UNIV-BENCH-OWL2DL-seed1-universities2.owl"
test ! -e "$FLAKY_OUT/UNIV-BENCH-OWL2DL-seed1-universities5.owl"
test ! -e "$FLAKY_OUT/ladder-manifest.json"

# 4. Anchor drift fails closed: a staged anchor whose bytes no longer match the
#    pinned hash is never referenced.
DRIFT_OUT="$TMP_DIR/drift-scalability"
printf 'tampered\n' >> "$ANCHOR_STAGED/0006_UNIV-BENCH-OWL2DL-seed1-u1.owl"
set +e
generate_fixture "$DRIFT_OUT" --profiles DL --sizes 2 --timeout-seconds 60 \
  > "$TMP_DIR/drift.out" 2> "$TMP_DIR/drift.err"
drift_status="$?"
set -e
test "$drift_status" -ne 0
grep -q "anchor" "$TMP_DIR/drift.err"
grep -q "${anchor_hash[DL]}" "$TMP_DIR/drift.err"
test ! -e "$DRIFT_OUT/ladder-manifest.json"
printf 'anchor-DL\n' > "$ANCHOR_STAGED/0006_UNIV-BENCH-OWL2DL-seed1-u1.owl"

# 5. University count 1 is rejected as a generation size: it is the anchor
#    point, reused by hash, never regenerated.
set +e
generate_fixture "$TMP_DIR/anchor-size" --profiles DL --sizes 1 \
  > "$TMP_DIR/anchor-size.out" 2> "$TMP_DIR/anchor-size.err"
anchor_size_status="$?"
set -e
test "$anchor_size_status" -ne 0
grep -q "anchor" "$TMP_DIR/anchor-size.err"

# 6. Resume: a completed point is not regenerated when its staged bytes still
#    match the previously verified manifest entry.
identity_before="$(jq -r '.identity_sha256' "$MANIFEST")"
rm -f "$SHIM_LOG"; touch "$SHIM_LOG"
generate_fixture "$OUT_DIR" --profiles DL,EL --sizes 2,5 --timeout-seconds 60 \
  > "$TMP_DIR/resume.out"
test "$(wc -l < "$SHIM_LOG")" -eq 0
test "$(jq -r '.identity_sha256' "$MANIFEST")" == "$identity_before"
jq -e '.status == "complete"' "$MANIFEST" >/dev/null

# 7. The committed ladder manifest exists, is tracked, covers the full
#    24-point ladder, and the datasets next to it stay gitignored.
COMMITTED_MANIFEST="$ROOT_DIR/benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json"
test -f "$COMMITTED_MANIFEST"
git -C "$ROOT_DIR" ls-files --error-unmatch \
  "benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json" >/dev/null
git -C "$ROOT_DIR" check-ignore -q \
  "benchmarks/ontologies/external/owl2bench/scalability/UNIV-BENCH-OWL2DL-seed1-universities2.owl"
git -C "$ROOT_DIR" check-ignore -q \
  "benchmarks/ontologies/external/owl2bench/scalability/logs/example.log"
if git -C "$ROOT_DIR" check-ignore -q \
  "benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json"; then
  echo "committed ladder manifest must not be gitignored" >&2
  exit 1
fi
python3 "$LADDER" validate --manifest "$COMMITTED_MANIFEST" --shape-only \
  > "$TMP_DIR/committed.out"
jq -e '.status == "complete" and .point_count == 24' "$TMP_DIR/committed.out" >/dev/null

printf '%s\n' '[test-owl2bench-ladder-dataset] ok'
