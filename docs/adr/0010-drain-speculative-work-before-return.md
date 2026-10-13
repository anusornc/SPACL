# Drain speculative work before returning

Status: accepted

A SPACL invocation drains its spawned worker loops after signaling shutdown,
including when result collection times out. This makes a subsequent invocation
safe from unfinished work belonging to its predecessor and permits verified
warm-pool diagnostics. The configured timeout initiates cancellation; completion
can take longer while already-running finite checks finish.

Returning on a second cleanup timer was rejected because it would leave shared
state mutable by old workers. Completion acknowledgments, rather than elapsed
time, define the cleanup boundary. This decision does not introduce adaptive
scheduling or extend the reasoner's supported semantic fragment.
