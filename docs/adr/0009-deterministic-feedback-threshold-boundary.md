# Deterministic feedback threshold boundary for SPACL

Status: accepted

The active SPACL scheduler remains a static scheduling gate. Its fixed
branch-count and estimated-cost thresholds are evaluated once before reasoning,
and the public names `SchedulingMode::Adaptive` and `adaptive_tuning` do not
denote an observation-driven controller.

Feedback-based threshold work is separated into two experimental phases. Phase
1 is a harness-owned feedback-trained threshold selector that changes
`SpeculativeConfig` only between completed reasoning invocations. Phase 2 is a
possible within-run feedback controller and cannot begin unless Phase 1 passes
its semantic, replay, coverage, and held-out performance gates.

Controller state is explicit, versioned, and portable. A replay consumes the
same canonical feedback tape and must reproduce the same threshold and
scheduling-decision trace. Wall-clock measurements may label harness
observations, but elapsed time, worker completion order, and implicit
host-local state are excluded from the live runtime decision inputs. Invalid or
incompatible controller state falls back to the static scheduling gate without
mutating that state.

The static scheduling gate remains the default throughout both phases.
Experiments are opt-in and may not alter the normalized semantic verdict or
error classification.

## Considered Options

- Connecting the existing `AdaptiveTuner` directly to the reasoner was rejected
  because it is a standalone public helper, consumes externally supplied
  speedup values, and has no deterministic execution or replay contract.
- Updating a controller from live wall-clock samples was rejected because
  measurement noise would make decisions depend on the host and run timing.
- Replacing the static default during the prototype was rejected because the
  performance benefit is not yet established and fallback behavior must remain
  predictable.
- Keying learned state by ontology name or path was rejected because it would
  memorize corpus identity and make state sensitive to local file layout.
- Requiring identical worker interleaving was rejected because semantic and
  controller determinism do not require an identical operating-system schedule.

## Consequences

The first experiment can evaluate feedback without changing the reasoner's
execution path. Its results remain reproducible from a retained manifest and
feedback tape, while fresh timing collection is allowed to show normal
measurement variance. The split also prevents an experimental selector from
being documented as an implemented within-run capability.

The approach requires paired benchmark blocks, canonical state serialization,
explicit trace hashing, and a hard distinction between timing metadata and
decision inputs. It cannot react to workload changes inside one invocation in
Phase 1. Phase 2 adds synchronization cost and is blocked unless Phase 1 shows
a held-out advantage under the locked protocol.

The complete measurement contract is defined in
[SPACL feedback-threshold experiment](../specs/SPACL_FEEDBACK_THRESHOLD_EXPERIMENT.md).
