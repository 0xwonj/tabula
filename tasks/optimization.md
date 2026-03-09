# Optimization

> Status: ⬜ Blocked on [commitment-traits.md](commitment-traits.md), [composition.md](composition.md), [state-traits.md](state-traits.md)
> Design: [docs/design/proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md)

## Goal

Realize Tabula-native optimizations. Target: proving time < 50% of current, global width 261 → ~163 cols.

## Tasks

### ShortRun Routing (KeyRoute)

> Blocked on: extensibility framework

Route single-tx memory accesses to lightweight ShortRunChip instead of GlobalSortedMem.

- [ ] Activate `route_keys()` — produce `KeyRoute::ShortRun`
- [ ] Implement `ShortRunChip<W>` (InitReadWrite, InitWrite patterns)
- [ ] Wire Memory bus + MergeCompleteness bus
- [ ] **Mandatory**: ShortRun = Full equivalence test

**Effect**: ~7% cell reduction

### Width Specialization

> Blocked on: extensibility framework

Per-type optimal width chip instantiation. Currently all global chips use W=3.

- [ ] ProofPlan schema type → W mapping
- [ ] Instantiate `MemoryShard<1>`, `<3>`, `<8>`
- [ ] Keygen deduplication for same-W variants

**Custom type foundation**: Establishes multi-W support. Custom types use the same path.

### NF-Aware Constraint Elision

> Blocked on: composition framework (BusId, ChipExtension)

Selectively remove AIR constraints that the compiler already guarantees via Normal Form rules.

- [ ] PreprocessedCatalog — batch-invariant data
- [ ] ConstraintElision enum (NF-1 through NF-4 variants)
- [ ] Program-specific preprocessed selectors (~15 cols saved)

### D1: Poseidon Chain Delegation — core bottleneck

> Blocked on: ShortRun + Width Spec + NF Elision

Move hash chain computation from StateColumn into PoseidonChip. Largest single optimization.

- [ ] PoseidonChip chain tracking (+3 cols)
- [ ] StateColumnChip hash chain removal (-48 cols)
- [ ] StateColumn reduction/elimination analysis
- [ ] CommitmentVerification bus update

**Effect**: 261 → ~163 cols (38% reduction)

### Pipeline Parallelism

> Blocked on: D1

- [ ] Witness → trace parallel overlap
- [ ] Level 0 independence (Execution/Poseidon/RangeCheck parallel)
- [ ] Deterministic parallel framework

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo bench -p tabula-machine  # performance benchmark
```
