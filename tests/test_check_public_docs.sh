#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GUARD="$REPO_ROOT/scripts/check_public_docs.sh"
FIXTURE_ROOT="$(mktemp -d)"
trap 'rm -rf "$FIXTURE_ROOT"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_guard_fails_with() {
  local root="$1"
  local expected="$2"
  local output
  local status

  set +e
  output="$(bash "$GUARD" --root "$root" 2>&1)"
  status=$?
  set -e

  [[ "$status" -ne 0 ]] || fail "guard should fail for $root"
  [[ "$output" == *"$expected"* ]] || \
    fail "expected '$expected' in guard output; got: $output"
}

make_clean_fixture() {
  local root="$1"

  mkdir -p \
    "$root/benchmarks/results/retained" \
    "$root/docs/adr" \
    "$root/docs/agents" \
    "$root/docs/benchmarking" \
    "$root/docs/research" \
    "$root/docs/specs" \
    "$root/src/reasoner"
  touch "$root/CONTEXT.md"
  touch "$root/docs/adr/0001-official-oracle-first-benchmark-contract.md"
  touch "$root/docs/adr/0002-three-lane-w3c-full-corpus-gate.md"
  touch "$root/docs/adr/0003-unified-structural-owl-expression-model.md"
  touch "$root/docs/adr/0004-model-based-direct-semantics-entailment.md"
  touch "$root/docs/adr/0005-dual-semantic-views-for-functional-w3c-rows.md"
  touch "$root/docs/adr/0006-owned-rdf-graph-boundary-for-owl-full.md"
  touch "$root/docs/adr/0009-deterministic-feedback-threshold-boundary.md"
  touch "$root/docs/agents/domain.md"
  touch "$root/docs/agents/issue-tracker.md"
  touch "$root/docs/agents/triage-labels.md"
  touch "$root/docs/specs/OWL2_BENCHMARK_CONFORMANCE.md"
  touch "$root/docs/specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md"
  printf '%s\n' \
    '# SPACL scheduling status' \
    'The active reasoner uses a static scheduling gate.' \
    'Soundness and completeness remain verification objectives and are not formally proven here.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  printf '%s\n' \
    '//! Tableauxx exposes an experimental speculative reasoner.' \
    > "$root/src/lib.rs"
  printf '%s\n' \
    '//! The active scheduling decision uses fixed pre-run thresholds.' \
    > "$root/src/reasoner/speculative.rs"
  touch "$root/docs/research/linked.md"
  touch "$root/docs/research/linked file.md"
  touch "$root/benchmarks/results/retained/summary.json"
  printf '%s\n' \
    'Artifact: `benchmarks/results/retained/summary.json`' \
    'Historical: `unretained-artifact:benchmark/example/summary.json`' \
    '```bash' \
    'output=benchmarks/results/generated/summary.json' \
    '```' \
    > "$root/docs/benchmarking/evidence.md"
  printf '%s\n' \
    '[Conformance spec](../specs/OWL2_BENCHMARK_CONFORMANCE.md#solution)' \
    '[File with spaces](<linked file.md>)' \
    '[File with line suffix](linked.md:12)' \
    '[External](https://example.com/reference)' \
    '[Mail](mailto:maintainer@example.com)' \
    '[Section](#local-section)' \
    > "$root/docs/research/overview.md"
}

test_clean_public_docs_pass() {
  local root="$FIXTURE_ROOT/clean"
  local output

  make_clean_fixture "$root"
  if ! output="$(bash "$GUARD" --root "$root" 2>&1)"; then
    fail "clean public docs should pass; output: $output"
  fi
}

test_missing_required_doc_fails() {
  local root="$FIXTURE_ROOT/missing-required"

  make_clean_fixture "$root"
  rm "$root/CONTEXT.md"
  assert_guard_fails_with "$root" "missing required doc: CONTEXT.md"
}

test_missing_required_agent_doc_fails() {
  local root="$FIXTURE_ROOT/missing-agent-doc"

  make_clean_fixture "$root"
  rm "$root/docs/agents/domain.md"
  assert_guard_fails_with "$root" "missing required doc: docs/agents/domain.md"
}

