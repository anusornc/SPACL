#!/usr/bin/env python3
"""Generate canonical synthetic benchmark summary CSVs for paper artifacts."""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
from statistics import median
from typing import Dict, Iterable, List


SCALABILITY_SIZES = [100, 500, 1000, 5000, 10000]
SCALABILITY_SERIES = ["sequential", "spacl_adaptive"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate synthetic paper summary CSVs from saved Criterion artifacts."
    )
    parser.add_argument(
        "--criterion-root",
        default="target/criterion",
        help="Path to the Criterion output root.",
    )
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Directory to write summary CSVs into.",
    )
    parser.add_argument(
        "--metric",
        default="point_estimate",
        choices=["point_estimate", "mean", "median"],
        help="Metric to extract from Criterion estimates.json.",
    )
    parser.add_argument(
        "--run-id",
        default="",
        help="Optional run identifier recorded in the output rows.",
    )
    parser.add_argument(
        "--ablation-csv",
        action="append",
        default=[],
        help="Path(s) to synthetic ablation results.csv files used for reasoning-side summaries.",
    )
    parser.add_argument(
        "--include-workload",
        action="append",
        default=[],
        help="Workload names to include from ablation CSV inputs.",
    )
    return parser.parse_args()


def metric_value_us(estimates: Dict[str, object], metric_name: str) -> float:
    if metric_name == "point_estimate":
        return float(estimates["mean"]["point_estimate"]) / 1000.0
    return float(estimates[metric_name]["point_estimate"]) / 1000.0


def load_json(path: Path) -> Dict[str, object]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def throughput_mops(classes: int, metric_value_microseconds: float) -> float:
    if metric_value_microseconds <= 0:
        return 0.0
    return classes / metric_value_microseconds


def generate_scalability_rows(
    criterion_root: Path, metric_name: str, run_id: str
) -> Iterable[Dict[str, object]]:
    extracted: Dict[int, Dict[str, Dict[str, object]]] = {}

    for series in SCALABILITY_SERIES:
        for classes in SCALABILITY_SIZES:
            estimates_path = (
                criterion_root / "scalability" / series / str(classes) / "new" / "estimates.json"
            )
            benchmark_path = (
                criterion_root / "scalability" / series / str(classes) / "new" / "benchmark.json"
            )
            if not estimates_path.exists():
                raise FileNotFoundError(f"Missing Criterion estimates file: {estimates_path}")
            if not benchmark_path.exists():
                raise FileNotFoundError(f"Missing Criterion benchmark file: {benchmark_path}")

            estimates = load_json(estimates_path)
            metric_value = metric_value_us(estimates, metric_name)
            extracted.setdefault(classes, {})[series] = {
                "metric_value_us": metric_value,
                "raw_estimates_path": str(estimates_path),
                "raw_benchmark_path": str(benchmark_path),
            }

    for classes in SCALABILITY_SIZES:
        row_bucket = extracted[classes]
        sequential_us = row_bucket["sequential"]["metric_value_us"]
        for series in SCALABILITY_SERIES:
            current_us = row_bucket[series]["metric_value_us"]
            row = {
                "run_id": run_id,
                "benchmark_family": "scalability",
                "workload_label": str(classes),
                "classes": classes,
                "series": series,
                "metric_name": metric_name,
                "metric_value_us": f"{current_us:.6f}",
                "raw_estimates_path": row_bucket[series]["raw_estimates_path"],
                "raw_benchmark_path": row_bucket[series]["raw_benchmark_path"],
                "speedup_vs_sequential": (
                    f"{(sequential_us / current_us):.6f}" if current_us > 0 else ""
                ),
                "throughput_mops": f"{throughput_mops(classes, current_us):.6f}",
            }
            yield row


def load_ablation_rows(paths: List[Path]) -> List[Dict[str, str]]:
    rows: List[Dict[str, str]] = []
    for path in paths:
        with path.open("r", encoding="utf-8", newline="") as handle:
            reader = csv.DictReader(handle)
            for row in reader:
                row["_source_csv"] = str(path)
                rows.append(row)
    return rows


def grouped_median(values: List[float]) -> float:
    return float(median(values))


