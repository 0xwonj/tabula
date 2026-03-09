# Tabula Implementation Workplan

> **Version**: 5.1
> **Date**: 2026-03-08
> **References**: [master-roadmap.md](master-roadmap.md), [extensibility-architecture.md](extensibility-architecture.md), [tabula-machine-architecture.md](tabula-machine-architecture.md)
> **Scope**: Concrete implementation tasks derived from architecture analysis and SP1/OpenVM comparison study

---

## Context

This document consolidates all planned work into a single actionable list. It derives from:

1. **Architecture comparison** (Tabula vs SP1 vs OpenVM) — trait hierarchies, chip patterns, machine abstractions
2. **Code review** of `crates/machine/src/` — R1-R7 refactoring (completed)
3. **Extensibility analysis** — OpcodeHandler vs Precompile tradeoff (Precompile chosen)
4. **Constraint audit** — compiler-enforced vs ZK-verified separation (verified correct)
5. **Composition model analysis** — compile-time enum vs runtime registry (Registry chosen)
6. **ColumnCommitment trait analysis** — pluggable per-column commitment schemes via bus protocol boundary

### Version History

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-03-06 | Phase 1 closed, 1.8 moved to 2.5 |
| 3.0 | 2026-03-07 | Holistic Phase 2 redesign: `ColumnCommitment` trait replaces `ColumnStrategy` enum. SSMC/SMT become implementations, not special cases. Bus-driven collection replaces hardcoded Poseidon/RangeCheck. Phase 3.3 absorbed into Phase 2.1. Parallel trace pipeline restructured around per-column independence. |
| 4.0 | 2026-03-07 | **Major revision**: Research proved pure shard migration causes 18-20x proof size explosion. Phase 2 revised: global chips KEPT as default, shard chips for extensibility only. |
| 5.0 | 2026-03-08 | **Architecture refinement**: StateColumnChip moved from core to `SsmcCommitment` plugin. SSMC/SMT as equal-citizen plugins via `ColumnCommitment` trait, not hardcoded. Internal `MemoryModel` and `RootProof` traits added (`pub(crate)`) for Tabula's own development. Builder API: `with_core_chips()` + `with_commitment()`. Layered composition: Core (fixed) + Column Commitment (pluggable) + Bus Consumers (auto-collected). |
| 5.1 | 2026-03-08 | **Phase 2 complete**: `CommitmentScheme` trait (machine-level) with `SsmcScheme`/`SmtScheme` impls. Witness pipeline confirmed working (no revert needed). `MemoryModel` + `RootProof` traits implemented (public, in `composition.rs`). `BusConsumer` trait wired for PoseidonChip/RangeCheckChip. `ProveError` replaces panics. 49 tests passing. |

### Key Architectural Decisions