test_missing_feedback_experiment_spec_fails() {
  local root="$FIXTURE_ROOT/missing-feedback-experiment-spec"

  make_clean_fixture "$root"
  rm "$root/docs/specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md"
  assert_guard_fails_with "$root" \
    "missing required doc: docs/specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md"
}

test_missing_local_link_fails() {
  local root="$FIXTURE_ROOT/missing-link"

  make_clean_fixture "$root"
  printf '%s\n' '[missing](../specs/DOES_NOT_EXIST.md)' \
    > "$root/docs/research/broken.md"
  assert_guard_fails_with "$root" \
    "missing local link: docs/research/broken.md -> ../specs/DOES_NOT_EXIST.md"
}

test_missing_benchmark_link_fails() {
  local root="$FIXTURE_ROOT/missing-benchmark-link"

  make_clean_fixture "$root"
  mkdir -p "$root/benchmarks"
  printf '%s\n' '[missing](artifacts/DOES_NOT_EXIST.json)' \
    > "$root/benchmarks/README.md"
  assert_guard_fails_with "$root" \
    "missing local link: benchmarks/README.md -> artifacts/DOES_NOT_EXIST.json"
}

test_missing_line_suffixed_link_fails() {
  local root="$FIXTURE_ROOT/missing-line-link"

  make_clean_fixture "$root"
  printf '%s\n' '[missing](DOES_NOT_EXIST.md:12)' \
    > "$root/docs/research/broken-line-link.md"
  assert_guard_fails_with "$root" \
    "missing local link: docs/research/broken-line-link.md -> DOES_NOT_EXIST.md:12"
}

test_missing_code_span_artifact_fails() {
  local root="$FIXTURE_ROOT/missing-code-span-artifact"

  make_clean_fixture "$root"
  printf '%s\n' 'Artifact: `benchmarks/results/missing/summary.json`' \
    > "$root/docs/benchmarking/evidence.md"
  assert_guard_fails_with "$root" \
    "missing artifact reference: docs/benchmarking/evidence.md -> benchmarks/results/missing/summary.json"
}

test_missing_experiment_artifact_fails() {
  local root="$FIXTURE_ROOT/missing-experiment-artifact"

  make_clean_fixture "$root"
  mkdir -p "$root/docs/experiments"
  printf '%s\n' 'Artifact: `benchmarks/results/missing/summary.json`' \
    > "$root/docs/experiments/evidence.md"
  assert_guard_fails_with "$root" \
    "missing artifact reference: docs/experiments/evidence.md -> benchmarks/results/missing/summary.json"
}

test_missing_plaintext_artifact_fails() {
  local root="$FIXTURE_ROOT/missing-plaintext-artifact"

  make_clean_fixture "$root"
  mkdir -p "$root/docs/experiments"
  printf '%s\n' 'Artifact: benchmarks/results/missing/summary.json' \
    > "$root/docs/experiments/evidence.md"
  assert_guard_fails_with "$root" \
    "missing artifact reference: docs/experiments/evidence.md -> benchmarks/results/missing/summary.json"
}

test_host_local_paths_fail() {
  local root="$FIXTURE_ROOT/host-local"

  make_clean_fixture "$root"
  printf '%s\n' \
    'Home artifact: `/home/agent/private/result.json`' \
    'Ephemeral artifact: `/tmp/tableauxx-run.A1b2C3/result.json`' \
    > "$root/docs/research/host-local.md"
  assert_guard_fails_with "$root" \
    "forbidden host-local path: docs/research/host-local.md -> /home/agent/private/result.json"
  assert_guard_fails_with "$root" \
    "forbidden host-local path: docs/research/host-local.md -> /tmp/tableauxx-run.A1b2C3/result.json"
}

