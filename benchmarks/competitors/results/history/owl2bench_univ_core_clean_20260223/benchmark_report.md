# OWL Reasoner Head-to-Head Report

- Run ID: owl2bench_univ_core_clean_20260223
- Generated: 2026-02-23T14:22:01+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 4 | 0 | 0 | 0 | 0 | 0 | 2094 |
| hermit | 4 | 0 | 0 | 0 | 0 | 0 | 3532 |
| konclude | 0 | 4 | 0 | 0 | 0 | 0 | N/A |
| openllet | 4 | 0 | 0 | 0 | 0 | 0 | 3190 |
| elk | 4 | 0 | 0 | 0 | 0 | 0 | 2919 |
| jfact | 1 | 0 | 3 | 0 | 0 | 0 | 3308 |
| pellet | 4 | 0 | 0 | 0 | 0 | 0 | 2876 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | 0001_UNIV-BENCH-OWL2DL.owl | 3012 | 115 | success | consistent |
| elk | 0002_UNIV-BENCH-OWL2EL.owl | 2890 | 98 | success | consistent |
| elk | 0003_UNIV-BENCH-OWL2QL.owl | 2820 | 101 | success | consistent |
| elk | 0004_UNIV-BENCH-OWL2RL.owl | 2954 | 111 | success | consistent |
| hermit | 0001_UNIV-BENCH-OWL2DL.owl | 3703 | 1557 | success | consistent |
| hermit | 0002_UNIV-BENCH-OWL2EL.owl | 3545 | 1449 | success | consistent |
| hermit | 0003_UNIV-BENCH-OWL2QL.owl | 3314 | 1237 | success | consistent |
| hermit | 0004_UNIV-BENCH-OWL2RL.owl | 3566 | 1446 | success | consistent |
| jfact | 0001_UNIV-BENCH-OWL2DL.owl | 302477 | 302477 | timeout | unknown |
| jfact | 0002_UNIV-BENCH-OWL2EL.owl | 302388 | 302388 | timeout | unknown |
| jfact | 0003_UNIV-BENCH-OWL2QL.owl | 3308 | 212 | success | consistent |
| jfact | 0004_UNIV-BENCH-OWL2RL.owl | 302364 | 302364 | timeout | unknown |
| konclude | 0001_UNIV-BENCH-OWL2DL.owl | 2102 | 72 | failed | unknown |
| konclude | 0002_UNIV-BENCH-OWL2EL.owl | 2109 | 68 | failed | unknown |
| konclude | 0003_UNIV-BENCH-OWL2QL.owl | 2709 | 86 | failed | unknown |
| konclude | 0004_UNIV-BENCH-OWL2RL.owl | 2226 | 57 | failed | unknown |
| openllet | 0001_UNIV-BENCH-OWL2DL.owl | 3142 | 105 | success | consistent |
| openllet | 0002_UNIV-BENCH-OWL2EL.owl | 3200 | 92 | success | consistent |
| openllet | 0003_UNIV-BENCH-OWL2QL.owl | 3261 | 77 | success | consistent |
| openllet | 0004_UNIV-BENCH-OWL2RL.owl | 3157 | 117 | success | consistent |
| pellet | 0001_UNIV-BENCH-OWL2DL.owl | 2957 | 204 | success | consistent |
| pellet | 0002_UNIV-BENCH-OWL2EL.owl | 2818 | 96 | success | consistent |
| pellet | 0003_UNIV-BENCH-OWL2QL.owl | 2866 | 52 | success | consistent |
| pellet | 0004_UNIV-BENCH-OWL2RL.owl | 2864 | 107 | success | consistent |
| tableauxx | 0001_UNIV-BENCH-OWL2DL.owl | 2169 | 13 | success | consistent |
| tableauxx | 0002_UNIV-BENCH-OWL2EL.owl | 2064 | 13 | success | consistent |
| tableauxx | 0003_UNIV-BENCH-OWL2QL.owl | 2076 | 12 | success | consistent |
| tableauxx | 0004_UNIV-BENCH-OWL2RL.owl | 2070 | 12 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
