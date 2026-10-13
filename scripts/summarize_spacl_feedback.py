#!/usr/bin/env python3
"""Derive per-case spread and geometric-mean ratios from retained Phase 1 rows."""
import argparse
import csv
import hashlib
import json
import math
import statistics
from fractions import Fraction
from pathlib import Path

MODES = ("sequential", "parallel", "static", "best_fixed", "feedback")


def percentile(values, fraction):
    values = sorted(values)
    position = (len(values) - 1) * fraction
    lo = int(position)
    hi = min(lo + 1, len(values) - 1)
    return values[lo] + (values[hi] - values[lo]) * (position - lo)


def summarize(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    result = json.loads((directory / "summary.json").read_text())
    with (directory / "raw_results.csv").open() as stream:
        rows = [row for row in csv.DictReader(stream) if row["warmup"] == "False"]
    cases = [case for case in manifest["cases"] if case["split"] == "evaluation"]
    output = []
    ratios = {"static": [], "best_fixed": []}
    coverage = {"no_op_case_ids": [], "decision_flipped_case_ids": [],
                "unavailable_case_ids": [c["id"] for c in manifest["cases"]
                                         if c.get("census", {}).get("status") != "success"],
                "excluded_invalid_case_ids": []}
    for case in cases:
        groups = {mode: [row for row in rows if row["case_id"] == case["id"]
                         and row["mode"] == mode] for mode in MODES}
        valid = {mode: len(group) == manifest["repeats"] and all(
            row["status"] == "success" and int(row["reasoning_ns"]) > 0 for row in group)
                 for mode, group in groups.items()}
        medians = {mode: statistics.median(int(row["reasoning_ns"]) for row in group)
                   for mode, group in groups.items() if valid[mode]}
        for mode, group in groups.items():
            row = {"case_id": case["id"], "corpus": case["corpus"], "mode": mode,
                   "status": "complete" if valid[mode] else "incomplete",
                   "successful_repeats": sum(r["status"] == "success" for r in group),
                   "required_repeats": manifest["repeats"]}
            if valid[mode]:
                times = [int(r["reasoning_ns"]) for r in group]
                row.update(median_reasoning_ns=medians[mode],
                           p25_reasoning_ns=percentile(times, 0.25),
                           p75_reasoning_ns=percentile(times, 0.75),
                           p95_reasoning_ns=percentile(times, 0.95),
                           median_wall_ns=statistics.median(int(r["wall_ns"]) for r in group),
                           parallel_repeats=sum(r["used_parallel"] == "True" for r in group))
                if valid["sequential"] and valid["parallel"]:
                    row["regret"] = medians[mode] / min(medians["sequential"], medians["parallel"])
                for comparator in ratios:
                    if valid[comparator]:
                        row["ratio_vs_" + comparator] = medians[mode] / medians[comparator]
            output.append(row)
        if all(valid.values()):
            features = case["census"]["features"]
            config = manifest["config"]
            gates = [features["disjunctions"] > 0
                     and features["branches"] >= config["parallel_threshold"]
                     and features["cost_us"] >= features["workers"] * max(
                         1, math.ceil(config["cost_threshold_us"] * Fraction(candidate)))
                     for candidate in manifest["ladder"]]
            if len(set(gates)) == 1:
                coverage["no_op_case_ids"].append(case["id"])
            if groups["feedback"][0]["used_parallel"] != groups["static"][0]["used_parallel"]:
                coverage["decision_flipped_case_ids"].append(case["id"])
            for comparator in ratios:
                ratios[comparator].append(medians["feedback"] / medians[comparator])
        else:
            coverage["excluded_invalid_case_ids"].append(case["id"])
    columns = ("case_id", "corpus", "mode", "status", "successful_repeats", "required_repeats",
               "median_reasoning_ns", "p25_reasoning_ns", "p75_reasoning_ns", "p95_reasoning_ns",
               "median_wall_ns", "parallel_repeats", "regret", "ratio_vs_static", "ratio_vs_best_fixed")
    with (directory / "per_case.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=columns)
        writer.writeheader()
        writer.writerows(output)
    hashes = {name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
              for name in ("manifest.json", "raw_results.csv", "feedback_tape.jsonl", "summary.json")}
    aggregate = {"classification": result["classification"], "source_hashes": hashes,
                 "summarizer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                 **coverage,
                 "complete_cases": len(ratios["static"]), "required_cases": len(cases),
                 "feedback_geometric_mean_ratio": {
                     key: math.exp(statistics.mean(map(math.log, values))) if values else None
                     for key, values in ratios.items()}}
    (directory / "descriptive_statistics.json").write_text(
        json.dumps(aggregate, sort_keys=True, indent=2, allow_nan=False) + "\n")
    lines = ["# Held-out per-case results", "",
             "Descriptive statistics; the locked promotion decision is in summary.json.",
             "Timing units below are milliseconds. Ratios below one favor feedback.", "",
             "| Case | Feedback median | Static median | Best-fixed median | Feedback/static | Feedback/fixed |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for case in cases:
        group = {row["mode"]: row for row in output if row["case_id"] == case["id"]}
        feedback = group["feedback"]
        if all(group[mode]["status"] == "complete" for mode in MODES):
            values = [group[mode]["median_reasoning_ns"] / 1_000_000
                      for mode in ("feedback", "static", "best_fixed")]
            lines.append(f'| {case["id"]} | {values[0]:.4f} | {values[1]:.4f} | {values[2]:.4f} | '
                         f'{feedback["ratio_vs_static"]:.4f} | {feedback["ratio_vs_best_fixed"]:.4f} |')
        else:
            lines.append(f'| {case["id"]} | incomplete | incomplete | incomplete | unavailable | unavailable |')
    lines += ["",
              "No-op cases: " + (", ".join(coverage["no_op_case_ids"]) or "none") + ".",
              "Decision-flipped cases: " + (", ".join(coverage["decision_flipped_case_ids"]) or "none") + ".",
              "Unavailable cases: " + (", ".join(coverage["unavailable_case_ids"]) or "none") + ".",
              "Excluded invalid cases: " + (", ".join(coverage["excluded_invalid_case_ids"]) or "none") + ".",
              "", "Per-mode percentiles and wall time: per_case.csv.",
              "Geometric means and input hashes: descriptive_statistics.json.", ""]
    (directory / "per_case.md").write_text("\n".join(lines))
    return aggregate


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    print(json.dumps(summarize(args.directory), sort_keys=True))
