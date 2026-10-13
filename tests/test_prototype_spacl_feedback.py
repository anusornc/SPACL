"""Acceptance examples for the controller, separate from timing evidence."""
import sys
import copy
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import prototype_spacl_feedback as feedback


def training_fixture():
    features = {"workers": 2, "disjunctions": 6, "branches": 2001, "cost_us": 720}
    manifest = {"schema_version": 1, "repeats": 1,
                "config": {"cost_threshold_us": 500, "parallel_threshold": 2000},
                "cases": [{"id": "train", "split": "calibration"}]}
    times = {"sequential": 100, "parallel": 80, "static": 100,
             "1/4": 101, "1/2": 80, "1/1": 100, "2/1": 85, "4/1": 95}
    rows = [{"case_id": "train", "split": "calibration", "mode": mode, "repeat": 0,
             "warmup": False, "candidate": mode if mode in feedback.LADDER else "1/1",
             "status": "success", "reasoning_ns": value, "features": features,
             "verdict": {"result": "consistent", "error_class": None}}
            for mode, value in times.items()]
    manifest["schedule"] = [[r["case_id"], r["mode"], r["repeat"], r["warmup"]] for r in rows]
    return manifest, rows


def classification_fixture(kind):
    """Synthetic timing fixture for classifier acceptance; not benchmark evidence."""
    config = {"cost_threshold_us": 500, "parallel_threshold": 2000}
    flip_features = {"workers": 2, "disjunctions": 6, "branches": 2001, "cost_us": 720}
    no_op_features = {"workers": 2, "disjunctions": 0, "branches": 2001, "cost_us": 720}
    cases = [
        case("cal_flip", "calibration", "synthetic", flip_features),
        case("cal_static_a", "calibration", "synthetic", no_op_features),
        case("cal_static_b", "calibration", "synthetic", no_op_features),
    ]
    eval_features = no_op_features if kind == "low_coverage" else flip_features
    cases.extend([
        case("doid", "evaluation", "real", eval_features),
        case("go_basic", "evaluation", "real", eval_features),
        case("uberon", "evaluation", "real", eval_features),
        case("heldout_no_op", "evaluation", "synthetic", no_op_features),
    ])
    manifest = {"schema_version": 1, "repeats": 7, "bootstrap_resamples": 20,
                "seed": 20260905, "smoke": False, "config": config,
                "cases": cases, "schedule": []}
    rows = []
    rows.extend(training_rows("cal_flip", flip_features, config, {
        "sequential": 100, "parallel": 100, "static": 120,
        "1/4": 130, "1/2": 70, "1/1": 120, "2/1": 130, "4/1": 130,
    }))
    for case_id in ("cal_static_a", "cal_static_b"):
        rows.extend(training_rows(case_id, no_op_features, config, {
            "sequential": 100, "parallel": 100, "static": 70,
            "1/4": 130, "1/2": 130, "1/1": 70, "2/1": 130, "4/1": 130,
        }))
    eval_times = {
        "better": {"sequential": 100, "parallel": 100, "static": 120,
                   "best_fixed": 120, "feedback": 80},
        "worse": {"sequential": 100, "parallel": 100, "static": 100,
                  "best_fixed": 100, "feedback": 130},
        "neutral": {"sequential": 100, "parallel": 100, "static": 100,
                    "best_fixed": 100, "feedback": 100},
        "low_coverage": {"sequential": 100, "parallel": 100, "static": 120,
                         "best_fixed": 120, "feedback": 80},
    }[kind]
    for case_id in ("doid", "go_basic", "uberon"):
        rows.extend(evaluation_rows(case_id, eval_features, config, eval_times))
    rows.extend(evaluation_rows("heldout_no_op", no_op_features, config, eval_times))
    manifest["schedule"] = [[r["case_id"], r["mode"], r["repeat"], r["warmup"]] for r in rows]
    return manifest, rows


