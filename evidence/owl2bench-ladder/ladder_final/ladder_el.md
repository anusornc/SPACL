# OWL2Bench EL Consistency Scalability Ladder

- Lane: execution performance and completability evidence only (ADR 0024/0026).
- Statistic: repetition median of container-inclusive wall time per engine across three serial passes; cells show `<median>ms (completed/cases)` (ADR 0025).
- Rows: universities ascending. Columns: ADR 0025 engine set for profile EL.
- Inputs: per-dataset comparative aggregations; run ids per point are recorded in ladder_metadata.json.

| Universities | tableauxx | hermit | konclude | elk |
|---|---:|---:|---:|---:|
| 1 | **2793ms (1/1)** | 5499ms (1/1) | 8896ms (1/1) | 5254ms (1/1) |
| 2 | **4269ms (1/1)** | 7227ms (1/1) | 11849ms (1/1) | 5457ms (1/1) |
| 5 | 21287ms (1/1) | 14128ms (1/1) | 23028ms (1/1) | **9402ms (1/1)** |
| 10 | 135372ms (1/1) | 27010ms (1/1) | 44937ms (1/1) | **15450ms (1/1)** |
| 20 | TO (0/1) | 51249ms (1/1) | 85183ms (1/1) | **26646ms (1/1)** |
| 50 | TO (0/1) | 146443ms (1/1) | 219941ms (1/1) | **63609ms (1/1)** |

Legend: `<median>ms (completed/cases)`; **bold** = best median among engines that completed; TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
