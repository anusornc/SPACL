#!/usr/bin/env python3
"""PROTOTYPE: frozen parallel-kernel diagnostic, never adaptive promotion."""
import argparse
import itertools
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import statistics
import subprocess

from prototype_parallel_value_oracle import cases, evaluate_case, validate_witness
from prototype_spacl_feedback import canonical, digest, file_hash, invoke_probe, unique_object

ROOT = Path(__file__).resolve().parents[1]
MODES = ("serial_direct", "serial_frontier", "parallel_frontier")
REPEATS = 7
SEED = 20260906
SOURCES = (
    "examples/prototype_spacl_parallel_value.rs",
    "src/reasoner/qualified_consistency.rs",
    "scripts/run_spacl_parallel_value.py",
    "scripts/prototype_parallel_value_oracle.py",
    "scripts/prototype_spacl_feedback.py",
    "docs/specs/SPACL_PARALLEL_VALUE_PROBE.md",
    "Cargo.toml", "Cargo.lock",
)


class GateError(ValueError):
    def __init__(self, message, classification="semantic_failure", gate=None):
        super().__init__(message)
        self.classification = classification
        # Validation labels are fixed; callers with dynamic diagnostics supply
        # a stable gate explicitly, independently of result classification.
        self.gate = gate or re.sub(r"[^a-z0-9]+", "_", message.lower()).strip("_")


def require(condition, message, classification="semantic_failure", gate=None):
    if not condition:
        raise GateError(message, classification, gate)


def read_json(path):
    return json.loads(Path(path).read_text(), object_pairs_hook=unique_object)


def cpu_pair():
    allowed = sorted(os.sched_getaffinity(0))
    selected, topology, seen = [], {}, set()
    for cpu in allowed:
        base = Path(f"/sys/devices/system/cpu/cpu{cpu}/topology")
        try:
            key = (base.joinpath("physical_package_id").read_text().strip(),
                   base.joinpath("core_id").read_text().strip())
        except OSError:
            key = ("unknown", str(cpu))
        topology[str(cpu)] = list(key)
        if key not in seen and len(selected) < 2:
            selected.append(cpu)
            seen.add(key)
    require(len(selected) == 2, "two allowed cores are required", "inconclusive")
    return selected, {str(cpu): topology[str(cpu)] for cpu in selected}


def request(case, mode):
    return {"case": case, "mode": mode, "workers": 2,
            "warmups": 0 if mode == "audit" else 1}


def expected_prefixes(case, order):
    depth = min(2, case["vertices"])
    edges = {tuple(edge) for edge in case["edges"]}
    prefixes = []
    for assignment in itertools.product(range(case["colors"]), repeat=depth):
        prefix = [None] * case["vertices"]
        for vertex, color in zip(order[:depth], assignment):
            prefix[vertex] = color
        if all(prefix[a] is None or prefix[b] is None or prefix[a] != prefix[b]
               for a, b in edges):
            prefixes.append(prefix)
    return prefixes