test_non_public_paper_paths_fail() {
  local root="$FIXTURE_ROOT/non-public-paper"

  make_clean_fixture "$root"
  printf '%s\n' \
    'Primary manuscript: `paper/submission/manuscript.tex`' \
    'RRN manuscript: `paper/submission_rrn/manuscript.tex`' \
    > "$root/docs/research/paper-paths.md"
  assert_guard_fails_with "$root" \
    "non-public paper path: docs/research/paper-paths.md -> paper/submission/manuscript.tex"
  assert_guard_fails_with "$root" \
    "non-public paper path: docs/research/paper-paths.md -> paper/submission_rrn/manuscript.tex"
}

test_non_public_paper_directory_fails() {
  local root="$FIXTURE_ROOT/non-public-paper-directory"

  make_clean_fixture "$root"
  printf '%s\n' 'Private directory: `paper/submission/`' \
    > "$root/docs/research/paper-directory.md"
  assert_guard_fails_with "$root" \
    "non-public paper path: docs/research/paper-directory.md -> paper/submission/"
}

test_non_public_paper_directory_without_slash_fails() {
  local root="$FIXTURE_ROOT/non-public-paper-directory-no-slash"

  make_clean_fixture "$root"
  printf '%s\n' 'Private directory: `paper/submission_rrn`' \
    > "$root/docs/research/paper-directory.md"
  assert_guard_fails_with "$root" \
    "non-public paper path: docs/research/paper-directory.md -> paper/submission_rrn"
}

test_non_public_path_in_legacy_docs_subtree_fails() {
  local root="$FIXTURE_ROOT/non-public-legacy-doc"

  make_clean_fixture "$root"
  mkdir -p "$root/docs/reports/root-archive"
  printf '%s\n' 'Private file: `paper/submission/manuscript.tex`' \
    > "$root/docs/reports/root-archive/legacy.md"
  assert_guard_fails_with "$root" \
    "non-public paper path: docs/reports/root-archive/legacy.md -> paper/submission/manuscript.tex"
}

test_spacl_novelty_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-novelty-claim"

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is a novel OWL2 DL reasoning algorithm.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified novelty claim"
}

test_spacl_dynamic_runtime_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-dynamic-runtime-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    '//! Evolutionary optimization dynamically adjusts parallelism parameters.' \
    > "$root/src/reasoner/speculative.rs"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: src/reasoner/speculative.rs -> active dynamic-tuning claim"
}

test_spacl_dynamic_gate_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-dynamic-gate-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    'SPACL dynamically decides when to parallelize from ontology characteristics.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> active dynamic-tuning claim"
}

test_spacl_active_tuner_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-active-tuner-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    'SPACL uses an `AdaptiveTuner` that adjusts its threshold from observed speedup.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> active AdaptiveTuner claim"
}

test_spacl_formal_property_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-formal-property-claim"

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is **sound** and complete for OWL2 DL.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified soundness claim"
}

test_spacl_formally_proven_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-formally-proven-claim"

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is formally proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified formal-proof claim"
}

test_spacl_negated_novelty_claim_passes() {
  local root="$FIXTURE_ROOT/spacl-negated-novelty-claim"
  local output

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is not novel.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  if ! output="$(bash "$GUARD" --root "$root" 2>&1)"; then
    fail "negated novelty claim should pass; output: $output"
  fi
}

test_spacl_positive_novelty_after_negation_fails() {
  local root="$FIXTURE_ROOT/spacl-positive-novelty-after-negation"

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is not novel. SPACL is novel.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified novelty claim"
}

test_spacl_positive_novelty_before_negation_fails() {
  local root="$FIXTURE_ROOT/spacl-positive-novelty-before-negation"

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is novel. SPACL is not novel.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified novelty claim"
}

test_spacl_negated_formal_proof_claim_passes() {
  local root="$FIXTURE_ROOT/spacl-negated-formal-proof-claim"
  local output

  make_clean_fixture "$root"
  printf '%s\n' 'SPACL is not formally proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  if ! output="$(bash "$GUARD" --root "$root" 2>&1)"; then
    fail "negated formal proof claim should pass; output: $output"
  fi
}

test_spacl_line_wrapped_formal_proof_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-line-wrapped-formal-proof-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    'The current paper-facing position is narrow.' \
    'SPACL is formally' \
    'proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified formal-proof claim"
}

