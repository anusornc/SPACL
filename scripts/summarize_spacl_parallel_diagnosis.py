#!/usr/bin/env python3
"""Independently check finite Boolean witnesses and summarize diagnostic evidence."""
import hashlib
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "benchmarks/prototypes/spacl_parallel_diagnosis"


def independent_truth(clauses):
    variables = sorted({literal["class"] for clause in clauses for literal in clause})
    assignments = []
    witness = None
    for values in itertools.product((False, True), repeat=len(variables)):
        assignment = dict(zip(variables, values))
        satisfied = [any(assignment[literal["class"]] != literal["negated"] for literal in clause)
                     for clause in clauses]
        assignments.append({"assignment": assignment, "clauses_satisfied": satisfied})
        if all(satisfied) and witness is None:
            witness = assignment
    return witness, assignments


def main():
    paths = {
        "oracle": ROOT / "boolean_oracle_final.json",
        "census": ROOT / "branch_census.json",
        "baseline": ROOT / "baseline_disjoint/summary.json",
        "cold": ROOT / "final_release_cold/summary.json",
        "warm": ROOT / "final_release_warm/summary.json",
    }
    data = {key: json.loads(path.read_text()) for key, path in paths.items()}
    checks = []
    for case in data["oracle"]["cases"]:
        expected = True
        tables = {}
        for group in case["case"]["individual_clauses"]:
            witness, table = independent_truth(group["clauses"])
            expected &= witness is not None
            tables[group["individual"]] = table
        assert expected == case["oracle"]["is_consistent"], "independent oracle disagreement"
        failures = [r["reasoner"] for r in case["observations"]
                    if r["status"] != "ok" or r["is_consistent"] != expected]
        checks.append({"case": case["case"]["name"], "expected": expected,
                       "failed_observations": len(failures),
                       "truth_tables": tables if failures else {}})
    report = {
        "semantic_gate_passed": all(c["failed_observations"] == 0 for c in checks),
        "parallel_value_established": False,
        "promotion_allowed": False,
        "boolean_oracle_independently_recomputed": True,
        "cases": checks,
        "disabled_learning_violations": {key: data[key]["disabled_learning_violations"]
                                       for key in ("baseline", "cold", "warm")},
        "timing_scope": "administrative controls only, not useful parallel search",
        "source_hashes": {key: hashlib.sha256(path.read_bytes()).hexdigest()
                          for key, path in paths.items()},
    }
    (ROOT / "diagnosis_summary.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    lines = ["# Generated parallel diagnosis", "",
             "No adaptive promotion is supported by this diagnostic.", "",
             "| Control | Mode | Learning | Baseline cold ms | Corrected cold ms | Corrected warm ms |",
             "| --- | --- | --- | ---: | ---: | ---: |"]
    by_group = {key: {(r["case"], r["mode"], r["learning"]): r for r in data[key]["groups"]}
                for key in ("baseline", "cold", "warm")}
    for group in sorted(by_group["baseline"]):
        values = [by_group[key][group]["median_reasoning_ns"] / 1_000_000
                  for key in ("baseline", "cold", "warm")]
        lines.append(f"| {group[0]} | {group[1]} | {group[2]} | " +
                     " | ".join(f"{v:.4f}" for v in values) + " |")
    lines += ["", "These medians describe the declared administrative controls only.",
              "They do not establish a speedup on semantically valid hard search problems.", "",
              "## Independent semantic gate", ""]
    for c in checks:
        lines.append(f'- {c["case"]}: expected {"SAT" if c["expected"] else "UNSAT"}; '
                     f'{c["failed_observations"]} observations fail the independent oracle.')
    lines += ["", "The full failing truth tables are retained in diagnosis_summary.json.", ""]
    (ROOT / "diagnosis_summary.md").write_text("\n".join(lines))
    print(json.dumps({"semantic_gate_passed": report["semantic_gate_passed"],
                      "promotion_allowed": False, "cases": len(checks)}))


if __name__ == "__main__":
    main()