def check_response(case, response, mode):
    require(response.get("status") == "success",
            f'{case["id"]}/{mode}: unavailable probe: {response}', "inconclusive", "probe_available")
    require(response.get("case_id") == case["id"] and response.get("mode") == mode,
            "probe identity mismatch")
    require(response.get("vertices") == case["vertices"]
            and response.get("colors") == case["colors"], "probe shape mismatch")
    oracle = evaluate_case(case)
    require(oracle["status"] in ("sat", "not_sat"), "invalid oracle input")
    expected = oracle["expected"]
    require(response.get("verdict", {}).get("result") ==
            ("consistent" if expected else "inconsistent"), "oracle verdict mismatch")
    require(response.get("verdict", {}).get("error_class") is None,
            "successful result contains an error")
    if expected:
        require(validate_witness(case, response.get("witness")), "invalid result witness")
    else:
        require(response.get("witness") is None, "UNSAT result contains a witness")

    order = response.get("order")
    require(isinstance(order, list) and all(type(v) is int for v in order)
            and sorted(order) == list(range(case["vertices"])), "invalid vertex order")
    tasks = response.get("tasks")
    require(isinstance(tasks, list), "missing task evidence")
    require(type(response.get("tasks_started")) is int
            and response["tasks_started"] == response.get("tasks_completed") == len(tasks),
            "unfinished or unaccounted task")
    trivial = case["vertices"] <= case["colors"]
    prefixes = [] if trivial else (
        [[None] * case["vertices"]] if mode in ("audit", "serial_direct")
        else expected_prefixes(case, order))
    require([task.get("prefix") for task in tasks] == prefixes, "frontier coverage/order mismatch")
    logical = []
    for index, task in enumerate(tasks):
        require(type(task.get("task_id")) is int and task["task_id"] == index,
                "task ID mismatch")
        branch_oracle = evaluate_case(case, task["prefix"])
        require(branch_oracle["status"] in ("sat", "not_sat"), "invalid branch oracle")
        require(type(task.get("result")) is bool
                and task["result"] == branch_oracle["expected"], "branch oracle mismatch")
        require(type(task.get("search_nodes")) is int and 0 < task["search_nodes"] <= 200_000,
                "invalid search work")
        if task["result"]:
            require(validate_witness(case, task.get("witness"), task["prefix"]),
                    "invalid branch witness")
        else:
            require(task.get("witness") is None, "UNSAT branch contains witness")
        worker = task.get("worker_id")
        if mode == "parallel_frontier":
            require(type(worker) is int and 0 <= worker < 2, "invalid parallel worker ID")
        else:
            require(worker is None, "serial task claims a worker")
        logical.append({key: task[key] for key in
                        ("task_id", "prefix", "result", "search_nodes", "witness")})
    require(response.get("search_nodes") == sum(t["search_nodes"] for t in tasks),
            "aggregate work mismatch")
    require((True if trivial else any(t["result"] for t in tasks)) == expected,
            "branch reduction mismatch")
    used = sorted({t["worker_id"] for t in tasks if t.get("worker_id") is not None})
    require(response.get("workers_used") == used and response.get("workers_used_count") == len(used),
            "worker accounting mismatch")
    if mode == "audit":
        observations = response.get("public_audit")
        require(isinstance(observations, list) and len(observations) == 3,
                "missing original runtime audit")
        require({(row["reasoner"], row["scheduling_mode"]) for row in observations} == {
            ("SimpleReasoner", None), ("SpeculativeTableauxReasoner", "sequential"),
            ("SpeculativeTableauxReasoner", "always_parallel")}, "audit mode coverage mismatch")
        for observation in observations:
            require(observation.get("status") == "ok" and type(observation.get("result")) is bool,
                    "original runtime unavailable", "inconclusive")
            require(observation["result"] == expected, "original runtime oracle mismatch")
            if observation["reasoner"] == "SpeculativeTableauxReasoner":
                require(observation["actual_workers_started"] == observation["actual_workers_completed"],
                        "original runtime did not drain workers")
    else:
        require(type(response.get("reasoning_ns")) is int and response["reasoning_ns"] > 0,
                "zero/invalid measured duration", "inconclusive")
        warmups = response.get("warmup_rows")
        require(isinstance(warmups, list) and len(warmups) == 1, "missing warm-up")
        warmup = warmups[0]
        require(warmup.get("result") == expected
                and warmup.get("tasks_started") == warmup.get("tasks_completed") == len(tasks),
                "warm-up semantic/lifecycle mismatch")
    return digest(logical)


def check_sources_clean(root):
    subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", "src", "scripts",
                    "examples", *SOURCES], check=True, cwd=root,
                   capture_output=True)
    subprocess.run(["git", "ls-files", "--error-unmatch", *SOURCES], check=True, cwd=root,
                   capture_output=True)


