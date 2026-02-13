# tabula-proof

STARK proof generation and verification for the Tabula kernel (in-circuit).

Given an `ExecutionResult` and state snapshots, generates a STARK proof that
the state transition is correct. Uses Plonky3 over BabyBear.

## Current Status

Phase 1 stub. Contains `ApplyBatchStatement` (public inputs), trait definitions
(`Prover`, `Verifier`), and mock implementations. Real proof logic comes in M5/M6.

## Public Inputs

```rust
ApplyBatchStatement {
    old_state_root: Digest,
    new_state_root: Digest,
    program_root: Digest,
    applied_tx_digest: Digest,
    static_table_root: Digest,
    budgets: ProgramBudgets,
}
```

## Planned (M5: Single-Tx, M6: Batch)

| Module | Phase | Contents |
|--------|-------|----------|
| `witness.rs` | M5 | Execution result → AIR witness columns |
| `air.rs` | M5 | `TabulaAir` — column layout, width-class chips (Narrow/Standard/Wide) |
| `constraints/` | M5 | Per-opcode AIR constraints (Read, Write, Add, Hash, Select, ...) |
| `state_constraints.rs` | M5 | In-circuit SMT/SSMC verification |
| `prover.rs` | M5 | `StarkProver` implementing `Prover` trait |
| `verifier.rs` | M5 | `StarkVerifier` implementing `Verifier` trait |
| `sorted_mem.rs` | M6 | GlobalSortedMem construction |
| `logup.rs` | M6 | LogUp argument — memory consistency, clock binding |
| `write_coalesce.rs` | M6 | WriteSet extraction from GlobalSortedMem |

## Boundary with `tabula-commitment`

This crate uses out-of-circuit primitives from `tabula-commitment`
(Poseidon, SMT, SSMC) to generate witnesses, then proves those computations
were performed correctly using Plonky3 AIR constraints.

## Dependencies

`tabula-core`, `tabula-executor`, `tabula-commitment`.
Will add full `p3-*` stack behind a `stark` feature flag.
