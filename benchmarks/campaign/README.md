# Integrated OWL 2 benchmark campaign

This report layer combines already-generated W3C, ORE, OWL2Bench, and optional query-lane artifacts. It does not launch external corpora or engines.

```bash
W3C_RUN_DIR=/path/to/w3c/run \
ORE_RUN_DIR=/path/to/ore/run \
OWL2BENCH_RUN_DIR=/path/to/owl2bench/run \
OWL2BENCH_QUERY_RUN_DIR=/path/to/query/run \
CAMPAIGN_OUTPUT_DIR=/path/to/campaign/output \
benchmarks/campaign/report.sh
```

Every suite in the campaign summary carries an explicit lane kind (ADR 0024/0027):

- `correctness` lanes: `w3c` and `owl2bench-query`. The campaign correctness gate is the conjunction over these suites only (W3C AND owl2bench-query). A missing or blocked correctness suite blocks the gate and suppresses that lane's performance and scalability claims until its own gate passes.
- `performance` lanes: `ore` and `owl2bench` (stage). Neither lane has an oracle for its consistency workload, so each adjudicates no correctness: its rows stay `oracle_unavailable`, its suite result is informational, and it never blocks or passes the campaign gate. A missing run directory for either lane is reported informationally (`not_available`) and does not block the gate.

Performance-claim permission is scoped per suite: a correctness lane may report performance only after its own gate passes; a performance lane (`ore`, `owl2bench`) may report execution performance (runtime, timeouts, completed-run counts) once execution evidence exists, and makes no correctness claims. The campaign-level `performance_claims_allowed` field is the conjunction over the per-suite permissions.