def build_manifest(binary):
    check_sources_clean(ROOT)
    cpus, topology = cpu_pair()
    locked = cases()
    return {"schema_version": 1, "protocol": "bounded_parallel_value_v1", "seed": SEED,
            "repeats": REPEATS, "warmups": 1, "workers": 2, "prefix_depth": 2,
            "timeout_seconds": 60, "cpus": cpus, "cpu_topology": topology,
            "platform": platform.platform(), "python": platform.python_version(),
            "build_profile": "release", "binary_sha256": file_hash(binary),
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"],
                                                     cwd=ROOT, text=True).strip(),
            "source_sha256": {name: file_hash(ROOT / name) for name in SOURCES},
            "cases": locked, "case_sha256": {case["id"]: digest(case) for case in locked},
            "oracle": {case["id"]: evaluate_case(case) for case in locked},
            "modes": list(MODES), "primary_cases": [c["id"] for c in locked if c["role"] == "primary"],
            "promotable": False, "timer_scope": "prepared_search_kernel_plus_frontier_dispatch_and_reduction"}


def record_order(locked):
    expected = [(case["id"], "audit", None) for case in locked]
    for case_index, case in enumerate(locked):
        for repeat in range(REPEATS):
            offset = (SEED + case_index + repeat) % len(MODES)
            expected.extend((case["id"], mode, repeat)
                            for mode in (*MODES[offset:], *MODES[:offset]))
    return expected


def aggregate_ratio(groups, primary, comparator, indexes=None):
    ratios = []
    for case_id in primary:
        paired = groups[case_id]
        use = list(range(REPEATS)) if indexes is None else indexes[case_id]
        parallel = statistics.median(paired["parallel_frontier"][i] for i in use)
        serial = statistics.median(paired[comparator][i] for i in use)
        ratios.append(parallel / serial)
    return math.exp(sum(math.log(ratio) for ratio in ratios) / len(ratios))


def failure_gates(rows, comparisons, primary):
    failures = []
    for row in rows:
        if row["case_id"] not in primary:
            continue
        if row["two_worker_samples"] < 4 or row["nontrivial_work_samples"] < 4:
            failures.append({"gate": "primary_work_gate", "case_id": row["case_id"],
                             "two_worker_samples": row["two_worker_samples"],
                             "nontrivial_work_samples": row["nontrivial_work_samples"],
                             "minimum_samples": 4})
    for comparator in MODES[:2]:
        result = comparisons[comparator]
        if result["ratio"] > .95:
            failures.append({"gate": f"{comparator}_ratio_above_0_95", "observed": result["ratio"]})
        if result["upper_95"] >= 1:
            failures.append({"gate": f"{comparator}_upper_interval_not_below_1",
                             "observed": result["upper_95"]})
    return failures