def generate_reused_conflict_rows(
    ablation_csv_paths: List[Path], include_workloads: List[str], run_id: str
) -> Iterable[Dict[str, object]]:
    if not ablation_csv_paths:
        return []

    selected = set(include_workloads or ["reused_conflict_12"])
    rows = load_ablation_rows(ablation_csv_paths)
    rows = [row for row in rows if row.get("workload") in selected]
    if not rows:
        raise ValueError("No matching reused-conflict rows found in supplied ablation CSVs.")

    grouped: Dict[tuple, List[Dict[str, str]]] = {}
    for row in rows:
        key = (
            row["workload"],
            row["mode"],
            row.get("nogood_enabled", ""),
            row.get("branch_policy", ""),
        )
        grouped.setdefault(key, []).append(row)

    result_rows = []
    for key, bucket in sorted(grouped.items()):
        workload, mode, nogood_enabled, branch_policy = key
        wall_values = [float(item["wall_time_ms"]) for item in bucket]
        reasoning_values = [float(item.get("reasoning_time_ms") or 0) for item in bucket]
        nogood_hits = [float(item.get("nogood_hits") or 0) for item in bucket]
        branches_pruned = [float(item.get("branches_pruned") or 0) for item in bucket]
        branches_created = [float(item.get("branches_created") or 0) for item in bucket]
        work_items_expanded = [float(item.get("work_items_expanded") or 0) for item in bucket]
        local_cache_hits = [float(item.get("local_cache_hits") or 0) for item in bucket]
        global_cache_hits = [float(item.get("global_cache_hits") or 0) for item in bucket]

        result_rows.append(
            {
                "run_id": run_id,
                "benchmark_family": "reused_conflict",
                "workload_label": workload,
                "mode": mode,
                "nogood_enabled": nogood_enabled,
                "branch_policy": branch_policy,
                "repeats": len(bucket),
                "median_wall_time_ms": f"{grouped_median(wall_values):.6f}",
                "median_reasoning_time_ms": f"{grouped_median(reasoning_values):.6f}",
                "median_nogood_hits": f"{grouped_median(nogood_hits):.6f}",
                "median_branches_pruned": f"{grouped_median(branches_pruned):.6f}",
                "median_branches_created": f"{grouped_median(branches_created):.6f}",
                "median_work_items_expanded": f"{grouped_median(work_items_expanded):.6f}",
                "median_local_cache_hits": f"{grouped_median(local_cache_hits):.6f}",
                "median_global_cache_hits": f"{grouped_median(global_cache_hits):.6f}",
                "source_csvs": ";".join(sorted({item["_source_csv"] for item in bucket})),
            }
        )
    return result_rows


def generate_union_ladder_rows(
    ablation_csv_paths: List[Path], include_workloads: List[str], run_id: str
) -> Iterable[Dict[str, object]]:
    if not ablation_csv_paths:
        return []

    selected = set(include_workloads or ["union_4x6", "union_6x6"])
    rows = load_ablation_rows(ablation_csv_paths)
    rows = [row for row in rows if row.get("workload") in selected]
    if not rows:
        raise ValueError("No matching union-ladder rows found in supplied ablation CSVs.")

    grouped: Dict[tuple, List[Dict[str, str]]] = {}
    for row in rows:
        key = (
            row["workload"],
            row["mode"],
            row.get("nogood_enabled", ""),
            row.get("branch_policy", ""),
        )
        grouped.setdefault(key, []).append(row)

    result_rows = []
    for key, bucket in sorted(grouped.items()):
        workload, mode, nogood_enabled, branch_policy = key
        wall_values = [float(item["wall_time_ms"]) for item in bucket]
        reasoning_values = [float(item.get("reasoning_time_ms") or 0) for item in bucket]
        disjunctions = [float(item.get("disjunctions_detected") or 0) for item in bucket]
        estimated_branch_count = [float(item.get("estimated_branch_count") or 0) for item in bucket]
        branches_created = [float(item.get("branches_created") or 0) for item in bucket]
        work_items_expanded = [float(item.get("work_items_expanded") or 0) for item in bucket]
        local_cache_hits = [float(item.get("local_cache_hits") or 0) for item in bucket]
        global_cache_hits = [float(item.get("global_cache_hits") or 0) for item in bucket]

        result_rows.append(
            {
                "run_id": run_id,
                "benchmark_family": "union_ladder",
                "workload_label": workload,
                "mode": mode,
                "nogood_enabled": nogood_enabled,
                "branch_policy": branch_policy,
                "repeats": len(bucket),
                "median_wall_time_ms": f"{grouped_median(wall_values):.6f}",
                "median_reasoning_time_ms": f"{grouped_median(reasoning_values):.6f}",
                "median_disjunctions_detected": f"{grouped_median(disjunctions):.6f}",
                "median_estimated_branch_count": f"{grouped_median(estimated_branch_count):.6f}",
                "median_branches_created": f"{grouped_median(branches_created):.6f}",
                "median_work_items_expanded": f"{grouped_median(work_items_expanded):.6f}",
                "median_local_cache_hits": f"{grouped_median(local_cache_hits):.6f}",
                "median_global_cache_hits": f"{grouped_median(global_cache_hits):.6f}",
                "source_csvs": ";".join(sorted({item["_source_csv"] for item in bucket})),
            }
        )
    return result_rows


def write_csv(path: Path, rows: Iterable[Dict[str, object]]) -> None:
    rows = list(rows)
    ensure_parent(path)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


def main() -> int:
    args = parse_args()
    criterion_root = Path(args.criterion_root)
    output_dir = Path(args.output_dir)

    scalability_rows = list(
        generate_scalability_rows(criterion_root, args.metric, args.run_id)
    )
    write_csv(output_dir / "scalability_summary.csv", scalability_rows)
    print(f"Wrote {output_dir / 'scalability_summary.csv'}")

    if args.ablation_csv:
        union_ladder_rows = list(
            generate_union_ladder_rows(
                [Path(path) for path in args.ablation_csv],
                args.include_workload,
                args.run_id,
            )
        )
        if union_ladder_rows:
            write_csv(output_dir / "union_ladder_summary.csv", union_ladder_rows)
            print(f"Wrote {output_dir / 'union_ladder_summary.csv'}")

        reused_conflict_rows = list(
            generate_reused_conflict_rows(
                [Path(path) for path in args.ablation_csv],
                args.include_workload,
                args.run_id,
            )
        )
        if reused_conflict_rows:
            write_csv(output_dir / "reused_conflict_summary.csv", reused_conflict_rows)
            print(f"Wrote {output_dir / 'reused_conflict_summary.csv'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
