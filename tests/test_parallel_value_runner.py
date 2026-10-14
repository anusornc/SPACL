import itertools
import sys
import subprocess
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import run_spacl_parallel_value as runner
from prototype_parallel_value_oracle import cases, evaluate_case, validate_witness


class EvidenceContractTests(unittest.TestCase):
    def test_dirty_protocol_cannot_be_claimed_as_clean_committed_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            for name in runner.SOURCES:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture\n")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                            "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
                            "commit", "-qm", "Establish an immutable source-proof fixture"], cwd=root, check=True)
            runner.check_sources_clean(root)
            (root / "docs/specs/SPACL_PARALLEL_VALUE_PROBE.md").write_text("changed protocol\n")
            with self.assertRaises(subprocess.CalledProcessError):
                runner.check_sources_clean(root)

    def test_failed_gates_are_explicit_and_deterministic(self):
        rows = [{"case_id": "primary", "two_worker_samples": 2, "nontrivial_work_samples": 7}]
        comparisons = {"serial_direct": {"ratio": 1.2, "upper_95": 1.3},
                       "serial_frontier": {"ratio": .8, "upper_95": .9}}
        failures = runner.failure_gates(rows, comparisons, ["primary"])
        self.assertEqual([entry["gate"] for entry in failures], [
            "primary_work_gate", "serial_direct_ratio_above_0_95",
            "serial_direct_upper_interval_not_below_1"])
        self.assertEqual(failures[0]["case_id"], "primary")

    def test_frozen_matrix_has_unique_audits_and_seven_paired_repeats(self):
        sequence = runner.record_order(cases())
        self.assertEqual(len(sequence), 176)
        self.assertEqual(len(set(sequence)), 176)
        self.assertEqual(sequence[:8], [(c["id"], "audit", None) for c in cases()])
        for case in cases():
            for repeat in range(7):
                block = [mode for name, mode, index in sequence if name == case["id"] and index == repeat]
                self.assertEqual(set(block), set(runner.MODES))

    def test_error_probe_cannot_become_fast_success(self):
        with self.assertRaises(runner.GateError) as captured:
            runner.check_response(cases()[0], {"status": "error", "reasoning_ns": 0}, "serial_direct")
        self.assertEqual(captured.exception.classification, "inconclusive")
        self.assertEqual(captured.exception.gate, "probe_available")

    def test_wrong_verdict_fails_before_time_is_used(self):
        case = cases()[0]
        response = {"status": "success", "case_id": case["id"], "mode": "serial_direct",
                    "vertices": 5, "colors": 2, "verdict": {"result": "consistent"}}
        with self.assertRaisesRegex(runner.GateError, "oracle verdict mismatch") as captured:
            runner.check_response(case, response, "serial_direct")
        self.assertEqual(captured.exception.gate, "oracle_verdict_mismatch")

    def test_independent_prefix_coverage_for_adjacent_and_nonadjacent_vertices(self):
        case = cases()[1]
        prefixes = runner.expected_prefixes(case, [0, 1, 2, 3, 4])
        self.assertEqual(len(prefixes), 6)
        self.assertEqual({(p[0], p[1]) for p in prefixes},
                         {(a, b) for a, b in itertools.product(range(3), repeat=2) if a != b})
        nonadjacent = runner.expected_prefixes(case, [0, 2, 1, 3, 4])
        self.assertEqual(len(nonadjacent), 9)

    def test_invalid_witness_is_rejected_even_when_result_label_is_sat(self):
        case = cases()[1]
        response = {"status": "success", "case_id": case["id"], "mode": "serial_direct",
                    "vertices": 5, "colors": 3, "verdict": {"result": "consistent"},
                    "witness": [0] * 5}
        with self.assertRaisesRegex(runner.GateError, "invalid result witness"):
            runner.check_response(case, response, "serial_direct")

    def test_locked_expected_sat_witnesses_are_independently_valid(self):
        expected_chromatic = [3, 3, 4, 4, 7, 7, 9, 9]
        for case, chromatic in zip(cases(), expected_chromatic):
            result = evaluate_case(case)
            self.assertEqual(result["chromatic_number"], chromatic)
            if result["expected"]:
                self.assertTrue(validate_witness(case, result["witness"]))


if __name__ == "__main__":
    unittest.main()
