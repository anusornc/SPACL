# OWL Reasoner Head-to-Head Report

- Run ID: standard_r1_20260225
- Generated: 2026-02-25T14:25:59+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 7 | 0 | 0 | 0 | 0 | 0 | 8486 |
| hermit | 7 | 0 | 0 | 0 | 0 | 0 | 4117 |
| konclude | 0 | 7 | 0 | 0 | 0 | 0 | N/A |
| openllet | 7 | 0 | 0 | 0 | 0 | 0 | 4213 |
| elk | 7 | 0 | 0 | 0 | 0 | 0 | 3767 |
| jfact | 5 | 2 | 0 | 0 | 0 | 0 | 3450 |
| pellet | 7 | 0 | 0 | 0 | 0 | 0 | 4202 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | disjunctive_simple.owl | 3718 | 53 | success | consistent |
| elk | disjunctive_test.owl | 3012 | 50 | success | consistent |
| elk | hierarchy_100.owl | 2910 | 48 | success | consistent |
| elk | hierarchy_1000.owl | 3188 | 66 | success | consistent |
| elk | hierarchy_10000.owl | 3643 | 131 | success | consistent |
| elk | hierarchy_100000.owl | 7035 | 421 | success | consistent |
| elk | univ-bench.owl | 2866 | 54 | success | consistent |
| hermit | disjunctive_simple.owl | 3446 | 1050 | success | consistent |
| hermit | disjunctive_test.owl | 3245 | 1041 | success | consistent |
| hermit | hierarchy_100.owl | 3442 | 1083 | success | consistent |
| hermit | hierarchy_1000.owl | 3559 | 1219 | success | consistent |
| hermit | hierarchy_10000.owl | 4083 | 1809 | success | consistent |
| hermit | hierarchy_100000.owl | 7706 | 5467 | success | consistent |
| hermit | univ-bench.owl | 3338 | 1060 | success | consistent |
| jfact | disjunctive_simple.owl | 3350 | 78 | success | consistent |
| jfact | disjunctive_test.owl | 3362 | 79 | success | consistent |
| jfact | hierarchy_100.owl | 3271 | 86 | success | consistent |
| jfact | hierarchy_1000.owl | 3954 | 391 | success | consistent |
| jfact | hierarchy_10000.owl | 4233 | 1579 | failed | unknown |
| jfact | hierarchy_100000.owl | 7479 | 4879 | failed | unknown |
| jfact | univ-bench.owl | 3317 | 82 | success | consistent |
| konclude | disjunctive_simple.owl | 2274 | 22 | failed | unknown |
| konclude | disjunctive_test.owl | 2292 | 31 | failed | unknown |
| konclude | hierarchy_100.owl | 2246 | 19 | failed | unknown |
| konclude | hierarchy_1000.owl | 2479 | 46 | failed | unknown |
| konclude | hierarchy_10000.owl | 2437 | 130 | failed | unknown |
| konclude | hierarchy_100000.owl | 3544 | 1369 | failed | unknown |
| konclude | univ-bench.owl | 2281 | 17 | failed | unknown |
| openllet | disjunctive_simple.owl | 3383 | 27 | success | consistent |
| openllet | disjunctive_test.owl | 3007 | 26 | success | consistent |
| openllet | hierarchy_100.owl | 3070 | 26 | success | consistent |
| openllet | hierarchy_1000.owl | 3937 | 29 | success | consistent |
| openllet | hierarchy_10000.owl | 4084 | 58 | success | consistent |
| openllet | hierarchy_100000.owl | 8881 | 249 | success | consistent |
| openllet | univ-bench.owl | 3135 | 28 | success | consistent |
| pellet | disjunctive_simple.owl | 3019 | 18 | success | consistent |
| pellet | disjunctive_test.owl | 2843 | 17 | success | consistent |
| pellet | hierarchy_100.owl | 2883 | 17 | success | consistent |
| pellet | hierarchy_1000.owl | 2921 | 24 | success | consistent |
| pellet | hierarchy_10000.owl | 3527 | 44 | success | consistent |
| pellet | hierarchy_100000.owl | 11485 | 241 | success | consistent |
| pellet | univ-bench.owl | 2742 | 18 | success | consistent |
| tableauxx | disjunctive_simple.owl | 2240 | 9 | success | consistent |
| tableauxx | disjunctive_test.owl | 1951 | 8 | success | consistent |
| tableauxx | hierarchy_100.owl | 2384 | 11 | success | consistent |
| tableauxx | hierarchy_1000.owl | 2330 | 43 | success | consistent |
| tableauxx | hierarchy_10000.owl | 8007 | 5664 | success | consistent |
| tableauxx | hierarchy_100000.owl | 40210 | 37807 | success | consistent |
| tableauxx | univ-bench.owl | 2282 | 10 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
