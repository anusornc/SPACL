#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PAPER="paper/submission/manuscript.tex"

if [[ ! -f "$PAPER" ]]; then
  echo "Error: missing $PAPER" >&2
  exit 1
fi

fail=0

check_absent() {
  local pattern="$1"
  local msg="$2"
  if rg -n "$pattern" "$PAPER" >/dev/null; then
    echo "FAIL: $msg"
    rg -n "$pattern" "$PAPER" | head -n 5
    fail=1
  else
    echo "PASS: $msg"
  fi
}

check_present_literal() {
  local literal="$1"
  local msg="$2"
  if grep -F "$literal" "$PAPER" >/dev/null; then
    echo "PASS: $msg"
  else
    echo "FAIL: $msg"
    echo "  missing literal: $literal"
    fail=1
  fi
}

check_present_regex() {
  local pattern="$1"
  local msg="$2"
  if rg -n "$pattern" "$PAPER" >/dev/null; then
    echo "PASS: $msg"
  else
    echo "FAIL: $msg"
    echo "  missing regex: $pattern"
    fail=1
  fi
}

# 1) Ban known invalid direct-comparison hierarchy claims in manuscript
check_absent "1,100\\\\\\$\\times\\\\\\$|96,000\\\\\\$\\times\\\\\\$|9,800\\\\\\$\\times\\\\\\$" "No non-equivalent hierarchy speedup claims in direct comparison"
check_absent "hierarchy_10000\.owl.*speedup|hierarchy_1000\.owl.*speedup|hierarchy_100\.owl.*speedup" "No hierarchy speedup rows in direct HermiT table"

# 2) Ensure direct comparison scope is explicit
check_present_literal "We conducted direct wall-clock comparison with HermiT on identical hardware." "Direct comparison scope is HermiT-only"

# 3) Ensure real-world table carries verified rerun values
check_present_regex "PATO\\s*&\\s*13,291\\s*&\\s*152,832\\s*&\\s*20 MB\\s*&\\s*4\\.02\\s*&\\s*9\\.09\\s*&\\s*0\\.44\\$\\\\times\\$" "Real-world PATO row matches verified values"
check_present_regex "DOID\\s*&\\s*15,660\\s*&\\s*207,054\\s*&\\s*27 MB\\s*&\\s*6\\.05\\s*&\\s*12\\.84\\s*&\\s*0\\.47\\$\\\\times\\$" "Real-world DOID row matches verified values"
check_present_regex "UBERON\\s*&\\s*45,104\\s*&\\s*647,434\\s*&\\s*93 MB\\s*&\\s*19\\.53\\s*&\\s*44\\.37\\s*&\\s*0\\.44\\$\\\\times\\$" "Real-world UBERON row matches verified values"
check_present_regex "GO\\\\_Basic\\s*&\\s*51,897\\s*&\\s*773,161\\s*&\\s*112 MB\\s*&\\s*38\\.85\\s*&\\s*61\\.70\\s*&\\s*0\\.63\\$\\\\times\\$" "Real-world GO_Basic row matches verified values"

# 4) Ensure duplicated limitation bullet removed
COUNT_TBOX=$(rg -n "TBox-Only Evaluation" "$PAPER" | wc -l | awk '{print $1}')
if [[ "$COUNT_TBOX" -eq 1 ]]; then
  echo "PASS: No duplicated TBox-only limitation"
else
  echo "FAIL: Expected exactly 1 'TBox-Only Evaluation' limitation, found $COUNT_TBOX"
  fail=1
fi

# 5) Ensure old Rust version not present
check_absent "Rust 1\.75\.0" "Old Rust version removed"

if [[ "$fail" -ne 0 ]]; then
  echo "\nIntegrity check FAILED"
  exit 1
fi

echo "\nIntegrity check PASSED"