def case(case_id, split, corpus, features):
    return {"id": case_id, "split": split, "corpus": corpus,
            "census": {"verdict": {"result": "consistent", "error_class": None},
                       "features": features, "ontology_sha256": f"sha-{case_id}"}}


def training_rows(case_id, features, config, times):
    rows = []
    for repeat in range(7):
        for mode in feedback.TRAIN_MODES:
            candidate = mode if mode in feedback.LADDER else "1/1"
            rows.append(row(case_id, "calibration", mode, candidate, repeat,
                            times[mode], features, config))
    return rows


def evaluation_rows(case_id, features, config, times):
    candidates = {"sequential": "1/1", "parallel": "1/1", "static": "1/1",
                  "best_fixed": "1/1", "feedback": "1/2"}
    rows = []
    for repeat in range(7):
        for mode in feedback.EVAL_MODES:
            rows.append(row(case_id, "evaluation", mode, candidates[mode], repeat,
                            times[mode], features, config))
    return rows


def row(case_id, split, mode, candidate, repeat, reasoning_ns, features, config):
    if mode == "sequential":
        used_parallel = False
    elif mode == "parallel":
        used_parallel = features["disjunctions"] > 0
    else:
        used_parallel = feedback.gate(features, config, candidate)
    return {"case_id": case_id, "split": split, "mode": mode, "candidate": candidate,
            "repeat": repeat, "warmup": False, "status": "success",
            "reasoning_ns": reasoning_ns, "wall_ns": reasoning_ns + 1,
            "features": features, "verdict": {"result": "consistent", "error_class": None},
            "ontology_sha256": f"sha-{case_id}", "used_parallel": used_parallel}


