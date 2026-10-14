#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "Usage: scripts/check_public_docs.sh [--root PATH]" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      ROOT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ ! -d "$ROOT_DIR" ]]; then
  echo "FAIL: root directory does not exist: $ROOT_DIR" >&2
  exit 2
fi

ROOT_DIR="$(cd "$ROOT_DIR" && pwd -P)"
fail=0

required_docs=(
  "CONTEXT.md"
  "docs/adr/0001-official-oracle-first-benchmark-contract.md"
  "docs/adr/0002-three-lane-w3c-full-corpus-gate.md"
  "docs/adr/0003-unified-structural-owl-expression-model.md"
  "docs/adr/0004-model-based-direct-semantics-entailment.md"
  "docs/adr/0005-dual-semantic-views-for-functional-w3c-rows.md"
  "docs/adr/0006-owned-rdf-graph-boundary-for-owl-full.md"
  "docs/adr/0009-deterministic-feedback-threshold-boundary.md"
  "docs/agents/domain.md"
  "docs/agents/issue-tracker.md"
  "docs/agents/triage-labels.md"
  "docs/SPACL_ALGORITHM.md"
  "docs/specs/OWL2_BENCHMARK_CONFORMANCE.md"
  "docs/specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md"
)

for path in "${required_docs[@]}"; do
  if [[ ! -f "$ROOT_DIR/$path" ]]; then
    echo "FAIL: missing required doc: $path" >&2
    fail=1
  fi
done

if ! python3 - "$ROOT_DIR" <<'PY'
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

root = Path(sys.argv[1]).resolve()
# Historical snapshots are preserved verbatim but are outside the active
# public-documentation surface checked before release.
excluded = {
    Path("docs/archive"),
}


def git_ignored_set(relatives):
    # Git-ignored scratch files (local templates, drafts) are not part
    # of the public documentation surface; a non-repo root (release
    # test harness) simply ignores nothing.
    try:
        proc = subprocess.run(
            ["git", "-C", str(root), "check-ignore", "--stdin", "-z", "--"],
            input="\0".join(relatives),
            capture_output=True,
            text=True,
        )
    except OSError:
        return set()
    if proc.returncode not in (0, 1):
        return set()
    return {entry for entry in proc.stdout.split("\0") if entry}
inline_link = re.compile(
    r"!?\[[^\]\n]*\]\((?P<target><[^>\n]+>|[^\s)]+)(?:\s+[^)]*)?\)"
)
reference_link = re.compile(r"^\s{0,3}\[[^\]\n]+\]:\s*(?P<target><[^>\n]+>|\S+)")
artifact_reference = re.compile(
    r"(?P<target>benchmarks/[A-Za-z0-9_./+${}<>*-]+)"
)
host_path = re.compile(
    r"(?P<path>/(?:home|Users)/[^\s`<>()\],;]+|/tmp/tableauxx-[^\s`<>()\],;]+)"
)
paper_path = re.compile(
    r"(?P<path>paper/submission(?:_rrn)?(?:/[^\s`<>()\],;]*)?)(?![A-Za-z0-9_-])"
)
claim_surfaces = (
    Path("README.md"),
    Path("docs/SPACL_ALGORITHM.md"),
    Path("docs/MODULE_ARCHITECTURE_EXP_HYBRID_RRN.md"),
    Path("docs/adr/0009-deterministic-feedback-threshold-boundary.md"),
    Path("docs/specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md"),
    Path("src/lib.rs"),
    Path("src/lib_original.rs"),
    Path("src/reasoner/speculative.rs"),
)
claim_rules = (
    (
        re.compile(
            r"(?:\bSPACL\b[^.?!]{0,120}\bnovel\b|\bnovel\b[^.?!]{0,80}\bSPACL\b)",
            re.IGNORECASE,
        ),
        "unqualified novelty claim",
    ),
    (re.compile(r"\bfeature-complete\b", re.IGNORECASE), "feature-completeness claim"),
    (
        re.compile(r"\bcomplete OWL2 DL support\b", re.IGNORECASE),
        "complete OWL2 DL claim",
    ),
    (
        re.compile(
            r"(?:"
            r"\bdynamically adjust(?:s|ed|ing)?\b[^.?!]{0,160}(?:parallelism|threshold)"
            r"|\bdynamically (?:decides?|selects?)\b[^.?!]{0,160}(?:parallel|schedul)"
            r")",
            re.IGNORECASE,
        ),
        "active dynamic-tuning claim",
    ),
    (
        re.compile(
            r"\buses?\s+(?:an\s+)?`?AdaptiveTuner`?\b[^.?!]{0,160}"
            r"(?:adjust|observed speedup)",
            re.IGNORECASE,
        ),
        "active AdaptiveTuner claim",
    ),
    (
        re.compile(r"\bself-tuning parameters?\b", re.IGNORECASE),
        "active self-tuning claim",
    ),
    (
        re.compile(r"\bSPACL\s+is\s+(?:\*\*)?sound(?:\*\*)?\b", re.IGNORECASE),
        "unqualified soundness claim",
    ),
    (
        re.compile(r"\bSPACL\s+is\s+(?:\*\*)?complete(?:\*\*)?\b", re.IGNORECASE),
        "unqualified completeness claim",
    ),
    (
        re.compile(r"\bSPACL\s+(?:\*\*)?terminates(?:\*\*)?\b", re.IGNORECASE),
        "unqualified termination claim",
    ),
    (
        re.compile(r"\bSPACL\s+is\s+(?:formally\s+)?proven\b", re.IGNORECASE),
        "unqualified formal-proof claim",
    ),
    (re.compile(r"\bProof:\s*", re.IGNORECASE), "informal proof presented as proof"),
    (
        re.compile(r"\bmaintaining completeness\b", re.IGNORECASE),
        "unqualified completeness claim",
    ),
    (
        re.compile(r"\bexponential speedup\b", re.IGNORECASE),
        "unqualified exponential-speedup claim",
    ),
    (
        re.compile(r"\bO\(1\) overhead\b", re.IGNORECASE),
        "unqualified constant-overhead claim",
    ),
    (
        re.compile(r"\bactual speedup achieved\b", re.IGNORECASE),
        "unmeasured speedup claim",
    ),
)


