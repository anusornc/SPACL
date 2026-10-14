# OWL2Bench QL Consistency Scalability Ladder

- Lane: execution performance and completability evidence only (ADR 0024/0026).
- Statistic: repetition median of container-inclusive wall time per engine across three serial passes; cells show `<median>ms (completed/cases)` (ADR 0025).
- Rows: universities ascending. Columns: ADR 0025 engine set for profile QL.
- Inputs: per-dataset comparative aggregations; run ids per point are recorded in ladder_metadata.json.

| Universities | tableauxx | hermit | konclude |
|---|---:|---:|---:|
| 1 | **2338ms (1/1)** | 4955ms (1/1) | 8900ms (1/1) |
| 2 | **2556ms (1/1)** | 6187ms (1/1) | 11858ms (1/1) |
| 5 | **3625ms (1/1)** | 10554ms (1/1) | 23303ms (1/1) |
| 10 | **5446ms (1/1)** | 18724ms (1/1) | 45556ms (1/1) |
| 20 | **8946ms (1/1)** | 34320ms (1/1) | 85495ms (1/1) |
| 50 | **20358ms (1/1)** | 87197ms (1/1) | 224809ms (1/1) |

Legend: `<median>ms (completed/cases)`; **bold** = best median among engines that completed; TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
