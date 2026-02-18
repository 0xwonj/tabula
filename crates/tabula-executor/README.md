# tabula-executor

Deterministic execution engine for the Tabula kernel.

## Role

Executes transactions against a state snapshot, producing read/write
sets and execution events. Interprets IR instructions from `tabula-ir`,
resolves expressions, and manages per-tx overlay with checkpoint/rollback.

Has zero crypto dependencies — all cryptographic operations are injected
via traits from `tabula-core`.

## Key Design

**Deterministic.** Given the same program, state, and batch, execution
always produces the same `ExecutionResult`. No randomness, no system
calls, no non-determinism. This is what makes execution provable.

**Overlay semantics.** Each tx executes against a write-buffer overlay
that provides read-your-writes, read caching, and checkpoint/rollback.
Failed transactions roll back state mutations but preserve the execution
trace (the prover needs to see both success and failure).

**Failure isolation.** A failing tx does not abort the batch. Per-tx
outcomes are recorded individually, and the batch continues. The overlay
checkpoint/rollback mechanism ensures a failed tx leaves no state
side-effects.
