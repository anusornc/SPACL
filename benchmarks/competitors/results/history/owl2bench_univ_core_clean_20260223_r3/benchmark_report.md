# OWL Reasoner Head-to-Head Report

- Run ID: owl2bench_univ_core_clean_20260223_r3
- Generated: 2026-02-23T15:22:08+07:00
- Suite: standard
- Operation: consistency
- Timeout per run: 300s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 4 | 0 | 0 | 0 | 0 | 0 | 2194 |
| hermit | 4 | 0 | 0 | 0 | 0 | 0 | 3585 |
| konclude | 0 | 4 | 0 | 0 | 0 | 0 | N/A |
| openllet | 4 | 0 | 0 | 0 | 0 | 0 | 3184 |
| elk | 4 | 0 | 0 | 0 | 0 | 0 | 2975 |
| jfact | 1 | 0 | 3 | 0 | 0 | 0 | 3507 |
| pellet | 4 | 0 | 0 | 0 | 0 | 0 | 2886 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | 0001_UNIV-BENCH-OWL2DL.owl | 3088 | 116 | success | consistent |
| elk | 0002_UNIV-BENCH-OWL2EL.owl | 2875 | 99 | success | consistent |
| elk | 0003_UNIV-BENCH-OWL2QL.owl | 2916 | 103 | success | consistent |
| elk | 0004_UNIV-BENCH-OWL2RL.owl | 3021 | 110 | success | consistent |
| hermit | 0001_UNIV-BENCH-OWL2DL.owl | 3756 | 1504 | success | consistent |
| hermit | 0002_UNIV-BENCH-OWL2EL.owl | 3520 | 1539 | success | consistent |
| hermit | 0003_UNIV-BENCH-OWL2QL.owl | 3373 | 1297 | success | consistent |
| hermit | 0004_UNIV-BENCH-OWL2RL.owl | 3692 | 1467 | success | consistent |
| jfact | 0001_UNIV-BENCH-OWL2DL.owl | 302410 | 302410 | timeout | unknown |
| jfact | 0002_UNIV-BENCH-OWL2EL.owl | 302409 | 302409 | timeout | unknown |
| jfact | 0003_UNIV-BENCH-OWL2QL.owl | 3507 | 203 | success | consistent |
| jfact | 0004_UNIV-BENCH-OWL2RL.owl | 302314 | 302314 | timeout | unknown |
| konclude | 0001_UNIV-BENCH-OWL2DL.owl | 2143 | 74 | failed | unknown |
| konclude | 0002_UNIV-BENCH-OWL2EL.owl | 2113 | 54 | failed | unknown |
| konclude | 0003_UNIV-BENCH-OWL2QL.owl | 2084 | 78 | failed | unknown |
| konclude | 0004_UNIV-BENCH-OWL2RL.owl | 2302 | 62 | failed | unknown |
| openllet | 0001_UNIV-BENCH-OWL2DL.owl | 3209 | 136 | success | consistent |
| openllet | 0002_UNIV-BENCH-OWL2EL.owl | 3228 | 81 | success | consistent |
| openllet | 0003_UNIV-BENCH-OWL2QL.owl | 3150 | 70 | success | consistent |
| openllet | 0004_UNIV-BENCH-OWL2RL.owl | 3149 | 101 | success | consistent |
| pellet | 0001_UNIV-BENCH-OWL2DL.owl | 2929 | 210 | success | consistent |
| pellet | 0002_UNIV-BENCH-OWL2EL.owl | 2893 | 99 | success | consistent |
| pellet | 0003_UNIV-BENCH-OWL2QL.owl | 2939 | 54 | success | consistent |
| pellet | 0004_UNIV-BENCH-OWL2RL.owl | 2785 | 99 | success | consistent |
| tableauxx | 0001_UNIV-BENCH-OWL2DL.owl | 2123 | 13 | success | consistent |
| tableauxx | 0002_UNIV-BENCH-OWL2EL.owl | 1907 | 13 | success | consistent |
| tableauxx | 0003_UNIV-BENCH-OWL2QL.owl | 2707 | 30 | success | consistent |
| tableauxx | 0004_UNIV-BENCH-OWL2RL.owl | 2042 | 12 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