def summarize(manifest, records):
    require(manifest.get("cases") == cases(), "frozen corpus was changed")
    require(manifest.get("modes") == list(MODES) and manifest.get("repeats") == REPEATS
            and manifest.get("seed") == SEED and manifest.get("workers") == 2
            and manifest.get("warmups") == 1 and manifest.get("prefix_depth") == 2
            and manifest.get("timeout_seconds") == 60,
            "protocol configuration changed")
    require(manifest.get("primary_cases") == [c["id"] for c in cases() if c["role"] == "primary"],
            "primary cases were changed")
    require(manifest.get("case_sha256") == {c["id"]: digest(c) for c in cases()},
            "case identity changed")
    require(manifest.get("oracle") == {c["id"]: evaluate_case(c) for c in cases()},
            "oracle evidence changed")
    actual_order = [(r.get("case_id"), r.get("mode"), r.get("repeat")) for r in records]
    require(actual_order == record_order(manifest["cases"])[:len(records)],
            "frozen observation order changed")
    case_map = {case["id"]: case for case in manifest["cases"]}
    audits, samples, trace_by_case = {}, {}, {}
    for record in records:
        case_id = record.get("case_id")
        require(case_id in case_map, "unexpected case")
        mode = record.get("mode")
        require(mode in ("audit", *MODES), "unexpected mode")
        trace = check_response(case_map[case_id], record["response"], mode)
        if mode == "audit":
            require(case_id not in audits, "duplicate audit")
            audits[case_id] = record["response"]
        else:
            repeat = record.get("repeat")
            require(type(repeat) is int and 0 <= repeat < REPEATS, "bad repeat")
            key = (case_id, mode, repeat)
            require(key not in samples, "duplicate sample")
            samples[key] = record["response"]
            if mode != "serial_direct":
                require(trace_by_case.setdefault(case_id, trace) == trace,
                        "logical frontier replay mismatch")
    require(len(audits) == len(case_map), "incomplete public audit", "inconclusive")
    require(len(samples) == len(case_map) * len(MODES) * REPEATS,
            "incomplete measurement matrix", "inconclusive")
    groups = {case_id: {mode: [samples[(case_id, mode, r)]["reasoning_ns"]
                              for r in range(REPEATS)] for mode in MODES} for case_id in case_map}
    rows = []
    for case_id, case in case_map.items():
        medians = {mode: statistics.median(groups[case_id][mode]) for mode in MODES}
        parallel = [samples[(case_id, "parallel_frontier", r)] for r in range(REPEATS)]
        rows.append({"case_id": case_id, "role": case["role"],
                     "expected": manifest["oracle"][case_id]["expected"],
                     "median_ns": medians,
                     "parallel_over_direct": medians["parallel_frontier"] / medians["serial_direct"],
                     "two_worker_samples": sum(r["workers_used_count"] == 2 for r in parallel),
                     "nontrivial_work_samples": sum(r["tasks_completed"] > 1
                         and r["search_nodes"] > 4 * r["tasks_completed"] for r in parallel)})
    primary = manifest["primary_cases"]
    work_pass = all(row["two_worker_samples"] >= 4 and row["nontrivial_work_samples"] >= 4
                    for row in rows if row["case_id"] in primary)
    rng = random.Random(SEED)
    intervals = {}
    for comparator in MODES[:2]:
        draws = []
        for _ in range(1000):
            indexes = {case_id: [rng.randrange(REPEATS) for _ in range(REPEATS)] for case_id in primary}
            draws.append(aggregate_ratio(groups, primary, comparator, indexes))
        draws.sort()
        intervals[comparator] = {"ratio": aggregate_ratio(groups, primary, comparator),
                                 "lower_95": draws[24], "upper_95": draws[974]}
    benefit = all(result["ratio"] <= .95 and result["upper_95"] < 1 for result in intervals.values())
    classification = ("inconclusive" if not work_pass else
                      "promising_kernel_signal" if benefit else "not_demonstrated")
    return {"classification": classification, "semantic_gate": "pass", "logical_replay_gate": "pass",
            "work_gate": work_pass, "primary_comparisons": intervals, "cases": rows,
            "failed_gates": failure_gates(rows, intervals, primary),
            "samples": len(samples), "public_audits": len(audits) * 3,
            "frontier_trace_sha256": trace_by_case, "promotable": False,
            "deployed_spacl_runtime_claim": False, "adaptive_benefit_claim": False}


