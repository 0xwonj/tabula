# Optimization

> Status: 🔧 In Progress
> Design: [docs/design/prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md), [docs/design/constraint-compilation.md](../docs/design/constraint-compilation.md), [docs/design/execution-chip-evolution.md](../docs/design/execution-chip-evolution.md), [docs/design/proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md)

## Goal

Proving time < 50% of current baseline. Two categories: infrastructure (any architecture) and sharded-specific.

Current chip widths (post-M10): Execution 278, SSMC 66, Merge 74, SortedMem 67, ColumnMeta 56, Poseidon 93+19pp, RangeCheck 2 = 636 main cols total.

## Tier 1a — Prover Pipeline (Goal 1) ✅

Infrastructure optimizations below the chip/AIR layer. Independent of architecture.

- [x] BLAKE3 Merkle hash — PCS commitment hash Poseidon2 → BLAKE3 (~30% proving reduction)
  - `machine/src/blake3_pcs.rs`: `Blake3FieldHasher`, `Blake3FieldCompressor`
  - MMCS uses scalar `BabyBear` packing; Poseidon2 remains for Fiat-Shamir + in-circuit
- [x] Trace ownership transfer — `ProofInstance::new()` takes `TraceMap` by value (~50% memory reduction)
  - `collect_chip_infos()` uses `traces.remove()` instead of `get()` + clone
  - `TabulaMachine::prove()` takes `ProofTraces` by value, destructures into per-tier TraceMaps

## Tier 1b — Parallelization + Batch Inversion (Goal 1 continuation) ✅

> Design: [prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md) §Parallelization, §Batch Inversion

All items use rayon for adaptive work-stealing.

### Parallelization

- [x] P-1: `compute_chip_quotients()` — chip-level `par_iter` in `proof_instance.rs`
- [x] P-2: `prove_impl()` — cross-proof sub-proof parallelism via `into_par_iter` in `machine.rs`
- [x] P-3: `build_perm_traces()` — chip-level `par_iter_mut` in `proof_instance.rs`
- [x] P-4: `build_proof_traces()` — column-level `into_par_iter` in `setup.rs`
- [x] P-5: `verify_impl()` — column sub-proof parallel verification via `par_iter` in `machine.rs`

### Batch Inversion

- [x] Montgomery batch inversion in `stark/src/permutation/trace.rs` — prefix-product algorithm replaces N independent EF4 divisions with 1 inversion + 3(N-1) multiplications

## Tier 2 — Constraint CSE (no blockers)

> Design: [constraint-compilation.md](../docs/design/constraint-compilation.md)

Independent of architecture. Applies to any `eval()` function.

- [ ] Symbolic DAG extraction — extend SymbolicAirBuilder to collect constraint expressions into hash-consed DAG
- [ ] CSE algorithm — topological sort + refcount-based extraction decision
- [ ] Code generation — proc-macro2/quote to emit optimized `eval_cse()` functions
- [ ] Integration — slot into quotient computation phase, correctness verification vs original `eval()`

**Effect**: 5-15x constraint evaluation speedup, 20-35% total proving time reduction.

## Tier 3 — GKR for LogUp (protocol-level, deferred)

> Design: [prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md) §GKR
> Detail: [research.md](research.md) §GKR

Replace committed permutation trace with GKR sum-check protocol. Eliminates permutation trace NTT + Merkle commit entirely.

**Status**: Deferred. Apply Tier 1b first (parallelization + batch inversion), then re-measure perm cost fraction. If still >10% of proving time, proceed with GKR. Also monitoring ecosystem (OpenVM v2, Plonky3 sum-check support).

**Blocker**: No FRI+BabyBear GKR-LogUp production implementation exists. Stwo uses Circle STARKs (different backend). Would require custom sum-check implementation with soundness risk.

**Effect**: 20-30% PCS cost reduction. **Effort**: ~4-5 weeks.

## Sharded Architecture Optimizations (Goal 9)

These apply to the sharded proof model. Depend on sharding migration completing (✅ done).

### D1: Poseidon Chain Delegation (per-column)

> Design: [proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md)

Move hash chain computation from StateShard into PoseidonLocal within each column proof.

- [ ] PoseidonLocal chain tracking (+3 cols)
- [ ] StateShard hash chain column removal
- [ ] Per-column proof width: 236→~191 cols (19% reduction per column)

### Coprocessor Factoring (Execution Tier 1)

> Design: [execution-chip-evolution.md](../docs/design/execution-chip-evolution.md)

Extract Mul/DivMod/Cmp/Hash into bus-linked coprocessor chips within the execution proof.

- [ ] CmpChip — 6 sub-ops, StrictIneq, Limb2Bits
- [ ] MulChip — carry chain
- [ ] DivModChip — dual-slot write
- [ ] Execution proof width: 278→~100 cols

### NF-Aware Constraint Elision

> Design: [execution-chip-evolution.md](../docs/design/execution-chip-evolution.md)

Remove AIR constraints guaranteed by compiler NF rules.

- [ ] PreprocessedCatalog — batch-invariant data
- [ ] ConstraintElision enum (NF-1 through NF-4 variants)
- [ ] Program-specific preprocessed selectors (~15 cols saved)

### Column-Level Optimizations (sharding-native)

- [ ] Untouched column skip — columns not accessed in batch require zero proving cost
- [ ] Read-only column optimization — no MemoryShard needed, just value verification in execution proof
- [ ] Dynamic `max_slot` — ExecutionChip column subsetting (278→~150 for typical programs)

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo bench -p tabula-machine  # performance benchmark
```
