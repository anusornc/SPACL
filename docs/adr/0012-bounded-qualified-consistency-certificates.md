# Separate preserved restrictions from bounded reasoning certificates

Status: accepted

Implement the cardinality portion of ADR 0003 with an optional qualifier in each
existing object/data min/max/exact expression and binary v4; old v2/v3 layouts
remain explicit compatibility adapters, not a second runtime semantic model.
Newly preserved restrictions may be unsupported by the reasoner: preserving
syntax must not turn the legacy absence-of-clash fallback into a SAT certificate.

For a bounded object ABox, SAT requires a verified finite model after global
individual merging; UNSAT requires a direct sound contradiction. Exhausting an
incomplete candidate search returns unsupported, and exceeding deterministic
resource limits returns an error. Per-restriction merge checks were rejected
because a merge can violate another role or class constraint. This does not
complete the domain/range portion of ADR 0003 or introduce adaptive scheduling.
