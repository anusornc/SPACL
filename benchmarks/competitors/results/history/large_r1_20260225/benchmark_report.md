# OWL Reasoner Head-to-Head Report

- Run ID: large_r1_20260225
- Generated: 2026-02-25T15:52:20+07:00
- Suite: large
- Operation: consistency
- Timeout per run: 900s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 3 | 0 | 0 | 0 | 0 | 0 | 5817 |
| hermit | 3 | 0 | 0 | 0 | 0 | 0 | 52469 |
| konclude | 0 | 3 | 0 | 0 | 0 | 0 | N/A |
| openllet | 2 | 0 | 1 | 0 | 0 | 0 | 16273 |
| elk | 3 | 0 | 0 | 0 | 0 | 0 | 15158 |
| jfact | 2 | 1 | 0 | 0 | 0 | 0 | 52623 |
| pellet | 2 | 0 | 1 | 0 | 0 | 0 | 56195 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | doid.owl | 13357 | 345 | success | consistent |
| elk | go-basic.owl | 15711 | 477 | success | consistent |
| elk | uberon.owl | 16406 | 1035 | success | consistent |
| hermit | doid.owl | 20576 | 18228 | success | consistent |
| hermit | go-basic.owl | 17549 | 15534 | success | consistent |
| hermit | uberon.owl | 119284 | 114781 | success | consistent |
| jfact | doid.owl | 61015 | 46484 | success | consistent |
| jfact | go-basic.owl | 44232 | 22970 | success | consistent |
| jfact | uberon.owl | 452454 | 449736 | failed | unknown |
| konclude | doid.owl | 5106 | 2883 | failed | unknown |
| konclude | go-basic.owl | 15479 | 12489 | failed | unknown |
| konclude | uberon.owl | 18637 | 16470 | failed | unknown |
| openllet | doid.owl | 13699 | 203 | success | consistent |
| openllet | go-basic.owl | 18847 | 250 | success | consistent |
| openllet | uberon.owl | 902827 | 902827 | timeout | unknown |
| pellet | doid.owl | 28610 | 204 | success | consistent |
| pellet | go-basic.owl | 83780 | 243 | success | consistent |
| pellet | uberon.owl | 902629 | 902629 | timeout | unknown |
| tableauxx | doid.owl | 3447 | 1176 | success | consistent |
| tableauxx | go-basic.owl | 7329 | 5211 | success | consistent |
| tableauxx | uberon.owl | 6677 | 4461 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