class ControllerTests(unittest.TestCase):
    def test_failed_probe_features_do_not_break_evidence_serialization(self):
        manifest, rows = training_fixture()
        manifest["cases"].append({"id": "failed", "split": "evaluation"})
        row = dict(rows[0], case_id="failed", split="evaluation", mode="feedback",
                   candidate="1/1", status="error", reasoning_ns=0,
                   features={"workers": 0, "disjunctions": 0, "branches": 0, "cost_us": 0},
                   verdict={"result": "error", "error_class": "ValidationError"})
        rows.append(row)
        manifest["schedule"].append(["failed", "feedback", 0, False])
        tape = feedback.make_tape(manifest, rows)
        self.assertEqual(feedback.replay(tape, manifest)["status"], "pass")

    def test_failed_census_still_writes_an_incomplete_report(self):
        # A failing external probe tests transport/control flow, not performance.
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            probe = base / "failed_probe"
            response = {"status": "error", "verdict": {"result": "error", "error_class": "ValidationError"},
                        "features": {"workers": 0, "branches": 0, "disjunctions": 0, "cost_us": 0},
                        "reasoning_ns": 0}
            probe.write_text("#!/usr/bin/env python3\nimport sys\nsys.stdin.read()\nprint(" +
                             repr(json.dumps(response)) + ")\n")
            probe.chmod(0o700)
            output = base / "results"
            process = subprocess.run([sys.executable, str(feedback.ROOT / "scripts/prototype_spacl_feedback.py"),
                                      "run", "--smoke", "--probe", str(probe),
                                      "--inputs-dir", directory, "--output", str(output)],
                                     capture_output=True, text=True, timeout=20)
            self.assertEqual(process.returncode, 0, process.stderr)
            report = json.loads((output / "summary.json").read_text())
            self.assertEqual(report["classification"], "inconclusive")
            self.assertNotIn("success", report["status_counts"])

    def test_probe_transport_separates_legacy_loader_diagnostics(self):
        payload = {"status": "success", "verdict": {"result": "consistent", "error_class": None}}
        output = "Configured IRI cache for 557202 classes (cache size: 1000000)\n"
        output += json.dumps(payload) + "\n"
        response = feedback.decode_probe_stdout(output)
        self.assertEqual(response["verdict"], payload["verdict"])
        self.assertEqual(response["diagnostic_line_count"], 1)
        with self.assertRaises(ValueError):
            feedback.decode_probe_stdout(json.dumps(payload) + "\n" + output)
        with self.assertRaises(ValueError):
            feedback.decode_probe_stdout(json.dumps(payload) + "\ntrailing diagnostic")

    def test_replay_rejects_missing_audit_fields_after_rehash(self):
        manifest, rows = training_fixture()
        for event_type, field in (("header", "controller_version"),
                                  ("observation", "semantic_fingerprint"),
                                  ("decision", "selection_evidence")):
            events = [json.loads(line) for line in feedback.make_tape(manifest, rows).splitlines()]
            event = next(e for e in events if e["type"] == event_type)
            del event[field]
            content = b"".join(feedback.canonical(e) for e in events[:-1])
            events[-1]["tape_sha256"] = feedback.hashlib.sha256(content).hexdigest()
            damaged = b"".join(feedback.canonical(e) for e in events)
            with self.assertRaises(ValueError):
                feedback.replay(damaged, manifest)

    def test_cost_ladder_rounds_up_and_clamps(self):
        self.assertEqual(feedback.threshold(501, "1/4"), 126)
        self.assertEqual(feedback.threshold(0, "1/4"), 1)
        self.assertEqual(feedback.threshold(2**64 - 1, "4/1"), 2**64 - 1)

    def test_bucket_zero_is_distinct_from_one_and_power_of_two(self):
        self.assertEqual(feedback.band(0), -1)
        self.assertEqual(feedback.band(1), 0)
        self.assertEqual(feedback.band(7), 2)
        self.assertEqual(feedback.band(8), 3)

    def test_training_selects_a_known_winner_without_evaluation_leakage(self):
        manifest, rows = training_fixture()
        state = feedback.fit(manifest, rows)
        self.assertEqual(state["global_multiplier"], "1/2")
        extra = dict(rows[0], case_id="heldout", split="evaluation", reasoning_ns=1)
        self.assertEqual(feedback.fit(manifest, rows + [extra]), state)
        before = copy.deepcopy(state)
        self.assertEqual(feedback.select(state, manifest, rows[0]["features"])["candidate"], "1/2")
        self.assertEqual(state, before)

    def test_ties_prefer_static_then_less_parallel(self):
        self.assertEqual(feedback.score_choice({"1/4": 100, "1/1": 104}), "1/1")
        self.assertEqual(feedback.score_choice({"1/4": 100, "1/2": 104}), "1/2")

    def test_unknown_bucket_and_incompatible_state_fall_back(self):
        manifest, rows = training_fixture()
        state = feedback.fit(manifest, rows)
        unknown = dict(rows[0]["features"], workers=4)
        self.assertEqual(feedback.select(state, manifest, unknown)["candidate"], "1/1")
        state["manifest_sha256"] = "invalid"
        before = copy.deepcopy(state)
        self.assertEqual(feedback.select(state, manifest, unknown)["reason"], "invalid_state")
        self.assertEqual(state, before)

    def test_canonical_hash_is_order_independent(self):
        self.assertEqual(feedback.digest({"x": {"a": 1, "b": 2}, "y": 3}),
                         feedback.digest({"y": 3, "x": {"b": 2, "a": 1}}))
        self.assertEqual(feedback.canonical({"b": 2, "a": 1}), b'{"a":1,"b":2}\n')

    def test_recorded_tape_replays_and_rejects_changes(self):
        manifest, rows = training_fixture()
        tape = feedback.make_tape(manifest, rows)
        self.assertEqual(feedback.replay(tape, manifest)["status"], "pass")
        for broken in (tape[:-1], tape.replace(b'"candidate":"1/2"', b'"candidate":"4/1"', 1)):
            with self.assertRaises(ValueError):
                feedback.replay(broken, manifest)
        with self.assertRaises(ValueError):
            feedback.replay(tape, dict(manifest, repeats=7))

    def test_missing_or_duplicate_observations_cannot_replay_even_with_new_hash(self):
        manifest, rows = training_fixture()
        for changed in (rows[:-1], rows + [rows[0]], list(reversed(rows))):
            with self.assertRaises(ValueError):
                feedback.replay(feedback.make_tape(manifest, changed), manifest)

    def test_replay_rejects_rehashed_feedback_choices_that_disagree_with_frozen_state(self):
        manifest, rows = classification_fixture("better")
        changed = copy.deepcopy(rows)
        feedback_row = next(r for r in changed
                            if r["split"] == "evaluation" and r["mode"] == "feedback")
        feedback_row["candidate"] = "4/1"
        with self.assertRaisesRegex(ValueError, "frozen decision mismatch"):
            feedback.replay(feedback.make_tape(manifest, changed), manifest)

    def test_evaluation_rows_do_not_train_or_change_frozen_bucket_state(self):
        manifest, rows = classification_fixture("better")
        calibration = [r for r in rows if r["split"] == "calibration"]
        state = feedback.fit(manifest, calibration)
        trained_key = feedback.bucket(next(r["features"] for r in rows
                                           if r["case_id"] == "doid"))

        self.assertEqual(feedback.fit(manifest, rows), state)
        self.assertEqual(state["buckets"][trained_key], "1/2")

    def test_semantic_mismatch_takes_precedence_over_replay_failure(self):
        manifest, rows = classification_fixture("better")
        changed = copy.deepcopy(rows)
        changed[0]["verdict"] = {"result": "inconsistent", "error_class": None}

        result = feedback.classify(manifest, changed, {"status": "fail"})

        self.assertEqual(result["classification"], "semantic_failure")
        self.assertIn("semantic_failure", result["failed_gates"])

    def test_timeout_required_evaluation_prevents_better_classification(self):
        manifest, rows = classification_fixture("better")
        changed = copy.deepcopy(rows)
        timed_out = next(r for r in changed
                         if r["case_id"] == "doid" and r["mode"] == "feedback")
        timed_out.update({"status": "timeout", "reasoning_ns": 0})

        result = feedback.classify(manifest, changed, {"status": "pass"})

        self.assertEqual(result["classification"], "inconclusive")
        self.assertIn("incomplete_evaluation", result["failed_gates"])
        self.assertIn("doid", result["missing_cases"])

    def test_decision_coverage_counts_flips_and_no_op_cases(self):
        manifest, rows = classification_fixture("better")

        result = feedback.classify(manifest, rows, {"status": "pass"})

        self.assertEqual(result["coverage"], {"eligible": 4, "decision_flipped": 3, "no_op": 1})

    def test_classifies_better_when_all_promotion_gates_pass(self):
        manifest, rows = classification_fixture("better")

        result = feedback.classify(manifest, rows, {"status": "pass"})

        self.assertEqual(result["classification"], "better")

    def test_classifies_worse_when_regression_is_confident(self):
        manifest, rows = classification_fixture("worse")

        result = feedback.classify(manifest, rows, {"status": "pass"})

        self.assertEqual(result["classification"], "worse")

    def test_classifies_neutral_when_differences_are_equivalent(self):
        manifest, rows = classification_fixture("neutral")

        result = feedback.classify(manifest, rows, {"status": "pass"})

        self.assertEqual(result["classification"], "neutral")

    def test_classifies_inconclusive_when_decision_flip_coverage_is_low(self):
        manifest, rows = classification_fixture("low_coverage")

        result = feedback.classify(manifest, rows, {"status": "pass"})

        self.assertEqual(result["classification"], "inconclusive")
        self.assertIn("decision_flip_below_20_percent", result["failed_gates"])


if __name__ == "__main__":
    unittest.main()