def is_excluded(path: Path) -> bool:
    relative = path.relative_to(root)
    return any(relative == prefix or prefix in relative.parents for prefix in excluded)


def markdown_files():
    files = []
    for path in (root / "README.md", root / "CONTEXT.md"):
        if path.is_file():
            files.append(path)
    for directory_name in ("docs", "benchmarks", "formal"):
        directory = root / directory_name
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*.md")):
            if not is_excluded(path):
                files.append(path)
    ignored = git_ignored_set(str(path.relative_to(root)) for path in files)
    for path in files:
        if str(path.relative_to(root)) not in ignored:
            yield path


def normalized_target(raw: str):
    target = raw[1:-1] if raw.startswith("<") and raw.endswith(">") else raw
    target = unquote(target).replace("\\ ", " ")
    if target.startswith("#"):
        return None
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc:
        return None
    return parsed.path


def checks_artifact_references(source: Path) -> bool:
    relative = source.relative_to(root)
    return (
        relative == Path("README.md")
        or relative.parts[:2]
        in {
            ("docs", "benchmarking"),
            ("docs", "experiments"),
            ("docs", "research"),
        }
    )


def normalized_artifact_target(raw: str):
    target = raw.strip().rstrip(".,;:")
    if not target.startswith("benchmarks/") or "/results/" not in target:
        return None
    if any(marker in target for marker in ("<", ">", "*", "$", "{", "}", "...")):
        return None
    if any(character.isspace() for character in target):
        return None
    return target.rstrip("/")


missing = []
missing_artifacts = []
host_local = []
non_public_paper = []
overstated_claims = []
invalid_claim_guard_directives = []
for source in markdown_files():
    text = source.read_text(encoding="utf-8")
    in_fence = False
    for line_number, line in enumerate(text.splitlines(), start=1):
        for host_match in host_path.finditer(line):
            path = host_match.group("path").rstrip(".:")
            host_local.append((source.relative_to(root), line_number, path))
        for paper_match in paper_path.finditer(line):
            path = paper_match.group("path").rstrip(".:")
            non_public_paper.append((source.relative_to(root), line_number, path))
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if checks_artifact_references(source):
            for artifact_match in artifact_reference.finditer(line):
                target = normalized_artifact_target(artifact_match.group("target"))
                if target and not (root / target).exists():
                    missing_artifacts.append(
                        (source.relative_to(root), line_number, target)
                    )
        matches = list(inline_link.finditer(line))
        reference = reference_link.match(line)
        if reference:
            matches.append(reference)
        for match in matches:
            raw_target = match.group("target")
            target = normalized_target(raw_target)
            if not target:
                continue
            if target.startswith("/"):
                resolved = root / target.lstrip("/")
            else:
                resolved = source.parent / target
            if resolved.exists():
                continue
            line_suffix = re.sub(r":\d+(?::\d+)?$", "", str(resolved))
            if line_suffix != str(resolved) and Path(line_suffix).exists():
                continue
            missing.append(
                (source.relative_to(root), line_number, raw_target)
            )

