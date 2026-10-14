# ORE 2015 Consistency Lane

This lane executes the ORE 2015 consistency corpus while preserving the
official `pure_dl` and `el` group labels. The archive's consistency metadata
maps each `ore2015_filename` to the shared `pool_sample/files` directory; the
prepare step reads that mapping and stages only the requested consistency
groups.

```bash
ORE2015_AUTO_DOWNLOAD=1 \
ORE2015_GROUPS=pure_dl,el \
benchmarks/external/ore/prepare.sh

RUN_ID=ore2015_full_tableauxx_YYYYMMDD \
REASONERS_OVERRIDE=tableauxx \
TIMEOUT_SECONDS=60 \
benchmarks/external/ore/run.sh all run

benchmarks/external/ore/report.sh ore2015_full_tableauxx_YYYYMMDD
```

## Performance-lane contract (ADR 0024)

This is a performance lane: it adjudicates no correctness and no public
oracle exists for the ORE 2015 consistency sample. Every row keeps
`oracle_unavailable` as its permanent correctness status, and the lane can
never pass or block a correctness gate. Its claims are execution performance
and completability only.

The report is performance-first:

- The summary TSV leads, per `local_group`/`official_group`/`ore_track`/
  `reasoner`, with completed-run counts, timeouts, failures, not-available
  counts, and an `other_execution` bucket so every execution outcome is
  accounted for; `pure_dl` and `el` stay distinguishable rows.
- Timing is summarized only where the run records it: each timing field
  (`wall_time_ms`, `reported_time_ms`, `parse_time_ms`, `reason_time_ms`)
  carries an explicit `*_recorded` count plus `min`/`mean`/`max` over the
  recorded values. `-1` means not recorded; unrecorded timing is never
  coerced or invented.
- The correctness status counts (`passed`, `wrong`, `oracle_unavailable`,
  `not_evaluated`, `unsupported`) are retained after the performance columns
  as the standing no-oracle record; `passed` and `wrong` are unreachable on
  this lane by design.
- The details TSV keeps its columns unchanged, with every row's correctness
  status `oracle_unavailable`.
- The report's Differential Appendix is a contract-only, clearly labelled
  section reserved for future comparative runs: engine-output comparisons
  will be observational evidence with no gate authority, never correctness.