test_spacl_line_wrapped_rust_doc_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-line-wrapped-rust-doc-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    '//! Crate-level public documentation.' \
    '//! SPACL is formally' \
    '//! proven.' \
    > "$root/src/lib.rs"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: src/lib.rs -> unqualified formal-proof claim"
}

test_spacl_fenced_formal_proof_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-fenced-formal-proof-claim"

  make_clean_fixture "$root"
  printf '%s\n' \
    '```text' \
    'SPACL is formally proven.' \
    '```' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified formal-proof claim"
}

test_spacl_claim_guard_allow_next_line_with_rationale_passes() {
  local root="$FIXTURE_ROOT/spacl-claim-guard-allow-next-line"
  local output

  make_clean_fixture "$root"
  printf '%s\n' \
    '<!-- public-doc-claim-guard: allow-next-line because: regression fixture intentionally exercises the guard -->' \
    'SPACL is formally proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  if ! output="$(bash "$GUARD" --root "$root" 2>&1)"; then
    fail "narrow claim guard allowlist should pass; output: $output"
  fi
}

test_spacl_claim_guard_allow_next_line_requires_rationale() {
  local root="$FIXTURE_ROOT/spacl-claim-guard-invalid-allow-next-line"

  make_clean_fixture "$root"
  printf '%s\n' \
    '<!-- public-doc-claim-guard: allow-next-line -->' \
    'SPACL is formally proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "invalid claim guard directive: docs/SPACL_ALGORITHM.md -> expected allow-next-line directive with rationale"
}

test_spacl_claim_guard_does_not_allow_wrapped_following_line() {
  local root="$FIXTURE_ROOT/spacl-claim-guard-wrapped-following-line"

  make_clean_fixture "$root"
  printf '%s\n' \
    '<!-- public-doc-claim-guard: allow-next-line because: regression fixture intentionally exercises the guard -->' \
    'SPACL is formally' \
    'proven.' \
    > "$root/docs/SPACL_ALGORITHM.md"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: docs/SPACL_ALGORITHM.md -> unqualified formal-proof claim"
}

test_spacl_self_tuning_crate_claim_fails() {
  local root="$FIXTURE_ROOT/spacl-self-tuning-crate-claim"

  make_clean_fixture "$root"
  printf '%s\n' '//! Evolutionary optimization provides self-tuning parameters.' \
    > "$root/src/lib.rs"
  assert_guard_fails_with "$root" \
    "overstated SPACL claim: src/lib.rs -> active self-tuning claim"
}

test_clean_public_docs_pass
test_missing_required_doc_fails
test_missing_required_agent_doc_fails
test_missing_feedback_experiment_spec_fails
test_missing_local_link_fails
test_missing_benchmark_link_fails
test_missing_line_suffixed_link_fails
test_missing_code_span_artifact_fails
test_missing_experiment_artifact_fails
test_missing_plaintext_artifact_fails
test_host_local_paths_fail
test_non_public_paper_paths_fail
test_non_public_paper_directory_fails
test_non_public_paper_directory_without_slash_fails
test_non_public_path_in_legacy_docs_subtree_fails
test_spacl_novelty_claim_fails
test_spacl_dynamic_runtime_claim_fails
test_spacl_dynamic_gate_claim_fails
test_spacl_active_tuner_claim_fails
test_spacl_formal_property_claim_fails
test_spacl_formally_proven_claim_fails
test_spacl_negated_novelty_claim_passes
test_spacl_positive_novelty_after_negation_fails
test_spacl_positive_novelty_before_negation_fails
test_spacl_negated_formal_proof_claim_passes
test_spacl_line_wrapped_formal_proof_claim_fails
test_spacl_line_wrapped_rust_doc_claim_fails
test_spacl_fenced_formal_proof_claim_fails
test_spacl_claim_guard_allow_next_line_with_rationale_passes
test_spacl_claim_guard_allow_next_line_requires_rationale
test_spacl_claim_guard_does_not_allow_wrapped_following_line
test_spacl_self_tuning_crate_claim_fails
echo "PASS: check_public_docs regression tests"
