# Optimization

> Status: 🔧 In Progress (Tier 1 unblocked)
> Design: [docs/design/prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md), [docs/design/constraint-compilation.md](../docs/design/constraint-compilation.md), [docs/design/execution-chip-evolution.md](../docs/design/execution-chip-evolution.md), [docs/design/proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md)

## Goal

Proving time < 50% of current baseline. Two categories: infrastructure (any architecture) and sharded-specific.

Current chip widths (post-M10): Execution 278, SSMC 66, Merge 74, SortedMem 67, ColumnMeta 56, Poseidon 93+19pp, RangeCheck 2 = 636 main cols total.

## Tier 1 — Prover Pipeline (Goal 1, no blockers)

Infrastructure optimizations below the chip/AIR layer. Independent of global vs sharded architecture.

Combined effect: ~40% proving time + ~50% memory, ~4 days.

- [ ] BLAKE3 Merkle hash — switch PCS commitment hash from Poseidon2 to BLAKE3 (~30% proving reduction, ~1 day)
- [ ] Batch inversion — Montgomery batch inversion for LogUp permutation trace (~5% proving reduction, ~1 day)
- [ ] Trace clone elimination — transfer ownership or `Arc<RowMajorMatrix>` instead of cloning (~50% memory reduction, ~1 day)
- [ ] Quotient parallelism — `rayon::par_iter` over chips in quotient computation phase (~10% proving reduction, ~1 day)

## Tier 2 — Constraint CSE (no blockers)

> Design: [constraint-compilation.md](../docs/design/constraint-compilation.md)

Independent of architecture. Applies to any `eval()` function.

- [ ] Symbolic DAG extraction — extend SymbolicAirBuilder to collect constraint expressions into hash-consed DAG
- [ ] CSE algorithm — topological sort + refcount-based extraction decision
- [ ] Code generation — proc-macro2/quote to emit optimized `eval_cse()` functions
- [ ] Integration — slot into quotient computation phase, correctness verification vs original `eval()`

**Effect**: 5-15x constraint evaluation speedup, 20-35% total proving time reduction.

## Sharded Architecture Optimizations (Goal 9, blocked on Goal 4)

These apply to the sharded proof model. Depend on sharding migration completing.

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
