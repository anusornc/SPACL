import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import prototype_parallel_value_oracle as oracle


class LockedCorpusTests(unittest.TestCase):
    def test_locked_cases_have_canonical_edges_and_declared_roles(self):
        locked = oracle.cases()
        self.assertEqual(
            [case["id"] for case in locked],
            [
                "cycle5_2",
                "cycle5_3",
                "mycielski_c5_3",
                "mycielski_c5_4",
                "clique7_6",
                "clique7_7",
                "clique9_8",
                "clique9_9",
            ],
        )
        self.assertEqual(
            [case["id"] for case in locked if case["role"] == "primary"],
            ["mycielski_c5_3", "clique9_8"],
        )
        for case in locked:
            self.assertEqual(case["edges"], sorted(case["edges"]))
            self.assertEqual(len(case["edges"]), len({tuple(edge) for edge in case["edges"]}))
            for source, target in case["edges"]:
                self.assertLess(source, target)
                self.assertLess(target, case["vertices"])

    def test_known_chromatic_numbers_are_recomputed_independently(self):
        by_id = {case["id"]: case for case in oracle.cases()}
        expected = {
            "cycle5_2": (3, False),
            "cycle5_3": (3, True),
            "mycielski_c5_3": (4, False),
            "mycielski_c5_4": (4, True),
            "clique7_6": (7, False),
            "clique7_7": (7, True),
            "clique9_8": (9, False),
            "clique9_9": (9, True),
        }
        for case_id, (chromatic_number, is_colourable) in expected.items():
            with self.subTest(case_id=case_id):
                result = oracle.evaluate_case(by_id[case_id])
                self.assertEqual(result["chromatic_number"], chromatic_number)
                self.assertEqual(result["expected"], is_colourable)

    def test_satisfiable_cases_return_valid_witnesses(self):
        for case in oracle.cases():
            result = oracle.evaluate_case(case)
            if result["expected"]:
                self.assertIsNotNone(result["witness"])
                self.assertTrue(oracle.validate_witness(case, result["witness"]))
            else:
                self.assertIsNone(result["witness"])

    def test_wrong_witnesses_are_rejected(self):
        case = next(case for case in oracle.cases() if case["id"] == "cycle5_3")
        self.assertFalse(oracle.validate_witness(case, [0, 0, 1, 2, 1]))
        self.assertFalse(oracle.validate_witness(case, [0, 1, 2, 0]))
        self.assertFalse(oracle.validate_witness(case, [0, 1, 2, 0, 3]))
        self.assertFalse(oracle.validate_witness(case, [0, 1, 2, 0, True]))

    def test_prefix_fixed_same_merges_vertices_and_preserves_numeric_colour(self):
        case = next(case for case in oracle.cases() if case["id"] == "cycle5_3")
        prefix = [2, None, 2, None, None]
        result = oracle.evaluate_case(case, prefix)
        self.assertTrue(result["expected"])
        self.assertEqual(result["witness"][0], 2)
        self.assertEqual(result["witness"][2], 2)
        self.assertTrue(oracle.validate_witness(case, result["witness"], prefix))

    def test_prefix_fixed_different_enforces_inequality(self):
        case = next(case for case in oracle.cases() if case["id"] == "cycle5_3")
        prefix = [0, 1, None, None, None]
        result = oracle.evaluate_case(case, prefix)
        self.assertTrue(result["expected"])
        self.assertEqual(result["witness"][0], 0)
        self.assertEqual(result["witness"][1], 1)
        self.assertTrue(oracle.validate_witness(case, result["witness"], prefix))

    def test_prefix_same_colour_on_edge_is_explicitly_not_sat(self):
        case = next(case for case in oracle.cases() if case["id"] == "cycle5_3")
        result = oracle.evaluate_case(case, [1, 1, None, None, None])
        self.assertFalse(result["expected"])
        self.assertEqual(result["status"], "contradictory_prefix")
        self.assertIsNone(result["witness"])

    def test_invalid_graph_is_explicitly_not_sat(self):
        bad = {
            "id": "bad",
            "vertices": 3,
            "edges": [[0, 1], [1, 0], [2, 2]],
            "colors": 2,
            "role": "control",
        }
        result = oracle.evaluate_case(bad)
        self.assertFalse(result["expected"])
        self.assertEqual(result["status"], "invalid_graph")
        self.assertEqual(result["chromatic_number"], 0)
        self.assertIsNone(result["witness"])


if __name__ == "__main__":
    unittest.main()
