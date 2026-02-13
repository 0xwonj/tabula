# tabula-executor

Deterministic execution engine for the Tabula kernel.

Executes transactions against a state snapshot, producing read/write sets
and execution events. Has **zero crypto dependencies** — all cryptographic
operations are injected via traits from `tabula-core`.

## Modules

| Module | Responsibility |
|--------|---------------|
| `program` | `Program` — registers tx type definitions, enforces SSA, type checking, and NF-1~NF-4 validation at compile time |
| `interpreter` | `execute()` — single-tx execution loop over IR instructions |
| `batch` | `execute_batch()` — multi-tx batch execution with failure isolation; `BatchEnv` bundles injected trait objects |
| `overlay` | `Overlay` — write buffer with read-your-writes semantics, checkpoint/rollback for tx failure recovery |
| `resolve` | `resolve_row_expr()`, `resolve_value_expr()`, `evaluate_predicate()` — expression resolution |
| `consistency` | `check_consistency()` — validates last-write semantics across execution events |

## Key Invariants

- **True SSA**: Each destination slot is assigned at most once per tx body.
  Enforced at registration time by `Program::register()`.
- **Normal Form (NF)**: Four structural rules enforced at compile time:
  - NF-1: Unique-Read per `(t, c, r)` per tx
  - NF-2: Unique-Write per `(t, c, r)` per tx
  - NF-3: No Read after Write to the same cell
  - NF-4: Row expressions must be provably equal or provably distinct
- **Deterministic execution**: Given the same inputs, always produces
  the same `ExecutionResult`. No randomness, no system calls.

## Dependencies

Only `tabula-core`. No crypto, no IO, no async.