def write_summary(output, summary):
    output.joinpath("summary.json").write_bytes(canonical(summary))
    lines = ["# Parallel-value diagnostic", "", f'Result: `{summary["classification"]}`.', "",
             "This is a prepared search-kernel diagnostic, not deployed SPACL or adaptive performance.", ""]
    if "error" in summary:
        lines += ["A validity gate stopped the run; exact diagnostics are retained in summary.json and records.jsonl.", ""]
    else:
        lines += ["| Case | Role | Parallel / direct serial | Two-worker samples |",
                  "| --- | --- | ---: | ---: |"]
        for row in summary["cases"]:
            lines.append(f'| {row["case_id"]} | {row["role"]} | {row["parallel_over_direct"]:.3f} | {row["two_worker_samples"]}/7 |')
        lines += ["", "Primary paired-bootstrap comparisons:", ""]
        for comparator, result in summary["primary_comparisons"].items():
            lines.append(f'- Versus `{comparator}`: ratio {result["ratio"]:.3f}, '
                         f'95% interval [{result["lower_95"]:.3f}, {result["upper_95"]:.3f}].')
    if summary["failed_gates"]:
        lines += ["", "Failed gates:", ""]
        for failure in summary["failed_gates"]:
            lines.append(f'- `{failure["gate"]}`' +
                         (f' ({failure["case_id"]})' if "case_id" in failure else ""))
    lines += ["", "All locked real-corpus Phase 1 gates remain unchanged. No adaptive promotion."]
    output.joinpath("summary.md").write_text("\n".join(lines) + "\n")


def replay(directory):
    manifest = read_json(directory / "manifest.json")
    integrity = read_json(directory / "integrity.json")
    for name, expected in integrity.items():
        require(name in ("manifest.json", "records.jsonl", "summary.json"), "unexpected integrity path")
        require(file_hash(directory / name) == expected, f"altered {name}")
    require(set(integrity) == {"manifest.json", "records.jsonl", "summary.json"}, "incomplete integrity record")
    records = [json.loads(line, object_pairs_hook=unique_object)
               for line in (directory / "records.jsonl").read_text().splitlines()]
    summary = summarize(manifest, records)
    require(summary == read_json(directory / "summary.json"), "summary replay mismatch")
    return {"status": "pass", "manifest_sha256": integrity["manifest.json"],
            "records_sha256": integrity["records.jsonl"], "summary_sha256": digest(summary),
            "samples": summary["samples"], "classification": summary["classification"]}


def run(binary, output):
    binary = binary.resolve()
    manifest = build_manifest(binary)
    output.mkdir(parents=True, exist_ok=False)
    output.joinpath("manifest.json").write_bytes(canonical(manifest))
    records = []

    def observe(case, mode, repeat=None):
        response = invoke_probe(binary, request(case, mode), manifest["cpus"], manifest["timeout_seconds"])
        record = {"case_id": case["id"], "mode": mode, "repeat": repeat, "response": response}
        records.append(record)
        with output.joinpath("records.jsonl").open("ab") as stream:
            stream.write(canonical(record))
        check_response(case, response, mode)

    try:
        for case in manifest["cases"]:
            observe(case, "audit")
        for case_index, case in enumerate(manifest["cases"]):
            for repeat in range(REPEATS):
                offset = (SEED + case_index + repeat) % len(MODES)
                for mode in (*MODES[offset:], *MODES[:offset]):
                    observe(case, mode, repeat)
        summary = summarize(manifest, records)
    except GateError as error:
        summary = {"classification": error.classification, "error": str(error),
                   "failed_gates": [{"gate": error.gate, "detail": str(error)}],
                   "records_retained": len(records), "promotable": False}
    write_summary(output, summary)
    output.joinpath("integrity.json").write_bytes(canonical({
        name: file_hash(output / name) for name in ("manifest.json", "records.jsonl", "summary.json")}))
    if "error" not in summary:
        output.joinpath("replay_report.json").write_bytes(canonical(replay(output)))
    print(json.dumps(summary, sort_keys=True))
    return 1 if "error" in summary else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    execute = sub.add_parser("run")
    execute.add_argument("--binary", type=Path, required=True)
    execute.add_argument("--output", type=Path, required=True)
    audit = sub.add_parser("replay")
    audit.add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.command == "replay":
        print(json.dumps(replay(args.directory), sort_keys=True))
        return 0
    return run(args.binary, args.output)


if __name__ == "__main__":
    raise SystemExit(main())
