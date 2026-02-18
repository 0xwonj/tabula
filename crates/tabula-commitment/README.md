# tabula-commitment

Protocol-level cryptographic primitives for the Tabula kernel (out-of-circuit).

## Role

Computes cryptographic commitments to state: Poseidon2 hashing over
BabyBear, Sparse Merkle Tree, Sorted Sparse Map Commitment (SSMC),
and hybrid per-column strategy dispatch.

The proof crate constrains these same computations in-circuit.
This crate computes them natively.

## Boundary with `tabula-proof`

This crate answers **"what is the state root?"** — it computes the answer.
The proof crate answers **"why is it correct?"** — it proves the computation.

Concretely: this crate runs Poseidon, builds Merkle paths, computes SSMC
digests. The proof crate encodes the same operations as AIR constraints
and verifies them inside a STARK.

## Key Design

**Two-layer hash abstraction.** `tabula-core::Hasher` operates on bytes
(used by the executor). `FieldHasher` in this crate operates on field
elements (used by the commitment layer). `PoseidonHasher` implements both.

**Domain separation.** All hashing is domain-separated: SSMC (0x00),
SMT (0x01), IR hash (0x02), leaf (0x10), tables (0x11), cols (0x12).
This prevents cross-protocol collision.

## Feature Flags

| Feature | Effect |
|---------|--------|
| `stark` | Enables Poseidon2, SMT, SSMC, and Plonky3 field dependencies |
