# OWL Reasoner Head-to-Head Report

- Run ID: standard_r3_20260225
- Generated: 2026-02-25T14:41:47+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 7 | 0 | 0 | 0 | 0 | 0 | 8427 |
| hermit | 7 | 0 | 0 | 0 | 0 | 0 | 4106 |
| konclude | 0 | 7 | 0 | 0 | 0 | 0 | N/A |
| openllet | 7 | 0 | 0 | 0 | 0 | 0 | 4020 |
| elk | 7 | 0 | 0 | 0 | 0 | 0 | 3373 |
| jfact | 5 | 2 | 0 | 0 | 0 | 0 | 3427 |
| pellet | 7 | 0 | 0 | 0 | 0 | 0 | 4111 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | disjunctive_simple.owl | 2695 | 54 | success | consistent |
| elk | disjunctive_test.owl | 2923 | 51 | success | consistent |
| elk | hierarchy_100.owl | 2985 | 50 | success | consistent |
| elk | hierarchy_1000.owl | 3025 | 55 | success | consistent |
| elk | hierarchy_10000.owl | 3579 | 130 | success | consistent |
| elk | hierarchy_100000.owl | 5687 | 457 | success | consistent |
| elk | univ-bench.owl | 2717 | 55 | success | consistent |
| hermit | disjunctive_simple.owl | 3275 | 1060 | success | consistent |
| hermit | disjunctive_test.owl | 3323 | 1115 | success | consistent |
| hermit | hierarchy_100.owl | 3391 | 1084 | success | consistent |
| hermit | hierarchy_1000.owl | 3498 | 1214 | success | consistent |
| hermit | hierarchy_10000.owl | 4275 | 1876 | success | consistent |
| hermit | hierarchy_100000.owl | 7636 | 5458 | success | consistent |
| hermit | univ-bench.owl | 3345 | 1041 | success | consistent |
| jfact | disjunctive_simple.owl | 3466 | 78 | success | consistent |
| jfact | disjunctive_test.owl | 3189 | 83 | success | consistent |
| jfact | hierarchy_100.owl | 3178 | 83 | success | consistent |
| jfact | hierarchy_1000.owl | 4110 | 362 | success | consistent |
| jfact | hierarchy_10000.owl | 3856 | 1599 | failed | unknown |
| jfact | hierarchy_100000.owl | 7441 | 4865 | failed | unknown |
| jfact | univ-bench.owl | 3196 | 83 | success | consistent |
| konclude | disjunctive_simple.owl | 2232 | 29 | failed | unknown |
| konclude | disjunctive_test.owl | 2112 | 30 | failed | unknown |
| konclude | hierarchy_100.owl | 2402 | 17 | failed | unknown |
| konclude | hierarchy_1000.owl | 2195 | 41 | failed | unknown |
| konclude | hierarchy_10000.owl | 2325 | 136 | failed | unknown |
| konclude | hierarchy_100000.owl | 3547 | 1233 | failed | unknown |
| konclude | univ-bench.owl | 2221 | 23 | failed | unknown |
| openllet | disjunctive_simple.owl | 2811 | 26 | success | consistent |
| openllet | disjunctive_test.owl | 3020 | 25 | success | consistent |
| openllet | hierarchy_100.owl | 2849 | 25 | success | consistent |
| openllet | hierarchy_1000.owl | 3204 | 28 | success | consistent |
| openllet | hierarchy_10000.owl | 4383 | 69 | success | consistent |
| openllet | hierarchy_100000.owl | 8967 | 244 | success | consistent |
| openllet | univ-bench.owl | 2908 | 30 | success | consistent |
| pellet | disjunctive_simple.owl | 3310 | 18 | success | consistent |
| pellet | disjunctive_test.owl | 2627 | 17 | success | consistent |
| pellet | hierarchy_100.owl | 2838 | 16 | success | consistent |
| pellet | hierarchy_1000.owl | 3222 | 21 | success | consistent |
| pellet | hierarchy_10000.owl | 3644 | 43 | success | consistent |
| pellet | hierarchy_100000.owl | 10648 | 277 | success | consistent |
| pellet | univ-bench.owl | 2492 | 19 | success | consistent |
| tableauxx | disjunctive_simple.owl | 2273 | 11 | success | consistent |
| tableauxx | disjunctive_test.owl | 2113 | 10 | success | consistent |
| tableauxx | hierarchy_100.owl | 2032 | 8 | success | consistent |
| tableauxx | hierarchy_1000.owl | 2083 | 41 | success | consistent |
| tableauxx | hierarchy_10000.owl | 7903 | 5716 | success | consistent |
| tableauxx | hierarchy_100000.owl | 39494 | 37351 | success | consistent |
| tableauxx | univ-bench.owl | 3096 | 15 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
