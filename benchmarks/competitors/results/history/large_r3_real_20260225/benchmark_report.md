# OWL Reasoner Head-to-Head Report

- Run ID: large_r3_real_20260225
- Generated: 2026-02-26T09:47:19+07:00
- Suite: large
- Operation: consistency
- Timeout per run: 900s
- Reasoners: tableauxx hermit konclude openllet elk jfact pellet
- Primary metric: wall time (ms)

## Summary by Reasoner

| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|
| tableauxx | 3 | 0 | 0 | 0 | 0 | 0 | 5769 |
| hermit | 3 | 0 | 0 | 0 | 0 | 0 | 49259 |
| konclude | 0 | 3 | 0 | 0 | 0 | 0 | N/A |
| openllet | 2 | 0 | 1 | 0 | 0 | 0 | 16041 |
| elk | 3 | 0 | 0 | 0 | 0 | 0 | 14992 |
| jfact | 2 | 1 | 0 | 0 | 0 | 0 | 50979 |
| pellet | 2 | 0 | 1 | 0 | 0 | 0 | 55838 |

## Detailed Results

| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |
|---|---|---:|---:|---|---|
| elk | doid.owl | 13406 | 355 | success | consistent |
| elk | go-basic.owl | 15976 | 451 | success | consistent |
| elk | uberon.owl | 15594 | 889 | success | consistent |
| hermit | doid.owl | 19180 | 17073 | success | consistent |
| hermit | go-basic.owl | 18039 | 15718 | success | consistent |
| hermit | uberon.owl | 110560 | 107683 | success | consistent |
| jfact | doid.owl | 60112 | 45596 | success | consistent |
| jfact | go-basic.owl | 41847 | 21565 | success | consistent |
| jfact | uberon.owl | 453188 | 450579 | failed | unknown |
| konclude | doid.owl | 5008 | 2837 | failed | unknown |
| konclude | go-basic.owl | 14416 | 12227 | failed | unknown |
| konclude | uberon.owl | 18505 | 16240 | failed | unknown |
| openllet | doid.owl | 14336 | 334 | success | consistent |
| openllet | go-basic.owl | 17747 | 207 | success | consistent |
| openllet | uberon.owl | 902670 | 902670 | timeout | unknown |
| pellet | doid.owl | 28771 | 235 | success | consistent |
| pellet | go-basic.owl | 82906 | 195 | success | consistent |
| pellet | uberon.owl | 902397 | 902397 | timeout | unknown |
| tableauxx | doid.owl | 3701 | 1235 | success | consistent |
| tableauxx | go-basic.owl | 7059 | 4945 | success | consistent |
| tableauxx | uberon.owl | 6548 | 4331 | success | consistent |

## Notes

- Wall time is the primary metric for cross-engine comparison.
- OWLAPI/Konclude internal reported times are kept as secondary diagnostics.
- `pellet` may appear as `not_available` when legacy artifact repositories are unavailable.