claim_guard_directive = re.compile(
    r"^\s*<!--\s*public-doc-claim-guard:\s*allow-next-line\s+because:\s*(?P<rationale>\S.{9,})\s*-->\s*$",
    re.IGNORECASE,
)


def claim_text_for_line(line: str) -> str:
    stripped = line.strip()
    stripped = re.sub(r"^(?://!|///|//)\s?", "", stripped)
    stripped = re.sub(r"^(?:>\s*)?(?:[-*+]\s+|\d+[.)]\s+)?", "", stripped)
    return stripped.replace("`", "").replace("*", "").replace("_", "")


def allowed_claim_lines(relative: Path, lines):
    allowed = set()
    for index, line in enumerate(lines):
        if "public-doc-claim-guard:" not in line:
            continue
        match = claim_guard_directive.match(line)
        if not match:
            invalid_claim_guard_directives.append(
                (relative, index + 1, "expected allow-next-line directive with rationale")
            )
            continue
        for next_index in range(index + 1, len(lines)):
            if claim_text_for_line(lines[next_index]).strip():
                allowed.add(next_index + 1)
                break
    return allowed


def normalized_claim_text(lines):
    text_parts = []
    line_by_offset = []
    for line_number, line in enumerate(lines, start=1):
        claim_line = claim_text_for_line(line)
        if not claim_line.strip():
            text_parts.append(" ")
            line_by_offset.append(line_number)
            continue
        for character in claim_line:
            text_parts.append(character)
            line_by_offset.append(line_number)
        text_parts.append(" ")
        line_by_offset.append(line_number)
    return "".join(text_parts), line_by_offset


def is_negated_claim(text: str, match: re.Match, description: str) -> bool:
    context = match.group(0).lower()
    if description == "unqualified novelty claim":
        return any(
            phrase in context
            for phrase in (
                "spacl is not novel",
                "spacl is not a novel",
                "spacl does not claim novelty",
                "no novelty claim",
                "not a novelty claim",
            )
        )
    if description == "unqualified formal-proof claim":
        return any(
            phrase in context
            for phrase in (
                "spacl is not formally proven",
                "spacl is not proven",
                "spacl has not been formally proven",
            )
        )
    return False


def is_allowed_claim_match(text: str, match: re.Match, line_by_offset, allowlisted_lines) -> bool:
    matched_lines = {
        line_by_offset[offset]
        for offset in range(match.start(), match.end())
        if not text[offset].isspace()
    }
    return bool(matched_lines) and matched_lines <= allowlisted_lines


for relative in claim_surfaces:
    source = root / relative
    if not source.is_file():
        continue
    lines = source.read_text(encoding="utf-8").splitlines()
    allowlisted_lines = allowed_claim_lines(relative, lines)
    text, line_by_offset = normalized_claim_text(lines)
    for pattern, description in claim_rules:
        for match in pattern.finditer(text):
            if is_negated_claim(text, match, description):
                continue
            if is_allowed_claim_match(text, match, line_by_offset, allowlisted_lines):
                continue
            line_number = line_by_offset[match.start()]
            overstated_claims.append((relative, line_number, description))

for source, line_number, target in missing:
    print(
        f"FAIL: missing local link: {source} -> {target} (line {line_number})",
        file=sys.stderr,
    )

for source, line_number, target in missing_artifacts:
    print(
        f"FAIL: missing artifact reference: {source} -> {target} (line {line_number})",
        file=sys.stderr,
    )

for source, line_number, path in host_local:
    print(
        f"FAIL: forbidden host-local path: {source} -> {path} (line {line_number})",
        file=sys.stderr,
    )

for source, line_number, path in non_public_paper:
    print(
        f"FAIL: non-public paper path: {source} -> {path} (line {line_number})",
        file=sys.stderr,
    )

for source, line_number, description in overstated_claims:
    print(
        f"FAIL: overstated SPACL claim: {source} -> {description} "
        f"(line {line_number})",
        file=sys.stderr,
    )

for source, line_number, reason in invalid_claim_guard_directives:
    print(
        f"FAIL: invalid claim guard directive: {source} -> {reason} "
        f"(line {line_number})",
        file=sys.stderr,
    )

raise SystemExit(
    1
    if missing
    or missing_artifacts
    or host_local
    or non_public_paper
    or overstated_claims
    or invalid_claim_guard_directives
    else 0
)
PY
then
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  echo "FAIL: public documentation checks" >&2
  exit 1
fi

echo "PASS: public documentation checks"
