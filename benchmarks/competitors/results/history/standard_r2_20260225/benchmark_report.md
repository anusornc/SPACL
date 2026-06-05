# OWL Reasoner Head-to-Head Report

- Run ID: standard_r2_20260225
- Generated: 2026-02-25T14:30:49+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 7 | 0 | 0 | 0 | 0 | 0 | 8636 |
| hermit | 7 | 0 | 0 | 0 | 0 | 0 | 4256 |
| konclude | 0 | 7 | 0 | 0 | 0 | 0 | N/A |
| openllet | 7 | 0 | 0 | 0 | 0 | 0 | 4128 |
| elk | 7 | 0 | 0 | 0 | 0 | 0 | 3594 |
| jfact | 5 | 2 | 0 | 0 | 0 | 0 | 3727 |
| pellet | 7 | 0 | 0 | 0 | 0 | 0 | 4297 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | disjunctive_simple.owl | 3920 | 51 | success | consistent |
| elk | disjunctive_test.owl | 2979 | 54 | success | consistent |
| elk | hierarchy_100.owl | 2816 | 48 | success | consistent |
| elk | hierarchy_1000.owl | 3055 | 55 | success | consistent |
| elk | hierarchy_10000.owl | 3507 | 140 | success | consistent |
| elk | hierarchy_100000.owl | 5816 | 490 | success | consistent |
| elk | univ-bench.owl | 3065 | 57 | success | consistent |
| hermit | disjunctive_simple.owl | 3783 | 1061 | success | consistent |
| hermit | disjunctive_test.owl | 3917 | 1082 | success | consistent |
| hermit | hierarchy_100.owl | 3238 | 1073 | success | consistent |
| hermit | hierarchy_1000.owl | 3341 | 1189 | success | consistent |
| hermit | hierarchy_10000.owl | 4131 | 1854 | success | consistent |
| hermit | hierarchy_100000.owl | 8038 | 5715 | success | consistent |
| hermit | univ-bench.owl | 3345 | 1060 | success | consistent |
| jfact | disjunctive_simple.owl | 4309 | 84 | success | consistent |
| jfact | disjunctive_test.owl | 3572 | 76 | success | consistent |
| jfact | hierarchy_100.owl | 3344 | 91 | success | consistent |
| jfact | hierarchy_1000.owl | 4318 | 378 | success | consistent |
| jfact | hierarchy_10000.owl | 4063 | 1650 | failed | unknown |
| jfact | hierarchy_100000.owl | 7210 | 4985 | failed | unknown |
| jfact | univ-bench.owl | 3093 | 83 | success | consistent |
| konclude | disjunctive_simple.owl | 2500 | 36 | failed | unknown |
| konclude | disjunctive_test.owl | 2305 | 21 | failed | unknown |
| konclude | hierarchy_100.owl | 2210 | 31 | failed | unknown |
| konclude | hierarchy_1000.owl | 2208 | 28 | failed | unknown |
| konclude | hierarchy_10000.owl | 2364 | 126 | failed | unknown |
| konclude | hierarchy_100000.owl | 3548 | 1248 | failed | unknown |
| konclude | univ-bench.owl | 2051 | 34 | failed | unknown |
| openllet | disjunctive_simple.owl | 4049 | 25 | success | consistent |
| openllet | disjunctive_test.owl | 2941 | 26 | success | consistent |
| openllet | hierarchy_100.owl | 3101 | 25 | success | consistent |
| openllet | hierarchy_1000.owl | 3220 | 27 | success | consistent |
| openllet | hierarchy_10000.owl | 3796 | 59 | success | consistent |
| openllet | hierarchy_100000.owl | 8862 | 267 | success | consistent |
| openllet | univ-bench.owl | 2930 | 28 | success | consistent |
| pellet | disjunctive_simple.owl | 3592 | 18 | success | consistent |
| pellet | disjunctive_test.owl | 2659 | 17 | success | consistent |
| pellet | hierarchy_100.owl | 2730 | 17 | success | consistent |
| pellet | hierarchy_1000.owl | 3682 | 20 | success | consistent |
| pellet | hierarchy_10000.owl | 3727 | 42 | success | consistent |
| pellet | hierarchy_100000.owl | 10867 | 245 | success | consistent |
| pellet | univ-bench.owl | 2827 | 18 | success | consistent |
| tableauxx | disjunctive_simple.owl | 2337 | 7 | success | consistent |
| tableauxx | disjunctive_test.owl | 2318 | 12 | success | consistent |
| tableauxx | hierarchy_100.owl | 2096 | 6 | success | consistent |
| tableauxx | hierarchy_1000.owl | 2190 | 43 | success | consistent |
| tableauxx | hierarchy_10000.owl | 8290 | 5889 | success | consistent |
| tableauxx | hierarchy_100000.owl | 40273 | 38136 | success | consistent |
| tableauxx | univ-bench.owl | 2951 | 11 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
