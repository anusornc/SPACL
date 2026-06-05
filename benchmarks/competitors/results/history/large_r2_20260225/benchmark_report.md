# OWL Reasoner Head-to-Head Report

- Run ID: large_r2_20260225
- Generated: 2026-02-25T22:44:24+07:00
- Suite: large
- Operation: consistency
- Timeout per run: 900s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 3 | 0 | 0 | 0 | 0 | 0 | 6258 |
| hermit | 3 | 0 | 0 | 0 | 0 | 0 | 48880 |
| konclude | 0 | 3 | 0 | 0 | 0 | 0 | N/A |
| openllet | 2 | 0 | 1 | 0 | 0 | 0 | 15981 |
| elk | 3 | 0 | 0 | 0 | 0 | 0 | 15102 |
| jfact | 2 | 1 | 0 | 0 | 0 | 0 | 50940 |
| pellet | 2 | 0 | 1 | 0 | 0 | 0 | 56067 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | doid.owl | 13534 | 289 | success | consistent |
| elk | go-basic.owl | 16456 | 574 | success | consistent |
| elk | uberon.owl | 15318 | 835 | success | consistent |
| hermit | doid.owl | 20295 | 18034 | success | consistent |
| hermit | go-basic.owl | 17616 | 15354 | success | consistent |
| hermit | uberon.owl | 108729 | 104610 | success | consistent |
| jfact | doid.owl | 60443 | 45482 | success | consistent |
| jfact | go-basic.owl | 41438 | 20974 | success | consistent |
| jfact | uberon.owl | 448703 | 446289 | failed | unknown |
| konclude | doid.owl | 4932 | 2846 | failed | unknown |
| konclude | go-basic.owl | 14289 | 12120 | failed | unknown |
| konclude | uberon.owl | 19590 | 17356 | failed | unknown |
| openllet | doid.owl | 14266 | 259 | success | consistent |
| openllet | go-basic.owl | 17697 | 222 | success | consistent |
| openllet | uberon.owl | 902598 | 902598 | timeout | unknown |
| pellet | doid.owl | 28896 | 252 | success | consistent |
| pellet | go-basic.owl | 83238 | 189 | success | consistent |
| pellet | uberon.owl | 902362 | 902362 | timeout | unknown |
| tableauxx | doid.owl | 3412 | 1195 | success | consistent |
| tableauxx | go-basic.owl | 8000 | 5675 | success | consistent |
| tableauxx | uberon.owl | 7363 | 5117 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
