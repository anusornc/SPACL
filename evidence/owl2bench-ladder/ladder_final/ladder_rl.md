# OWL2Bench RL Consistency Scalability Ladder

- Lane: execution performance and completability evidence only (ADR 0024/0026).
- Statistic: repetition median of container-inclusive wall time per engine across three serial passes; cells show `<median>ms (completed/cases)` (ADR 0025).
- Rows: universities ascending. Columns: ADR 0025 engine set for profile RL.
- Inputs: per-dataset comparative aggregations; run ids per point are recorded in ladder_metadata.json.

| Universities | tableauxx | hermit | konclude |
|---|---:|---:|---:|
| 1 | **4574ms (1/1)** | 5835ms (1/1) | 8800ms (1/1) |
| 2 | 12128ms (1/1) | **7806ms (1/1)** | 11862ms (1/1) |
| 5 | 20802ms (1/1) | **16732ms (1/1)** | 23302ms (1/1) |
| 10 | 136782ms (1/1) | **33046ms (1/1)** | 45816ms (1/1) |
| 20 | TO (0/1) | **63167ms (1/1)** | 85931ms (1/1) |
| 50 | TO (0/1) | **165849ms (1/1)** | 217620ms (1/1) |

Legend: `<median>ms (completed/cases)`; **bold** = best median among engines that completed; TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
