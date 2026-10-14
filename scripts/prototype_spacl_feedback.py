#!/usr/bin/env python3
"""PROTOTYPE: offline SPACL threshold experiment; never a production scheduler."""
import argparse
import csv
import hashlib
import json
import os
import platform
import random
import statistics
import subprocess
import sys
import time
from fractions import Fraction
from pathlib import Path

LADDER = ("1/4", "1/2", "1/1", "2/1", "4/1")
TRAIN_MODES = ("sequential", "parallel", "static", *LADDER)
EVAL_MODES = ("sequential", "parallel", "static", "best_fixed", "feedback")
ROOT = Path(__file__).resolve().parents[1]
MAX_THRESHOLD = 2**64 - 1
MAX_INPUT_BYTES = 256 * 1024 * 1024


def canonical(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True,
                       separators=(",", ":"), allow_nan=False) + "\n").encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def file_hash(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def threshold(base, candidate):
    if candidate not in LADDER or type(base) is not int or base < 0:
        raise ValueError("invalid cost threshold")
    ratio = Fraction(candidate)
    return min(MAX_THRESHOLD, max(1, (base * ratio.numerator + ratio.denominator - 1)
                                  // ratio.denominator))


def band(value):
    if type(value) is not int or value < 0:
        raise ValueError("invalid structural feature")
    return value.bit_length() - 1


def bucket(features):
    workers = features["workers"]
    if type(workers) is not int or workers <= 0:
        raise ValueError("invalid workers")
    return ":".join(map(str, (workers, band(features["disjunctions"]),
                             band(features["branches"]), band(features["cost_us"]))))


def has_features(row):
    features = row.get("features")
    return (isinstance(features, dict) and type(features.get("workers")) is int
            and features["workers"] > 0
            and all(type(features.get(k)) is int and features[k] >= 0
                    for k in ("disjunctions", "branches", "cost_us")))


def gate(features, config, candidate):
    return (features["disjunctions"] > 0
            and features["branches"] >= config["parallel_threshold"]
            and features["cost_us"] >= threshold(config["cost_threshold_us"], candidate)
            * features["workers"])


def measured(rows):
    return [r for r in rows if not r["warmup"]]


def successful(row):
    return (row["status"] == "success" and type(row["reasoning_ns"]) is int
            and row["reasoning_ns"] > 0
            and row["verdict"]["result"] in ("consistent", "inconsistent"))


def score_choice(scores):
    if not scores:
        return "1/1"
    best = min(scores.values())
    tied = [candidate for candidate in LADDER
            if candidate in scores and scores[candidate] <= best * Fraction(105, 100)]
    return "1/1" if "1/1" in tied else max(tied, key=Fraction)


def rational(value):
    value = Fraction(value)
    return {"numerator": value.numerator, "denominator": value.denominator}


def choice_evidence(scores):
    minimum = min(scores.values())
    tied = [candidate for candidate in LADDER
            if scores[candidate] <= minimum * Fraction(105, 100)]
    return {"scores": {c: rational(score) for c, score in scores.items()},
            "tie_set": tied,
            "tie_break_reason": "prefer_static" if "1/1" in tied else
            "prefer_less_parallel" if len(tied) > 1 else "minimum_regret"}


def complete_cases(manifest, rows, split):
    result = {}
    modes = TRAIN_MODES if split == "calibration" else EVAL_MODES
    for case in manifest["cases"]:
        if case["split"] != split:
            continue
        found = [r for r in measured(rows)
                 if r["case_id"] == case["id"] and r["split"] == split]
        expected = {(mode, repeat) for mode in modes
                    for repeat in range(manifest["repeats"])}
        actual = {(r["mode"], r["repeat"]) for r in found}
        if len(found) == len(expected) and actual == expected and all(map(successful, found)):
            result[case["id"]] = found
    return result


def fit(manifest, rows):
    """Fit only complete calibration blocks; any incomplete training falls back."""
    groups = complete_cases(manifest, rows, "calibration")
    state = {"schema_version": 1, "manifest_sha256": digest(manifest),
             "global_multiplier": "1/1", "buckets": {}, "training_complete": False,
             "selection_evidence": {"buckets": {}, "global": None}}
    required = [c for c in manifest["cases"] if c["split"] == "calibration"]
    if not required or len(groups) != len(required):
        return state
    regrets = {}
    by_bucket = {}
    for case_id, case_rows in groups.items():
        times = {mode: statistics.median(Fraction(r["reasoning_ns"])
                 for r in case_rows if r["mode"] == mode) for mode in TRAIN_MODES}
        denominator = min(times["sequential"], times["parallel"])
        regrets[case_id] = {candidate: times[candidate] / denominator
                            for candidate in LADDER}
        by_bucket.setdefault(bucket(case_rows[0]["features"]), []).append(case_id)
    for key, cases in sorted(by_bucket.items()):
        scores = {
            candidate: statistics.median(regrets[c][candidate] for c in cases)
            for candidate in LADDER}
        state["buckets"][key] = score_choice(scores)
        state["selection_evidence"]["buckets"][key] = choice_evidence(scores)
    global_scores = {
        candidate: statistics.median(r[candidate] for r in regrets.values())
        for candidate in LADDER}
    state["global_multiplier"] = score_choice(global_scores)
    state["selection_evidence"]["global"] = choice_evidence(global_scores)
    state["training_complete"] = True
    return state


def select(state, manifest, features):
    """Return a decision without mutating state; incompatible state uses default."""
    valid = (isinstance(state, dict) and state.get("schema_version") == 1
             and state.get("manifest_sha256") == digest(manifest)
             and state.get("global_multiplier") in LADDER
             and isinstance(state.get("buckets"), dict)
             and type(state.get("training_complete")) is bool
             and (state["training_complete"] or not state["buckets"])
             and all(v in LADDER for v in state.get("buckets", {}).values()))
    if not valid:
        return {"candidate": "1/1", "reason": "invalid_state"}
    key = bucket(features)
    return {"candidate": state["buckets"].get(key, "1/1"),
            "reason": "trained_bucket" if key in state["buckets"] else "unseen_bucket"}


def decision_events(manifest, rows):
    state = fit(manifest, rows)
    event = {"type": "decision", "phase": "freeze", "state": state,
             "bucket": "global", "candidate": state["global_multiplier"],
             "selection_evidence": state["selection_evidence"]["global"],
             "state_before_sha256": digest({}), "state_after_sha256": digest(state)}
    event["decision_sha256"] = digest(event)
    decisions = [event]
    for key, candidate in sorted(state["buckets"].items()):
        event = {"type": "decision", "phase": "trained_bucket", "bucket": key,
                 "candidate": candidate,
                 "selection_evidence": state["selection_evidence"]["buckets"][key],
                 "state_before_sha256": digest(state), "state_after_sha256": digest(state)}
        event["decision_sha256"] = digest(event)
        decisions.append(event)
    for row in rows:
        if row["split"] == "evaluation" and row["mode"] == "feedback" and has_features(row):
            choice = select(state, manifest, row["features"])
            event = {"type": "decision", "phase": "evaluation", "case_id": row["case_id"],
                     "bucket": bucket(row["features"]),
                     "repeat": row["repeat"], "warmup": row["warmup"], **choice,
                     "use_parallel": gate(row["features"], manifest["config"], choice["candidate"]),
                     "state_before_sha256": digest(state), "state_after_sha256": digest(state)}
            event["decision_sha256"] = digest(event)
            decisions.append(event)
    return decisions


def tape_header(manifest):
    return {"type": "header", "schema_version": 1, "controller_version": 1,
            "manifest": manifest, "manifest_sha256": digest(manifest),
            "source_commit": manifest.get("source_commit"),
            "seed": manifest.get("seed"), "static_config": manifest["config"],
            "parser_config": manifest.get("parser_config")}


def make_tape(manifest, rows):
    events = [tape_header(manifest)]
    medians = {}
    for row in measured(rows):
        if successful(row):
            medians.setdefault((row["case_id"], row["mode"]), []).append(row["reasoning_ns"])
    medians = {key: rational(statistics.median(map(Fraction, values)))
               for key, values in medians.items()}
    events.extend({"type": "observation", "index": i, "row": row,
                   "bucket": bucket(row["features"]) if row.get("features") and
                   row["features"].get("workers", 0) > 0 else None,
                   "median_duration_ns": medians.get((row["case_id"], row["mode"])),
                   "diagnostic_baselines": {mode: medians.get((row["case_id"], mode))
                                            for mode in ("sequential", "parallel")},
                   "semantic_fingerprint": digest(row.get("verdict"))}
                  for i, row in enumerate(rows))
    events.extend(decision_events(manifest, rows))
    content = b"".join(map(canonical, events))
    events.append({"type": "footer", "event_count": len(events),
                   "tape_sha256": hashlib.sha256(content).hexdigest()})
    return b"".join(map(canonical, events))


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def replay(tape, manifest):
    lines = tape.splitlines(keepends=True)
    events = [json.loads(line, object_pairs_hook=unique_object) for line in lines]
    if len(events) < 3 or any(canonical(e) != line for e, line in zip(events, lines)):
        raise ValueError("non-canonical or truncated tape")
    header, footer = events[0], events[-1]
    if header != tape_header(manifest):
        raise ValueError("manifest mismatch")
    if footer != {"type": "footer", "event_count": len(events) - 1,
                   "tape_sha256": hashlib.sha256(b"".join(lines[:-1])).hexdigest()}:
        raise ValueError("tape hash mismatch")
    observations = [e for e in events[1:-1] if e.get("type") == "observation"]
    if [e["index"] for e in observations] != list(range(len(observations))):
        raise ValueError("observation order mismatch")
    rows = [e["row"] for e in observations]
    identities = [(r["case_id"], r["mode"], r["repeat"], r["warmup"]) for r in rows]
    if len(set(identities)) != len(identities):
        raise ValueError("duplicate observation")
    if [list(item) for item in identities] != manifest["schedule"]:
        raise ValueError("missing or reordered observations")
    cases = {case["id"]: case for case in manifest["cases"]}
    for row in rows:
        if row["split"] != cases[row["case_id"]]["split"]:
            raise ValueError("observation split mismatch")
        if row["candidate"] not in LADDER:
            raise ValueError("invalid candidate")
        if row["status"] == "success" and not successful(row):
            raise ValueError("invalid successful observation")
    state = fit(manifest, rows)
    for row in rows:
        if row["mode"] == "feedback" and has_features(row):
            if row["candidate"] != select(state, manifest, row["features"])["candidate"]:
                raise ValueError("frozen decision mismatch")
        if row["mode"] == "best_fixed" and row["candidate"] != state["global_multiplier"]:
            raise ValueError("fixed decision mismatch")
    if make_tape(manifest, rows) != tape:
        raise ValueError("decision replay mismatch")
    return {"status": "pass", "observations": len(rows),
            "decisions": len(decision_events(manifest, rows)),
            "state_sha256": digest(fit(manifest, rows)),
            "tape_sha256": footer["tape_sha256"]}


def semantic_issues(manifest, rows):
    issues = []
    for case in manifest["cases"]:
        case_rows = [r for r in rows if r["case_id"] == case["id"]]
        expected = case.get("census", {}).get("verdict")
        if not expected:
            continue
        if (case.get("corpus") == "synthetic" and case.get("input")
                and case.get("census", {}).get("status") == "success"):
            expected_result = "inconsistent" if case["input"]["conflict"] else "consistent"
            if expected["result"] != expected_result:
                issues.append(f'{case["id"]}:census:known_synthetic_verdict')
        for row in case_rows:
            if row["status"] in ("success", "error") and row.get("verdict") != expected:
                issues.append(f'{case["id"]}:{row["mode"]}:{row["repeat"]}:verdict')
            if row.get("features") and row["features"] != case["census"]["features"]:
                issues.append(f'{case["id"]}:{row["mode"]}:{row["repeat"]}:features')
            if row.get("ontology_sha256") and row["ontology_sha256"] != case["census"]["ontology_sha256"]:
                issues.append(f'{case["id"]}:{row["mode"]}:{row["repeat"]}:ontology')
            if successful(row):
                wanted = (False if row["mode"] == "sequential" else
                          row["features"]["disjunctions"] > 0 if row["mode"] == "parallel" else
                          gate(row["features"], manifest["config"], row["candidate"]))
                if row["used_parallel"] != wanted:
                    issues.append(f'{case["id"]}:{row["mode"]}:{row["repeat"]}:gate')
    return issues


def aggregate_ratios(groups, indices=None):
    regrets = {mode: [] for mode in EVAL_MODES}
    for case_id, rows in groups.items():
        by_mode = {mode: sorted([r for r in rows if r["mode"] == mode],
                               key=lambda r: r["repeat"]) for mode in EVAL_MODES}
        take = indices[case_id] if indices else range(len(by_mode["static"]))
        times = {mode: statistics.median(by_mode[mode][i]["reasoning_ns"] for i in take)
                 for mode in EVAL_MODES}
        denominator = min(times["sequential"], times["parallel"])
        for mode in EVAL_MODES:
            regrets[mode].append(times[mode] / denominator)
    summary = {mode: statistics.median(values) for mode, values in regrets.items()}
    return {mode: summary["feedback"] / summary[mode] for mode in ("static", "best_fixed")}


def interval(values):
    ordered = sorted(values)
    return [ordered[int((len(ordered) - 1) * 0.025)],
            ordered[int((len(ordered) - 1) * 0.975)]]


def classify(manifest, rows, replay_report):
    """Evidence classification never drops an unsuccessful required case."""
    issues = semantic_issues(manifest, rows)
    groups = complete_cases(manifest, rows, "evaluation")
    required = [c for c in manifest["cases"] if c["split"] == "evaluation"]
    state = fit(manifest, rows)
    missing = [c["id"] for c in required if c["id"] not in groups]
    failed = []
    if not state["training_complete"]:
        failed.append("incomplete_calibration")
    if missing:
        failed.append("incomplete_evaluation")
    if manifest["repeats"] < 7 or manifest.get("smoke"):
        failed.append("smoke_not_promotable")
    real_cases = [c for c in required if c.get("corpus") == "real"]
    if {c["id"] for c in real_cases} != {"doid", "go_basic", "uberon"}:
        failed.append("real_corpus_incomplete")
    coverage = {"eligible": len(groups), "decision_flipped": 0, "no_op": 0}
    for case_rows in groups.values():
        features = case_rows[0]["features"]
        baseline = gate(features, manifest["config"], "1/1")
        learned = gate(features, manifest["config"], select(state, manifest, features)["candidate"])
        coverage["decision_flipped"] += int(baseline != learned)
        coverage["no_op"] += int(all(gate(features, manifest["config"], c) == baseline for c in LADDER))
    fraction = coverage["decision_flipped"] / max(1, coverage["eligible"])
    if fraction < 0.2:
        failed.append("decision_flip_below_20_percent")
    result = {"classification": "inconclusive", "failed_gates": failed,
              "semantic_issues": issues, "coverage": coverage,
              "missing_cases": missing, "state": state, "ratios": {}, "intervals": {},
              "real_intervals": {}}
    if groups:
        result["ratios"] = aggregate_ratios(groups)
        rng = random.Random(manifest["seed"])
        boot = {"static": [], "best_fixed": []}
        real_boot = {c["id"]: [] for c in real_cases if c["id"] in groups}
        for _ in range(manifest["bootstrap_resamples"]):
            picks = {cid: [rng.randrange(manifest["repeats"])
                          for _ in range(manifest["repeats"])] for cid in groups}
            for mode, ratio in aggregate_ratios(groups, picks).items():
                boot[mode].append(ratio)
            for cid in real_boot:
                rs = groups[cid]
                values = {mode: sorted((r for r in rs if r["mode"] == mode),
                                      key=lambda r: r["repeat"]) for mode in ("static", "feedback")}
                med = {mode: statistics.median(values[mode][i]["reasoning_ns"] for i in picks[cid])
                       for mode in values}
                real_boot[cid].append(med["feedback"] / med["static"])
        result["intervals"] = {mode: interval(v) for mode, v in boot.items()}
        result["real_intervals"] = {cid: interval(v) for cid, v in real_boot.items()}
        if any(value > 0.95 for value in result["ratios"].values()):
            failed.append("improvement_below_5_percent")
        if any(bounds[1] >= 1 for bounds in result["intervals"].values()):
            failed.append("improvement_not_confident")
        if any(bounds[0] > 1.05 for bounds in result["real_intervals"].values()):
            failed.append("real_regression_above_5_percent")
        worsening = any(bounds[0] > 1.05 for bounds in result["intervals"].values())
        worsening |= "real_regression_above_5_percent" in failed
        valid = not (missing or not state["training_complete"] or manifest.get("smoke")
                     or "real_corpus_incomplete" in failed)
        if not failed:
            result["classification"] = "better"
        elif valid and worsening:
            result["classification"] = "worse"
        elif valid and fraction >= 0.2 and all(
                0.95 <= bounds[0] <= bounds[1] <= 1.05 for bounds in result["intervals"].values()):
            result["classification"] = "neutral"
    if replay_report.get("status") != "pass":
        result["classification"] = "replay_failure"
        failed.append("replay_failure")
    if issues:
        result["classification"] = "semantic_failure"
        failed.append("semantic_failure")
    return result


def probe_environment():
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("SPACL_", "OWL2_REASONER_"))}
    environment.update({"OWL2_REASONER_AUTO_CACHE": "0", "OWL2_REASONER_FORCE_TEXT": "1",
                        "OWL2_REASONER_BIN_ONLY": "0",
                        "OWL2_REASONER_MAX_FILE_SIZE": str(MAX_INPUT_BYTES)})
    return environment


def decode_probe_stdout(output):
    """The loader may print diagnostics; the probe result is the final JSON line."""
    lines = output.splitlines()
    if not lines:
        raise ValueError("missing probe response")
    response = json.loads(lines[-1], object_pairs_hook=unique_object)
    if not isinstance(response, dict) or "status" not in response or "verdict" not in response:
        raise ValueError("invalid probe envelope")
    if any(line.lstrip().startswith(("{", "[")) for line in lines[:-1]):
        raise ValueError("ambiguous probe response")
    diagnostics = "\n".join(lines[:-1])
    response["diagnostic_line_count"] = len(lines) - 1
    response["diagnostics_sha256"] = hashlib.sha256(diagnostics.encode()).hexdigest()
    return response


def invoke_probe(binary, request, cpus, timeout_seconds):
    started = time.perf_counter_ns()
    try:
        process = subprocess.run(["taskset", "-c", ",".join(map(str, cpus)), str(binary)],
                                 input=json.dumps(request), text=True, capture_output=True,
                                 timeout=timeout_seconds, env=probe_environment())
        wall = time.perf_counter_ns() - started
        if process.returncode:
            return {"status": "failed", "wall_ns": wall, "reasoning_ns": 0,
                    "error": process.stderr[-500:]}
        response = decode_probe_stdout(process.stdout)
        response["wall_ns"] = wall
        return response
    except subprocess.TimeoutExpired:
        return {"status": "timeout", "wall_ns": time.perf_counter_ns() - started,
                "reasoning_ns": 0}
    except (OSError, ValueError) as error:
        return {"status": "failed", "wall_ns": time.perf_counter_ns() - started,
                "reasoning_ns": 0, "error": type(error).__name__}


def build_cases(provenance, smoke):
    cases = []
    for split, sizes in (("calibration", (6, 12, 24)), ("evaluation", (7, 13, 25))):
        for count in sizes:
            cases.append({"id": f"{split}_union_{count}", "split": split, "corpus": "synthetic",
                          "input": {"kind": "synthetic", "unions": count, "operands": 2,
                                    "padding": 20000, "conflict": False}})
        count = sizes[0]
        cases.append({"id": f"{split}_conflict_{count}", "split": split, "corpus": "synthetic",
                      "input": {"kind": "synthetic", "unions": count, "operands": 2,
                                "padding": 20000, "conflict": True}})
        cases.append({"id": f"{split}_no_op", "split": split, "corpus": "synthetic",
                      "input": {"kind": "synthetic", "unions": count, "operands": 2,
                                "padding": 0, "conflict": False}})
    if smoke:
        return [cases[0], cases[3], cases[5], cases[8]]
    for item in provenance["inputs"]:
        cases.append({"id": item["id"], "split": "evaluation", "corpus": "real",
                      "input": {"kind": "file", "filename": item["filename"],
                                "sha256": item["sha256"]}, "provenance": item})
    return cases


def request_for(case, config, mode, candidate, inputs_dir):
    data = dict(case["input"])
    if data["kind"] == "file":
        data = {"kind": "file", "path": str(inputs_dir / data["filename"])}
    return {"input": data, **config, "mode": {"parallel": "always_parallel"}.get(mode, mode)
            if mode in ("sequential", "parallel") else "adaptive",
            "cost_threshold_us": threshold(config["cost_threshold_us"], candidate)}


def create_manifest(binary, inputs_dir, cpus, smoke, timeout_seconds):
    provenance = json.loads((ROOT / "benchmarks/prototypes/spacl_feedback/corpus_provenance.json").read_text())
    cases = build_cases(provenance, smoke)
    config = {"workers": len(cpus), "parallel_threshold": 2000, "cost_threshold_us": 500,
              "timeout_ms": 2000}
    manifest = {"schema_version": 1, "protocol": "spacl_feedback_v1", "smoke": smoke,
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "binary_sha256": file_hash(binary), "cases": cases, "config": config,
                "seed": 20260905, "repeats": 1 if smoke else 7, "warmups": 1,
                "bootstrap_resamples": 1000, "cpus": cpus, "timeout_seconds": timeout_seconds,
                "ladder": list(LADDER), "platform": platform.platform(),
                "parser_config": {"loader": "load_ontology_with_env",
                                  "force_text": True, "bin_only": False,
                                  "auto_cache": False, "large_profile_auto": False,
                                  "max_file_size_bytes": MAX_INPUT_BYTES,
                                  "resolve_imports": False,
                                  "scope": "local_document_only"},
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip(),
                "source_hashes": {p: file_hash(ROOT / p) for p in (
                    "scripts/prototype_spacl_feedback.py", "scripts/prototype_spacl_probe.rs",
                    "src/reasoner/speculative.rs", "Cargo.lock")},
                "schedule": []}
    for case in cases:
        if case["input"]["kind"] == "file":
            path = inputs_dir / case["input"]["filename"]
            if not path.is_file() or file_hash(path) != case["input"]["sha256"]:
                case["census"] = {"status": "input_unavailable"}
                continue
        census = invoke_probe(binary, request_for(case, config, "sequential", "1/1", inputs_dir),
                              cpus, timeout_seconds)
        # Census durations cannot become feedback or benchmark samples.
        case["census"] = {key: census[key] for key in (
            "status", "verdict", "features", "ontology_sha256", "error",
            "diagnostic_line_count", "diagnostics_sha256") if key in census}
        print(f'census {case["id"]}: {census["status"]}', flush=True)
    for case in cases:
        modes = TRAIN_MODES if case["split"] == "calibration" else EVAL_MODES
        for repeat in range(-1, manifest["repeats"]):
            offset = (manifest["seed"] + repeat + 1) % len(modes)
            for mode in (*modes[offset:], *modes[:offset]):
                manifest["schedule"].append([case["id"], mode, repeat, repeat == -1])
    return manifest


def write_artifacts(output, manifest, rows):
    tape = make_tape(manifest, rows)
    (output / "feedback_tape.jsonl").write_bytes(tape)
    try:
        replay_report = replay(tape, manifest)
    except (ValueError, KeyError, TypeError) as error:
        replay_report = {"status": "fail", "error": str(error)}
    summary = classify(manifest, rows, replay_report)
    (output / "replay_report.json").write_bytes(canonical(replay_report))
    (output / "summary.json").write_bytes(canonical(summary))
    (output / "decisions.jsonl").write_bytes(b"".join(map(canonical, decision_events(manifest, rows))))
    fingerprints = [{"case_id": r["case_id"], "mode": r["mode"], "repeat": r["repeat"],
                     "warmup": r["warmup"], "status": r["status"], "verdict": r.get("verdict"),
                     "fingerprint_sha256": digest(r.get("verdict"))} for r in rows]
    (output / "semantic_fingerprints.jsonl").write_bytes(b"".join(map(canonical, fingerprints)))
    columns = ("case_id", "split", "mode", "candidate", "repeat", "warmup", "status",
               "reasoning_ns", "wall_ns", "used_parallel", "branches_created", "nogood_hits",
               "features", "verdict", "ontology_sha256")
    with (output / "raw_results.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=columns, extrasaction="ignore")
        writer.writeheader()
        writer.writerows({k: json.dumps(v, sort_keys=True) if isinstance(v, dict) else v
                          for k, v in row.items()} for row in rows)
    counts = {status: sum(r["status"] == status for r in measured(rows))
              for status in sorted({r["status"] for r in measured(rows)})}
    summary["status_counts"] = counts
    (output / "summary.json").write_bytes(canonical(summary))
    with (output / "summary.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(["classification", "eligible", "decision_flipped", "no_op"])
        writer.writerow([summary["classification"], *summary["coverage"].values()])
    report = ("# SPACL Phase 1 prototype result\n\n"
              f'Classification: **{summary["classification"]}**\n\n'
              f'Source commit: {manifest["source_commit"]}\n\n'
              f'Manifest SHA-256: {digest(manifest)}\n\n'
              f'Status counts (measured samples): {json.dumps(counts, sort_keys=True)}\n\n'
              f'Coverage: {json.dumps(summary["coverage"], sort_keys=True)}\n\n'
              f'Feedback/comparator regret ratios: {json.dumps(summary["ratios"], sort_keys=True)}\n\n'
              f'Paired 95% intervals: {json.dumps(summary["intervals"], sort_keys=True)}\n\n'
              f'Failed gates: {", ".join(summary["failed_gates"]) or "none"}\n\n'
              f'Semantic mismatches: {len(summary["semantic_issues"])}; replay: {replay_report["status"]}.\n\n'
              "This result describes a frozen, harness-trained selector. "
              "No within-run controller or formal correctness proof is established.\n")
    (output / "summary.md").write_text(report)
    return summary


def run(args):
    binary = Path(args.probe).resolve()
    inputs_dir = Path(args.inputs_dir).resolve()
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    available = sorted(os.sched_getaffinity(0))
    cpus = available[:2]
    if len(cpus) < 2:
        raise ValueError("experiment requires at least two allowed CPUs")
    manifest = create_manifest(binary, inputs_dir, cpus, args.smoke, args.timeout)
    (output / "manifest.json").write_bytes(canonical(manifest))
    rows, state = [], {}
    cases = {c["id"]: c for c in manifest["cases"]}
    stopped = False
    bad_cases = set()
    last_case = None
    for case_id, mode, repeat, warmup in manifest["schedule"]:
        case = cases[case_id]
        if case_id != last_case:
            print(f'measure {case_id}', flush=True)
            if case["split"] == "evaluation":
                state = fit(manifest, rows)
            last_case = case_id
        features = case["census"].get("features") if case["census"]["status"] == "success" else None
        candidate = (mode if mode in LADDER else state.get("global_multiplier", "1/1")
                     if mode == "best_fixed" else select(state, manifest, features)["candidate"]
                     if mode == "feedback" and features else "1/1")
        if stopped or case_id in bad_cases or case["census"]["status"] != "success":
            response = {"status": "not_run", "reasoning_ns": 0, "wall_ns": 0}
        else:
            response = invoke_probe(binary, request_for(case, manifest["config"], mode, candidate, inputs_dir),
                                    cpus, args.timeout)
        row = {"case_id": case_id, "split": case["split"], "mode": mode, "candidate": candidate,
               "repeat": repeat, "warmup": warmup, **response}
        rows.append(row)
        # Preserve raw observations immediately even if later processing fails.
        with (output / "observations.jsonl").open("ab") as checkpoint:
            checkpoint.write(canonical(row))
        if response["status"] != "success":
            bad_cases.add(case_id)
        if semantic_issues(manifest, [row]):
            stopped = True
            print(f'safety failure at {case_id}:{mode}; remaining samples are not_run', flush=True)
    summary = write_artifacts(output, manifest, rows)
    print(json.dumps({"classification": summary["classification"],
                      "status_counts": summary["status_counts"], "output": str(output)}, sort_keys=True))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    run_parser = commands.add_parser("run")
    run_parser.add_argument("--probe", required=True)
    run_parser.add_argument("--inputs-dir", required=True)
    run_parser.add_argument("--output", required=True)
    run_parser.add_argument("--timeout", type=int, default=30)
    run_parser.add_argument("--smoke", action="store_true")
    replay_parser = commands.add_parser("replay")
    replay_parser.add_argument("directory")
    args = parser.parse_args()
    if args.command == "run":
        return run(args)
    directory = Path(args.directory)
    manifest = json.loads((directory / "manifest.json").read_bytes(), object_pairs_hook=unique_object)
    print(json.dumps(replay((directory / "feedback_tape.jsonl").read_bytes(), manifest), sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
