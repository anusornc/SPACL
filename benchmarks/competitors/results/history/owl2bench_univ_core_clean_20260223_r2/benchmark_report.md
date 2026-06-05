# OWL Reasoner Head-to-Head Report

- Run ID: owl2bench_univ_core_clean_20260223_r2
- Generated: 2026-02-23T15:05:21+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 4 | 0 | 0 | 0 | 0 | 0 | 2263 |
| hermit | 4 | 0 | 0 | 0 | 0 | 0 | 3871 |
| konclude | 0 | 4 | 0 | 0 | 0 | 0 | N/A |
| openllet | 4 | 0 | 0 | 0 | 0 | 0 | 3451 |
| elk | 4 | 0 | 0 | 0 | 0 | 0 | 3179 |
| jfact | 1 | 0 | 3 | 0 | 0 | 0 | 3762 |
| pellet | 4 | 0 | 0 | 0 | 0 | 0 | 2968 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | 0001_UNIV-BENCH-OWL2DL.owl | 3124 | 114 | success | consistent |
| elk | 0002_UNIV-BENCH-OWL2EL.owl | 3239 | 101 | success | consistent |
| elk | 0003_UNIV-BENCH-OWL2QL.owl | 3370 | 101 | success | consistent |
| elk | 0004_UNIV-BENCH-OWL2RL.owl | 2986 | 113 | success | consistent |
| hermit | 0001_UNIV-BENCH-OWL2DL.owl | 4505 | 1559 | success | consistent |
| hermit | 0002_UNIV-BENCH-OWL2EL.owl | 3580 | 1503 | success | consistent |
| hermit | 0003_UNIV-BENCH-OWL2QL.owl | 3459 | 1255 | success | consistent |
| hermit | 0004_UNIV-BENCH-OWL2RL.owl | 3941 | 1633 | success | consistent |
| jfact | 0001_UNIV-BENCH-OWL2DL.owl | 302986 | 302986 | timeout | unknown |
| jfact | 0002_UNIV-BENCH-OWL2EL.owl | 302422 | 302422 | timeout | unknown |
| jfact | 0003_UNIV-BENCH-OWL2QL.owl | 3762 | 218 | success | consistent |
| jfact | 0004_UNIV-BENCH-OWL2RL.owl | 303399 | 303399 | timeout | unknown |
| konclude | 0001_UNIV-BENCH-OWL2DL.owl | 2125 | 72 | failed | unknown |
| konclude | 0002_UNIV-BENCH-OWL2EL.owl | 2007 | 61 | failed | unknown |
| konclude | 0003_UNIV-BENCH-OWL2QL.owl | 2232 | 59 | failed | unknown |
| konclude | 0004_UNIV-BENCH-OWL2RL.owl | 2008 | 56 | failed | unknown |
| openllet | 0001_UNIV-BENCH-OWL2DL.owl | 3836 | 126 | success | consistent |
| openllet | 0002_UNIV-BENCH-OWL2EL.owl | 3479 | 93 | success | consistent |
| openllet | 0003_UNIV-BENCH-OWL2QL.owl | 3071 | 63 | success | consistent |
| openllet | 0004_UNIV-BENCH-OWL2RL.owl | 3420 | 92 | success | consistent |
| pellet | 0001_UNIV-BENCH-OWL2DL.owl | 3002 | 200 | success | consistent |
| pellet | 0002_UNIV-BENCH-OWL2EL.owl | 2879 | 120 | success | consistent |
| pellet | 0003_UNIV-BENCH-OWL2QL.owl | 2858 | 52 | success | consistent |
| pellet | 0004_UNIV-BENCH-OWL2RL.owl | 3135 | 105 | success | consistent |
| tableauxx | 0001_UNIV-BENCH-OWL2DL.owl | 2243 | 14 | success | consistent |
| tableauxx | 0002_UNIV-BENCH-OWL2EL.owl | 2092 | 12 | success | consistent |
| tableauxx | 0003_UNIV-BENCH-OWL2QL.owl | 2509 | 12 | success | consistent |
| tableauxx | 0004_UNIV-BENCH-OWL2RL.owl | 2211 | 15 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
