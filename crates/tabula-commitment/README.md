# tabula-commitment

Protocol-level cryptographic primitives for the Tabula kernel (out-of-circuit).

This crate answers **"what is the state root?"** — it computes cryptographic
commitments to state. The proof crate answers **"why is it correct?"** by
proving these computations in-circuit.

## Current Status

Phase 1 placeholder. Contains mock trait implementations and trivial
hash-based commitment functions. These will be replaced by the real
protocol crypto described below.

## Planned (M4: Plonky3 Foundation)

| Module | Contents | Implements |
|--------|----------|------------|
| `poseidon.rs` | Poseidon2 over BabyBear | `Hasher` trait |
| `baby_bear_codec.rs` | Schema-typed 3-limb encoding | `ValueCodec` trait |
| `smt.rs` | 64-level Sparse Merkle Tree | — |
| `ssmc.rs` | Sorted Sparse Map Commitment + streaming Poseidon | — |
| `hybrid.rs` | Per-column strategy dispatch (SSMC / SMT) | — |
| `membership.rs` | SMT-based program root | `MembershipScheme` trait |
| `digest.rs` | Poseidon-based batch digest | `BatchDigester` trait |

## Boundary with `tabula-proof`

| | commitment | proof |
|---|---|---|
| Poseidon hash | Computes it | Constrains it in AIR |
| SMT membership | Generates Merkle proof | Verifies proof in-circuit |
| SSMC commitment | Computes via streaming hash | Constrains in AIR |
| STARK proof | — | Generates / verifies |

## Dependencies

`tabula-core`. Will add `p3-field`, `p3-baby-bear`, `p3-poseidon2` behind
a `stark` feature flag (no Plonky3 AIR/STARK dependencies).

## Features

| Feature | Effect |
|---------|--------|
| `mock` | Enables `mock` module with Blake3-based test implementations |
