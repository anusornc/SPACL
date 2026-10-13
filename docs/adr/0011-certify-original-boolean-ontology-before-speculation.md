# Certify the original Boolean ontology before speculation

Status: accepted

Use a bounded deterministic Boolean checker for the exact class-only fragment,
and honor its result before SPACL creates artificial branch witnesses: a class
may be empty without making its ontology inconsistent. Unsupported mixed
fragments retain the legacy path; partial Boolean certificates inside those
branches were rejected because invented witnesses can create false contradictions.

Correcting a known wrong verdict is authorized separately from scheduling
experiments, which must still preserve correct semantics. A certificate bypass
reports no speculative workers, even when AlwaysParallel was requested; it is
not a parallel-speedup result or an extension to full OWL reasoning.