| Decision | Rationale |
|----------|-----------|
| **Runtime `ChipRegistry` over compile-time `define_chip_set!`** | OpenVM proves `dyn AnyRap` works in production. Eliminates need for proc macro, simpler extension composition, negligible vtable overhead (<1% of eval cost). See [Decision Record: Composition Model](#composition-model-compile-time-enum-vs-runtime-registry). |
| **Remove `define_chip_set!`, `ChipSet` trait, `TabulaAir` enum, `StarkAir` alias** | All superseded by `AnyRap` + `ChipRegistry`. Less macro magic, fewer traits, same capability. |
| **Precompile pattern for new operations** | Hash and Lookup already demonstrate this. New computations = new chip + bus. See [Decision Record: Precompile vs OpcodeHandler](#precompile-vs-opcodehandler). |
| **No per-opcode chips** | SSA slot carry requires 112+ shared columns per chip. 12 core opcodes are stable. |
| **All current AIR constraints are essential** | Audit confirmed zero redundancy with compiler NF rules. See [Constraint Audit](#constraint-audit-results). |

---

## Trait Hierarchy (Target)

After all phases, the proof system traits form three layers:

```
Individual chip traits (implemented by each chip struct):
  ChipSpec           — chip_id(), chip_name(), has_interactions(), ...
  BaseAir<F>         — width()
  Air<AB>            — eval() for each builder type
  TraceContributor   — phase(), contribute()

Type-erased traits (blanket-implemented, used by registry/machine):
  AnyRap             — bundles ChipSpec + all Air<AB> bounds into one object-safe trait
  DynTraceContributor — object-safe wrapper for TraceContributor

Pluggable commitment (per-column proof strategy):
  ColumnCommitment   — register shard chips, populate witness, build traces
  BusConsumer        — declare bus dependencies, collect interaction records
  (SSMC, SMT are implementations — not special cases)

Composition (replaces define_chip_set! + ChipSet + TabulaAir):
  ChipRegistry       — dynamic chip registration, bus manifest, setup validation
  ChipExtension      — extension package (chips + buses + commitment schemes)
  ProofPlan          — per-(t,c) commitment scheme selection + width class
  TabulaMachine      — owns registry + config + plan, provides prove/verify

Removed:
  ChipSet trait       — replaced by ChipRegistry
  StarkAir trait      — replaced by AnyRap
  define_chip_set!    — replaced by ChipRegistry
  TabulaAir enum      — replaced by core_chips() function
```

---

## Work Items

### Phase 1: Machine Layer — COMPLETE

> **Goal**: Soundness, shared PCS, single FRI, registry-based composition.
> **Status**: Complete. 36 tests passing, zero clippy warnings.

| Task | Status | Summary |
|------|--------|---------|
| 1.1 AnyRap | **Done** | `AnyRap` trait with blanket impl (`machine/src/any_rap.rs`). `DynChip` (in `stark/src/trace/dyn_chip.rs`) serves as the witness-side equivalent of `DynTraceContributor`. |
| 1.2 ChipRegistry + TabulaMachine | **Done** | `ChipRegistry`, `RegisteredChip`, `TabulaMachine` with builder pattern, `core_chips()` function. |
| 1.3 Remove Superseded Abstractions | **Done** | `define_chip_set!`, `ChipSet`, `TabulaAir`, `StarkAir` — all deleted. Zero references in codebase. `chip_set.rs`, `chip_instance.rs` removed. |
| 1.4 ProvingKey / VerifyingKey | **Done** | `TabulaProvingKey`, `TabulaVerifyingKey`, `verify_with_key()`. Keys cached at `TabulaMachine::build()`. |
| 1.5 Directory Structure Cleanup | **Done** | `permutation/` (mod, challenges, trace, tests), `prove/` (mod, quotient, rap_folder), `verify/` (mod, rap_folder). |
| 1.6 RAP Phase Abstraction | **Dropped** | Single RAP implementation (LogUp) with no second consumer planned. Trait abstraction would add indirection without value. Prover/verifier construct `RapProverFolder`/`RapVerifierFolder` directly. |
| 1.7 Shared PCS + Single FRI | **Done** | 3-round batched protocol: commit main → sample LogUp challenges → commit perm → sample alpha → commit quotients → sample zeta → single FRI open. `TabulaProof` with shared commitments. |
| 1.8 Dependent Chip Collection | **→ Moved to 2.4** | Deferred to Phase 2 where witness pipeline is redesigned. Generalized as `BusConsumer` trait in 2.4 (Bus-Driven Collection). |

**Completion gate results:**
- Single FRI proof ✅
- C1 soundness resolved (perm trace PCS-committed) ✅
- `define_chip_set!`/`ChipSet`/`TabulaAir`/`StarkAir` deleted ✅
- 36 tests passing (17 unit + 2 any_rap + 11 machine + 6 E2E) ✅

**Note on `p3-uni-stark`**: Task 1.7.4 proposed removing this dependency. In practice, utility types (`StarkConfig`, `ProverConstraintFolder`, `VerifierConstraintFolder`, `SymbolicAirBuilder`, etc.) are still used as building blocks — the per-chip `p3_uni_stark::prove()`/`verify()` are no longer called. Full removal would require reimplementing ~10 types with no soundness benefit.

---

### Phase 2: Extensibility Infrastructure + Code Quality — COMPLETE

> **Goal**: CommitmentScheme trait for pluggable commitment schemes, BusConsumer for bus-driven
> collection, code quality improvements. **Global chips kept as default** — no migration.
> **Status**: Complete. 49 tests passing, zero clippy warnings.
> **Ref**: [master-roadmap.md §6](master-roadmap.md#6-phase-2-extensibility-infrastructure),
>         [commitment-architecture-research.md](commitment-architecture-research.md)

#### Design Rationale

Research ([commitment-architecture-research.md](commitment-architecture-research.md)) proved pure per-column
sharding causes **18-20x proof size explosion** (261 cols fixed → C×237 for C=50 columns).

The central insight: **bus protocol is the interface boundary**. Commitment schemes are pluggable —
each receives from Memory bus and sends on CommitVerif bus. SSMC and SMT are default-provided
plugins, not hardcoded core. StateColumnChip is SSMC's implementation detail, not a core chip.

**What changed from v4.0**:
- StateColumnChip moved from core to `SsmcCommitment` plugin
- SSMC/SMT register via `with_commitment()`, same API as custom schemes
- Internal `MemoryModel` and `RootProof` traits added (`pub(crate)`)
- Builder API: `with_core_chips()` + `with_commitment()` + `with_default_commitments()`

```
┌──────────────────────────────────────────────────────────────┐
│  Layer 0: Core (fixed — Tabula's identity)                   │
│  Execution:     ExecutionChip, StaticTableChip                │
│  Memory:        InterTxOrderChip  (MemoryModel trait, internal)│
│  Root Proof:    ColumnMetaChip, SmtColPathChip,               │
│                 SmtTablePathChip  (RootProof trait, internal)  │
│  Bus Consumers: PoseidonChip, RangeCheckChip (BusConsumer)    │
├──────────────────────────────────────────────────────────────┤
│  Layer 1: Column Commitment (pluggable — app choice)         │
│  ColumnCommitment trait (batch API)                           │
│    "ssmc" → SsmcCommitment → StateColumnChip (global)        │
│    "smt"  → SmtCommitment  → (no extra chip)                 │
│    "custom" → CustomCommitment → CustomChip                  │
│  Bus contract: Memory bus receive → CommitVerif bus send      │
├──────────────────────────────────────────────────────────────┤
│  Layer 2: Bus Consumers (auto-collected via BusConsumer)      │
│  PoseidonChip, RangeCheckChip, ...extensible                  │
└──────────────────────────────────────────────────────────────┘
```

#### Trait Design (Implemented)

Two-level trait design avoids circular deps between `tabula-stark` and `tabula-machine`:

```rust
// Machine-level (tabula-machine/src/composition.rs) — provides AIRs + DynChips for builder
pub trait CommitmentScheme: Send + Sync {
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;      // for proving/verifying
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>; // for trace building
}
// Impls: SsmcScheme (StateColumnChip), SmtScheme (empty)

pub trait MemoryModel: Send + Sync {
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;
}
// Impl: GlobalSortedMemory (InterTxOrderChip)

pub trait RootProof: Send + Sync {
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;
}
// Impl: SmtRootProof (ColumnMetaChip + SmtColPathChip + SmtTablePathChip)

// Stark-level (tabula-stark/src/trace/column_commitment.rs) — shard-style trace building
// ColumnCommitment trait (pre-existing, for per-column batch trace building)
// BusConsumer trait (pre-existing, for interaction collection)
```

#### Builder API (Implemented)

```rust
// Default usage (9 chips: 8 core + 1 SSMC)
let machine = TabulaMachine::builder()
    .with_core_chips()            // Layer 0: exec + memory + root proof + bus consumers
    .with_default_commitments()   // Layer 1: SsmcScheme (StateColumnChip)
    .build()?;

// Custom commitment scheme
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_commitment(&SsmcScheme)           // explicit SSMC
    .with_commitment(&MyAccumulatorScheme)   // custom scheme
    .build()?;

// Granular layer control
let machine = TabulaMachine::builder()
    .with_execution()                       // ExecutionChip + StaticTableChip
    .with_memory_model(&GlobalSortedMemory) // InterTxOrderChip
    .with_root_proof(&SmtRootProof)         // ColumnMeta + SmtPath chips
    .with_bus_consumers()                   // Poseidon + RangeCheck
    .with_commitment(&SsmcScheme)
    .build()?;
```

#### 2.0 Witness Pipeline — No Revert Needed

Investigation confirmed the witness pipeline was already working correctly after Phase 1.
The files referenced below use `BusConsumer` trait (from `tabula-stark::trace`) and compile cleanly.
No revert was necessary.

| Task | Status |
|------|--------|
| 2.0.1–2.0.6 | **Skipped** — witness pipeline functional, `cargo check -p tabula-witness` passes |

#### 2.1 Core Trait Definitions

| Task | Status | Notes |
|------|--------|-------|
| 2.1.1 | **Done** (pre-existing) | `ColumnCommitment` trait already in `tabula-stark::trace::column_commitment`. |
| 2.1.2 | **Done** (pre-existing) | `BusConsumer` trait already in `tabula-stark::trace`. |
| 2.1.3 | **Done** | `MemoryModel` trait in `machine/src/composition.rs`. Impl: `GlobalSortedMemory` wrapping InterTxOrderChip. |
| 2.1.4 | **Done** | `RootProof` trait in `machine/src/composition.rs`. Impl: `SmtRootProof` wrapping ColumnMeta + SmtPath chips. |
| 2.1.5 | **Deferred** | `ColumnPlan` exists in `tabula-stark` but not yet used by machine builder. Not needed for Phase 2 gate. |
| 2.1.6 | **Deferred** | `ProofPlan` dispatch deferred to Phase 3/4. Current default: all columns use SSMC. |

#### 2.2 Commitment Scheme Separation

StateColumnChip moved from core to `SsmcScheme`. SSMC/SMT register via `CommitmentScheme` trait.

**Design note**: Machine-level `CommitmentScheme` trait (in `composition.rs`) is distinct from
lower-level `ColumnCommitment` trait (in `tabula-stark`). `CommitmentScheme` provides `airs()` +
`dyn_chips()` for the builder. `ColumnCommitment` provides `build_traces()` for shard-style trace
building. This avoids circular deps (`AnyRap` is in tabula-machine, can't be referenced from tabula-stark).

| Task | Status | Notes |
|------|--------|-------|
| 2.2.1 | **Done** | `SsmcScheme` implements `CommitmentScheme`. Owns `StateColumnChip::<3>`. |
| 2.2.2 | **Done** | `SmtScheme` implements `CommitmentScheme`. Returns empty `airs()`/`dyn_chips()`. |
| 2.2.3 | **Done** | `with_core_chips()` registers Layer 0 only (8 chips). No StateColumnChip. |
| 2.2.4 | **Done** | `with_commitment(&dyn CommitmentScheme)` registers scheme's AIRs + DynChips. |
| 2.2.5 | **Done** | `with_default_commitments()` delegates to `with_commitment(&SsmcScheme)`. |
| 2.2.6 | **Done** | 8 integration tests: scheme isolation, equivalence, prove/verify pipeline. |

#### 2.3 Bus-Driven Collection

| Task | Status | Notes |
|------|--------|-------|
| 2.3.1 | **Done** | `PoseidonChip` implements `BusConsumer`. Registered via `with_bus_consumers()`. |
| 2.3.2 | **Done** | `RangeCheckChip` implements `BusConsumer`. Registered via `with_bus_consumers()`. |
| 2.3.3 | **Done** | Builder's `with_bus_consumer()` method for custom consumers. Orchestration uses trait. |

#### 2.4 Machine Code Quality

| Task | Status | Notes |
|------|--------|-------|
| 2.4.1 | **Done** | `ProveError` enum in `proof.rs`. Prover returns `Result<TabulaProof, ProveError>`. |
| 2.4.2 | **Done** | RAP folder fields encapsulated — private fields + getters in `prove/rap_folder.rs` and `verify/rap_folder.rs`. |
| 2.4.3 | **Done** | PCS ceremony extracted into `pcs_commit_and_open()` and `pcs_verify_and_recompose()`. |
| 2.4.4 | **Done** | `prove/` (mod, quotient, rap_folder) and `verify/` (mod, rap_folder) modules. |

**Phase 2 completion gate** (all met):
- ✅ All existing tests pass (49 total: 17 unit + 2 any_rap + 24 machine + 6 E2E)
- ✅ StateColumnChip owned by `SsmcScheme`, not core
- ✅ SSMC registered via `CommitmentScheme` trait (no hardcoded dispatch)
- ✅ `BusConsumer` for PoseidonChip and RangeCheckChip
- ✅ `MemoryModel` and `RootProof` trait boundaries exist
- ✅ Builder: `with_core_chips()` + `with_commitment()` working
- ✅ Witness pipeline confirmed working (no revert needed)
- ✅ `ProveError` replaces panics in prover

---

### Phase 3: Extensibility Framework

> **Goal**: Zero-Modification Principle — apps customize purely in their own crate.
> **Ref**: [master-roadmap.md §7](master-roadmap.md#7-phase-3-extensibility-framework), [extensibility-architecture.md](extensibility-architecture.md)

#### 3.1 ChipExtension Trait

**Motivation**: Standardized extension packaging. Each extension registers its chips, buses,
witness population logic, and optionally custom `ColumnCommitment` schemes as a unit.

| Task | Description |
|------|-------------|
| 3.1.1 | `ChipExtension` trait: `register(&self, registry, plan)`, `populate_witness(&self, store, context)`, `name()`. Includes optional `commitment_schemes() -> Vec<Box<dyn ColumnCommitment>>` for state commitment extensions. |
| 3.1.2 | `CoreExtension` — wraps `core_chips()` + `SsmcCommitment` + `SmtCommitment` as a `ChipExtension`. |
| 3.1.3 | `TabulaMachine::builder().with_extension(ext)` — registers all chips and commitment schemes from an extension. |
| 3.1.4 | `tabula-machine::prelude` — stable re-exports of p3 types apps need (`BabyBear`, `RowMajorMatrix`, `InteractionAirBuilder`, etc.). |

#### 3.2 Precompile Framework

**Decision**: Precompile pattern over OpcodeHandler. Hash and Lookup are the existing proof-of-concept.

| Task | Description |
|------|-------------|
| 3.2.1 | **IR**: Add `Precompile { dst: Slot, precompile_id: PrecompileId, args: Vec<SlotRef> }` to `Instruction` enum. |
| 3.2.2 | **Executor**: `PrecompileHandler` trait — `fn execute(&self, args: &[Value]) -> Result<Value>`. Registry in `BatchEnv`. |
| 3.2.3 | **ExecutionChip**: Add `op_precompile` selector (1 boolean column). Constraint: when active, send `(precompile_id, args, result)` to `PrecompileBus`. |
| 3.2.4 | **Bus**: `define_bus!` for `PrecompileAirBuilder` (C17) — send + receive. |
| 3.2.5 | **WitnessStore**: Precompile events collected during execution, stored for precompile chip trace generation. |
| 3.2.6 | **DSL**: `precompile!()` syntax in tabula-lang for precompile calls. |

**Scope**: One-time ~100 lines added to ExecutionChip. Each subsequent precompile = independent chip, zero core modification.

#### 3.3 Custom Commitment Scheme Example

**Motivation**: Validate that the `ColumnCommitment` trait from Phase 2 actually supports
third-party commitment schemes without core modification.

| Task | Description |
|------|-------------|
| 3.3.1 | Example: `AccumulatorCommitment` — Pedersen/inner-product commitment for columns where order doesn't matter. Demonstrates a completely different data structure proving through the same bus interface. |
| 3.3.2 | Integration test: `TabulaMachine::builder().with_extension(AccumulatorExtension).build()` — proves a batch where one column uses SSMC and another uses Accumulator. |

#### 3.4 Template Chip Framework

| Task | Description |
|------|-------------|
| 3.4.1 | `TemplateChip` trait — `matches(tx_type)`, specialized AIR, equivalence test harness. |
| 3.4.2 | Equivalence test infrastructure — same bus messages as interpreter for identical inputs. |

**Phase 3 completion gate**: Example app crate compiles and proves with custom chip + bus + precompile + custom commitment scheme, without modifying Tabula.

---

### Phase 4: Optimization

> **Goal**: Realize all Tabula-native optimizations.
> **Ref**: [master-roadmap.md §8](master-roadmap.md#8-phase-4-optimization)

#### 4.1 Late-Binding Proof Strategy

| Task | Description |
|------|-------------|
| 4.1.1 | Activate `ShortRun` routing in `route_keys()`. |
| 4.1.2 | `ShortRunChip<W>` — lightweight single-tx access pattern chip. |
| 4.1.3 | LogUp bus integration: ShortRunChip sends on Memory + MergeCompleteness buses. |

#### 4.2 Schema-Driven Width Specialization

| Task | Description |
|------|-------------|
| 4.2.1 | Width-class chip instantiation: `MemorySegment<1>`, `<3>`, `<8>`. |
| 4.2.2 | `ProofPlan` width map: `ColumnPlan.schema_type → W`. |

#### 4.3 NF-Aware Constraint Elision

| Task | Description |
|------|-------------|
| 4.3.1 | `PreprocessedCatalog` — batch-invariant data in preprocessed trace. |
| 4.3.2 | Program-specific preprocessed selectors — replace `slot_written` flags (~15 columns saved). |

#### 4.4 Pipeline Parallelism

| Task | Description |
|------|-------------|
| 4.4.1 | Witness → trace overlap: shard trace building starts as column assembly completes. |
| 4.4.2 | Level 0 independence: Execution/Poseidon/RangeCheck traces built in parallel with shards. |

#### 4.5 D1: Poseidon Chain Delegation

> **Ref**: [master-roadmap.md §8.2 Phase 4d](master-roadmap.md#82-sub-phases), [commitment-architecture-research.md §3.1](commitment-architecture-research.md#31-direction-d1-poseidon-chain-delegation)

| Task | Description |
|------|-------------|
| 4.5.1 | **PoseidonChip chain tracking**: Add `chain_id` (table+col identifier), `step_index` (position in chain), and chaining constraint (`state_out[i] == state_in[i+1]` within same chain) to PoseidonChip. +3 columns (93→96 main). |
| 4.5.2 | **Eliminate StateColumn hash chains**: Remove `old_hash_acc[8]`, `new_hash_acc[8]`, `old_hash_chain[16]`, `new_hash_chain[16]` (48 columns total) from StateColumnChip. The hash chain computation moves entirely to PoseidonChip. |
| 4.5.3 | **StateColumn reduction or elimination**: After hash chain removal, StateColumnChip's remaining role is state entry tracking (identity + key + source + values + segment + key ordering ≈ 53 cols). Evaluate whether this can be folded into InterTxOrderChip or a unified memory chip. |
| 4.5.4 | **LogUp binding update**: Update CommitmentVerification bus to carry chain commitment from PoseidonChip instead of StateColumnChip's hash accumulator. |
| 4.5.5 | **Width reduction verification**: Total global chip width should drop from 261 to ~163 cols (38% reduction). Verify with proof size benchmark. |

**Feasibility**: HIGH — engineering optimization, no new cryptography. PoseidonChip already processes all Poseidon permutations; adding chain continuity tracking is a natural extension.

**Phase 4 completion gate**: Proving time < 50% of Phase 2. D1 chain delegation active. Global chip width reduced from 261 to ~163 cols.

---

### Phase 5: Advanced (Future Research)

> **Goal**: Push beyond current architecture for maximum efficiency.
> **Status**: Design-phase only. No implementation commitment.
> **Ref**: [master-roadmap.md §9](master-roadmap.md#9-phase-5-advanced-future-research), [commitment-architecture-research.md §3](commitment-architecture-research.md#3-research-directions)

#### 5.1 D2+D3: Algebraic Accumulator

> **Ref**: [commitment-architecture-research.md §3.2](commitment-architecture-research.md#32-direction-d2d3-algebraic-accumulator-in-global-chip)

Replace Poseidon hash chain commitment with an order-independent algebraic accumulator. In the layered composition architecture (Phase 2), this is a **new `ColumnCommitment` implementation** — not a core modification.

| Task | Description |
|------|-------------|
| 5.1.1 | **Security proof**: Formal analysis of sum-based multiset hash over EF4 (2^124 birthday bound ~2^62). Evaluate mitigations: double accumulator (Σ H₁, Σ H₂), power-sum symmetric hash, multiplicative accumulator. Must achieve ≥128-bit security. |
| 5.1.2 | **`AccumulatorCommitment` implementation**: New `ColumnCommitment` that embeds ~17 accumulator columns (h_old[4], h_new[4], acc_old[4], acc_new[4], is_state_entry) directly in a unified memory chip. Registers via `with_commitment("accumulator", ...)`. |
| 5.1.3 | **Unified memory chip**: Combine InterTxOrderChip (56 cols) + accumulator columns (17 cols) = 73 cols total (fixed, C-independent). Eliminates StateColumnChip entirely. |
| 5.1.4 | **Non-membership proof alternative**: SSMC uses sorted-list adjacency for non-membership. Sum-based accumulator is orderless — implement alternative (explicit table-size commitment or delegation to SMT). |
| 5.1.5 | **Integration test**: Prove a batch where one column uses SSMC and another uses AccumulatorCommitment, coexisting via the bus protocol. |

**Effect**: Global chip width drops from ~163 cols (after D1) to ~73 cols — 72% total reduction from baseline (261→73).

**Prerequisites**: Phase 4.5 (D1 Poseidon delegation). Security proof for chosen accumulator (estimated 1-2 month research effort).

**Architecture relationship**: `AccumulatorCommitment` registers via `with_commitment("accumulator", AccumulatorCommitment::new())` and can coexist with SSMC/SMT for different columns. The `ColumnCommitment` trait from Phase 2 enables this without core modification.

#### 5.2 D4: Recursive Proof Composition

> **Ref**: [commitment-architecture-research.md §3.3](commitment-architecture-research.md#33-direction-d4-recursive-composition)

Per-column inner STARK proofs + recursive tree aggregation → fixed-size final proof.

| Task | Description |
|------|-------------|
| 5.2.1 | **STARK verifier circuit in AIR**: Implement Plonky3 FRI verifier as an AIR circuit (~10K columns). This is the core engineering effort. |
| 5.2.2 | **Per-column inner proofs**: Shard chips (MemoryShard, StateShard, MetaShard) generate independent STARK proofs per column. |
| 5.2.3 | **Tree reduction**: Binary tree aggregation — each node verifies 2 inner proofs. Layer count = ⌈log₂(C)⌉. |
| 5.2.4 | **Final proof**: Execution + meta + recursive verifications → single compact proof. Optional Groth16 wrapping for O(1) on-chain. |

**Tradeoff** (C=50): ~2-5s global STARK vs ~60s recursive. Crossover at C > ~1000 with R > ~100K rows each.

**Prerequisites**: Phase 1 (machine layer). Estimated 6+ months for recursive verifier circuit.

**Relationship to D2+D3**: Complementary, not competing. D2+D3 optimizes single-proof path. D4 adds recursive compression at scale. Natural progression: D1 → D2+D3 → D4.

#### 5.3 Template Chips

| Task | Description |
|------|-------------|
| 5.3.1 | Pattern recognition for hot-path tx types (e.g., `fill_order`, `transfer`). |
| 5.3.2 | Specialized execution chips (~60 cols vs 278 for generic interpreter). |
| 5.3.3 | Provable equivalence via test harness (same LogUp bus fingerprints). |

**Prerequisites**: Phase 3.4 (TemplateChip trait + equivalence test infrastructure).

#### 5.4 Compiled Per-Program AIR

| Task | Description |
|------|-------------|
| 5.4.1 | Program-specific AIR generation at compile time. |
| 5.4.2 | Entire instruction sequence as a single fixed constraint system. |

**Prerequisites**: Phase 4.3 (compiler-proof co-design infrastructure).

**Phase 5 completion gate**: D2+D3 security proof complete. AccumulatorCommitment passing integration tests. Global width ≤73 cols. (D4 and beyond are separate research milestones.)

---

## Completed Work

### Phase 1: Machine Layer (Done)

| ID | Change | Status |
|----|--------|--------|
| 1.1 | `AnyRap` trait + blanket impl (`machine/src/any_rap.rs`), `ChipRef` wrapper (`chip_ref.rs`) | **Done** |
| 1.2 | `ChipRegistry` + `TabulaMachine` + builder pattern + `core_chips()` | **Done** |
| 1.3 | Deleted `define_chip_set!`, `ChipSet`, `TabulaAir`, `StarkAir`, `chip_set.rs`, `chip_instance.rs` | **Done** |
| 1.4 | `TabulaProvingKey` / `TabulaVerifyingKey` with keygen caching | **Done** |
| 1.5 | Directory reorganization: `prove/`, `verify/`, `permutation/` modules | **Done** |
| 1.6 | RAP Phase Abstraction — **dropped** (YAGNI: single LogUp impl, no second consumer) | **Dropped** |
| 1.7 | Batched 3-round PCS protocol, single FRI opening proof, `TabulaProof` rewrite | **Done** |
| 1.8 | Dependent chip collection — **moved to 2.4** (generalized as `BusConsumer` trait) | **→ 2.4** |

**Verification**: 36 tests passing (17 unit + 2 any_rap + 11 machine + 6 E2E), zero clippy warnings.

### Phase 2: Extensibility Infrastructure + Code Quality (Done)

| ID | Change | Status |
|----|--------|--------|
| 2.0 | Witness pipeline — confirmed working, no revert needed | **Skipped** |
| 2.1.1–2 | `ColumnCommitment` + `BusConsumer` traits (pre-existing in `tabula-stark`) | **Done** |
| 2.1.3 | `MemoryModel` trait + `GlobalSortedMemory` impl (`composition.rs`) | **Done** |
| 2.1.4 | `RootProof` trait + `SmtRootProof` impl (`composition.rs`) | **Done** |
| 2.1.5–6 | `ColumnPlan`/`ProofPlan` dispatch — deferred to Phase 3/4 | **Deferred** |
| 2.2 | `CommitmentScheme` trait + `SsmcScheme`/`SmtScheme` + `with_commitment()` builder API | **Done** |
| 2.3 | `BusConsumer` wired for PoseidonChip + RangeCheckChip + `with_bus_consumer()` API | **Done** |
| 2.4.1 | `ProveError` enum replaces panics in prover | **Done** |
| 2.4.2 | RAP folder field encapsulation (private + getters) | **Done** |
| 2.4.3 | PCS ceremony extraction (`pcs_commit_and_open`, `pcs_verify_and_recompose`) | **Done** |
| 2.4.4 | Directory reorganization (`prove/`, `verify/` modules) | **Done** |

**Key files added/modified**:
- `machine/src/composition.rs` — `MemoryModel`, `RootProof`, `CommitmentScheme` traits + impls
- `machine/src/machine.rs` — `with_commitment()`, `with_bus_consumer()`, `with_buses()`, `with_config()` APIs
- `machine/src/registry.rs` — `default_commitment_chips()` uses `SsmcScheme.airs()`
- `machine/tests/common/mod.rs` — shared test infrastructure (extracted from duplicated pipelines)
- `machine/tests/machine.rs` — 24 tests (up from 11) covering all builder APIs

**Verification**: 49 tests passing (17 unit + 2 any_rap + 24 machine + 6 E2E), zero clippy warnings.

---

### R-series: Machine Layer Refactoring (Done)

| ID | Change | Files | Status |
|----|--------|-------|--------|
| R1 | Renamed `rap.rs` → `ef4.rs`, unified 3 EF4 multiply functions into generic `ef4_mul<T: PrimeCharacteristicRing>` | ef4.rs, rap_folder.rs, permutation.rs, lib.rs | **Done** |
| R2 | Eliminated `TabChallenger` duplication — import `Challenger` from config.rs | permutation.rs | **Done** |
| R3 | Moved `TabulaPcs` type alias to config.rs, removed duplicates | config.rs, prover.rs, verifier.rs | **Done** |
| R4 | Removed stale `#[allow(dead_code)]`, gated test-only code with `#[cfg(test)]` | keys.rs, permutation.rs | **Done** |
| R5 | Removed unused parameters (`_interactions_per_row`, `_inner_count`) | verifier.rs | **Done** |
| R6 | Inlined `prove_chip_rap_inner` into `prove_chip_rap` (12-arg, single-caller) | prover.rs | **Done** |
| R7 | Centralized `rap_constraint_count()` in keys.rs | keys.rs, prover.rs | **Done** |
| Bonus | Fixed clippy: clone-on-copy, collapsible if, needless borrows, borrow conflict in finalize_cumsum | prover.rs, verifier.rs, rap_folder.rs, permutation.rs | **Done** |

**Verification**: 21 tests passing, zero clippy warnings in `tabula-machine`.

---

## Constraint Audit Results

### Compiler-Enforced (NOT in AIR — correct design)

| Rule | What compiler enforces | Why AIR doesn't need it |
|------|----------------------|------------------------|
| NF-1 | Unique read per (t,c,r) per tx | Duplicate reads = identical bus messages, LogUp handles |
| NF-2 | Unique write per (t,c,r) per tx | GlobalSortedMem/Merge catches conflicts |
| NF-3 | No read-after-write to same cell | Memory consistency argument handles ordering |
| NF-4 | Key alias distinctness | Compiler inserts `Cmp(Ne)+Assert` guards → AIR verifies as regular instructions |
| Slot numbering | Contiguous renumbering | Carry + linkage work regardless of slot assignment |
| Type checking | Value type compatibility | AIR operates on field elements directly |
| Instruction order | Intra-tx instruction sequence | SSA semantics: order-independent for same result |

### AIR-Verified (ALL essential — confirmed correct)

| Category | Count | Why essential |
|----------|-------|---------------|
| Boolean constraints | ~50 | Prevent non-binary selector exploitation |
| One-hot opcode | 1 | Prevent multi-opcode or zero-opcode rows |
| SSA slot carry | 16×(W+1) | Prevent slot value fabrication across rows |
| Operand linkage | 48+3 | Prevent fake operand values |
| Arithmetic correctness | 7 opcodes | Verify computation results |
| Range checks | 20+ sends | Prevent BabyBear mod-p exploitation |
| Bus interactions | 6 buses | Cross-chip consistency |
| Clock/ordering | 2 | Timestamp and tx sequence integrity |
| First row init | 1 | Establish clean initial state |

**Conclusion**: Zero redundancy between compiler NF rules and AIR constraints. Complementary threat models (malformed program vs malicious prover).

---

## Dependency Graph

```
Phase 0: Foundation ──────────── COMPLETE (M1-M13, C1 fixed, 21 E2E tests)
  │
  ▼
Phase 1: Machine Layer ──────── COMPLETE (36 tests, single FRI, registry-based)
  │
  ▼
Phase 2: Extensibility + Quality  COMPLETE (49 tests, CommitmentScheme, BusConsumer)
  │
  ├──────────────────────────────────┐
  ▼                                  ▼
Phase 3: Extensibility ← NEXT   Phase 4a-c: Optimization (partial)
  │  3.1 ChipExtension trait         │  4.1 Late binding
  │  3.2 Precompile framework        │  4.2 Width specialization
  │  3.3 Custom commitment example   │  4.3 NF elision
  │  3.4 Template chip framework     │
  │                                  │
  └──────────┬───────────────────────┘
             ▼
         Phase 4d: D1 Poseidon Delegation
             │  Eliminate StateColumn hash chains
             ▼
         Phase 4e: Pipeline Parallelism
             │
             ▼
         Phase 5: Advanced (future)
             D2+D3 (algebraic accumulator),
             D4 (recursive composition),
             templates, compiled AIR, distributed proving
```

### Internal Dependencies

```
Phase 1:  COMPLETE

Phase 2:  COMPLETE

Phase 3:  3.1 ─→ 3.2 ─→ 3.4
          3.3 (independent after 3.1, validates Phase 2 trait design)

Phase 4:  4.1, 4.2 (independent, after Phase 2)
          4.3 (after Phase 3.1)
          4.4d D1 (after Phase 3+4a-c complete)
          4.4e pipeline (after Phase 2)

Phase 5:  D2+D3 (after 4d D1 + security proof)
          D4 (after Phase 1, can be parallel with D2+D3)
```

---

## Decision Records

### Composition Model: Compile-Time Enum vs Runtime Registry

#### Problem

How should Tabula compose multiple chips into a provable system?

#### Options

**Option A: Compile-time enum via `define_chip_set!` macro**

```rust
define_chip_set! {
    pub enum MyAppAir {
        include TabulaCoreAir;   // needs proc macro for include
        Keccak(KeccakChip),
    }
}
prove::<MyAppAir>(&config, &traces, stmt);
```

**Option B: Runtime registry via `ChipRegistry` + `AnyRap`**

```rust
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_chip(KeccakChip::default())
    .build();
machine.prove(&traces, stmt);
```

#### Analysis

| Aspect | Compile-Time (A) | Runtime Registry (B) |
|--------|-------------------|---------------------|
| **Extension composition** | `include` requires proc macro (complex, new crate) | `.with_chip()` — pure Rust, no macros |
| **Multi-extension** | Nested `include` = macro complexity explosion | `.with_extension(x).with_extension(y)` — trivial |
| **Performance** | enum match, potential inlining | vtable call (~1ns per eval, <1% of constraint cost) |
| **Type safety** | Compile-time | Setup-time (`build()` validates all bounds) |
| **Error messages** | Macro errors (hard to debug) | Standard Rust errors |
| **Boilerplate** | Macro generates ~6 trait impls per chip set | Zero boilerplate |
| **Traits needed** | `ChipSet` + `StarkAir` + `define_chip_set!` | `AnyRap` (blanket impl, zero per-chip work) |
| **Code to delete** | — | `ChipSet` trait, `define_chip_set!`, `TabulaAir` enum, `StarkAir` alias |

#### Corrected assumption

The master-roadmap.md stated: "p3's `Air<AB>` requires static dispatch." This is **incorrect**. OpenVM demonstrates `dyn AnyRap<SC>` in production by bundling all required `Air<AB>` bounds into a single object-safe supertrait. Tabula's `StarkAir` alias already enumerates these bounds — converting to `AnyRap` is straightforward.

#### Performance validation

`eval()` is called once per row per chip. Each call evaluates hundreds of field operations (~100ns+). A vtable indirect call adds ~1ns. For a chip with 2^20 rows, total vtable overhead is ~1ms vs ~100ms+ of constraint evaluation. **Negligible.**

OpenVM uses `Arc<dyn AnyRap<SC>>` in production with no measurable overhead.

#### Decision

**Option B: Runtime registry.**

The `include` feature for `define_chip_set!` was the most complex planned work item in Phase 3 (proc macro crate, compile-time variant forwarding, nested enum expansion). Eliminating it in favor of `.with_chip()` saves ~400 lines of macro code and an entire derive crate.

Individual chip structs (`ExecutionChip`, `PoseidonChip`, etc.) keep their `ChipSpec`, `Air<AB>`, and `TraceContributor` impls unchanged. Only the composition layer changes.

#### What stays

- `ChipSpec` trait — individual chip metadata (unchanged)
- `TraceContributor` trait — individual chip trace generation (unchanged)
- `InteractionAirBuilder` + `define_bus!` — typed bus interactions (unchanged)
- `core_chips` / `core_buses` constants — open ID newtypes (unchanged)
- All 9 chip implementations — zero modification

#### What goes

| Removed | Lines | Replacement |
|---------|-------|-------------|
| `define_chip_set!` macro | ~130 | `ChipRegistry::register()` |
| `ChipSet` trait | ~30 | `ChipRegistry` methods |
| `TabulaAir` enum + dispatch | ~30 | `core_chips()` function |
| `StarkAir` trait alias | ~10 | `AnyRap` trait |
| `bridge.rs` EmptyMessageBuilder impls | ~38 | Part of `AnyRap` bounds |
| Planned `include` proc macro | ~400 (avoided) | Not needed |
| **Total removed** | **~240 existing + ~400 avoided** | |

---

### Precompile vs OpcodeHandler

#### Problem

How should Tabula support new computational operations beyond the 12 core opcodes?

#### Options Considered

**Option A: OpcodeHandler trait** (per-opcode modular handlers within ExecutionChip)

Pros: No core modification after initial setup. Dynamic column layout.
Cons: Breaks `#[repr(C)]` zero-copy. vtable in eval hot path. Over-engineering for 12 stable opcodes.

**Option B: Precompile chips** (separate chip per complex operation, bus-connected)

Pros: Already proven (Hash→Poseidon, Lookup→StaticTable). Independent testing. Zero core modification after one-time setup. `#[repr(C)]` preserved.
Cons: Bus overhead per call (~5 EF4 operations). One-time ~100 line addition to ExecutionChip.

**Option C: Per-opcode chips** (SP1 pattern)

Pros: Minimal per-chip columns. Maximum parallelism.
Cons: SSA slot carry requires 112+ shared columns in every chip. Counterproductive for Tabula.

#### Decision

**Option B: Precompile chips.**

1. 12 core opcodes are computationally complete and stable.
2. New operations naturally map to separate chips.
3. Pattern already validated by Hash and Lookup.
4. SSA slot carry makes Option C impractical.

#### SSA Slot Carry Explanation

Each ExecutionChip row contains the full state of 16 SSA slots (48 value columns + 16 null flags + 16 written flags = 80 columns). The carry constraint ensures unwritten slots propagate values row-to-row:

```
if next.slot_written[s] == 0:
    next.slots[s] = local.slots[s]
```

This is essential: it models the SSA register file. Without it, a malicious prover could fabricate slot values. In a per-opcode model, every chip would need these 80 columns plus 48 selector columns = 128 shared columns. Opcode-specific columns are typically 5-30. The shared overhead dominates.

---

## Success Criteria

| Phase | Gate | Measurable |
|-------|------|------------|
| 1 | Single FRI proof, C1 resolved, proof size < 50% of current. `define_chip_set!` deleted. | Proof size benchmark. No `ChipSet`/`TabulaAir` in codebase. |
| 2 | CommitmentScheme + BusConsumer traits. Global chips unchanged. Code quality improved. | ✅ 49 tests passing. No hardcoded dispatch. ProveError replaces panics. |
| 3 | App can add custom chip + bus + precompile + commitment scheme without modifying Tabula | Example app crate compiles and proves |
| 4 | D1 eliminates StateColumn hash chains, proving time < 50% of Phase 2 | Global width 261→163. Optimization benchmark. |
| 5 | D2+D3 security proof. AccumulatorCommitment integration tests passing. | Global width ≤73 cols. Security proof document published. |

---

## Appendix: App Developer Experience (Target)

### A. Custom precompile chip

```rust
use tabula_machine::prelude::*;
use tabula_machine::{TabulaMachine, ChipExtension};

// App defines a precompile chip (in its own crate)
struct KeccakChip;
impl ChipSpec for KeccakChip { /* chip_id, name, ... */ }
impl<AB: InteractionAirBuilder> Air<AB> for KeccakChip { /* constraints */ }
impl TraceContributor for KeccakChip { /* trace generation */ }

// App defines an extension package
struct KeccakExtension;
impl ChipExtension for KeccakExtension {
    fn register(&self, registry: &mut ChipRegistry, _plan: &ProofPlan) {
        registry.register(KeccakChip::default());
    }
    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext) {
        let events = ctx.precompile_events(KECCAK_ID);
        store.put::<Vec<KeccakEvent>>("keccak_events", events);
    }
    fn name(&self) -> &str { "keccak" }
}

fn main() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_extension(KeccakExtension)
        .build()
        .expect("setup failed");

    let proof = machine.prove(&traces, statement);
    machine.verify(&proof).expect("verification failed");
}
```

### B. Custom commitment scheme

```rust
use tabula_machine::prelude::*;
use tabula_witness::{ColumnCommitment, ColumnPlan};

// A DEX-optimized accumulator commitment for order-book columns.
// Internally uses an algebraic accumulator instead of SSMC's sorted list.
struct AccumulatorCommitment;
impl ColumnCommitment for AccumulatorCommitment {
    fn name(&self) -> &str { "accumulator" }

    fn register_shard_chips(&self, col: &ColumnPlan, registry: &mut ChipRegistry)
        -> Vec<ChipId>
    {
        let chip = AccumulatorShardChip::new(col);
        let id = registry.register(chip);
        vec![id]
    }

    fn populate_shard_witness(&self, col: &ColumnPlan, store: &mut WitnessStore) {
        // Read column accesses from store, produce accumulator witness
    }

    fn build_shard_traces(&self, col: &ColumnPlan, store: &WitnessStore)
        -> Vec<(ChipId, TraceEntry)>
    {
        // Build trace from witness — same Memory bus receive, CommitVerif bus send
    }

    fn output_buses(&self) -> Vec<BusId> {
        vec![core_buses::COMMITMENT_VERIF, core_buses::POSEIDON_PERM]
    }
}

fn main() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        // Override commitment scheme for specific columns
        .with_commitment_scheme("accumulator", AccumulatorCommitment)
        .with_proof_plan_override(|plan| {
            // Use accumulator for table 5, column 0 (order book)
            plan.set_scheme(TableId(5), ColId(0), "accumulator");
        })
        .build()
        .expect("setup failed");

    let proof = machine.prove(&traces, statement);
    machine.verify(&proof).expect("verification failed");
}
```

**Key property**: `AccumulatorCommitment` speaks the same bus protocol (Memory bus in,
CommitVerif bus out). The prover, verifier, and all other chips are completely unaware
of the commitment scheme change. Soundness is maintained because the bus balancing
constraint is scheme-agnostic.

---

## Relationship to Existing Documents

| Document | Role | Update needed |
|----------|------|---------------|
| [master-roadmap.md](master-roadmap.md) | Phase structure, risk registry | Update non-goal: remove "p3 requires static dispatch", add registry rationale |
| [extensibility-architecture.md](extensibility-architecture.md) | Detailed API definitions | Update F2 (remove `include`), add `AnyRap`/`ChipRegistry` specs |
| [tabula-machine-architecture.md](tabula-machine-architecture.md) | Target architecture | Update composition model section |
| [tabula-native-optimizations.md](../research/tabula-native-optimizations.md) | 8 optimization specs | No change |
| [proof-optimization-architecture.md](proof-optimization-architecture.md) | KeyRoute, templates, ShortRun | No change |
| This document | Concrete task list with decisions | Primary reference |
