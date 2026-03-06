# Tabula Implementation Workplan

> **Version**: 2.0
> **Date**: 2026-03-06
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

After all phases, the proof system traits simplify to:

```
Individual chip traits (implemented by each chip struct):
  ChipSpec           — chip_id(), chip_name(), has_interactions(), ...
  BaseAir<F>         — width()
  Air<AB>            — eval() for each builder type
  TraceContributor   — phase(), contribute()

Type-erased traits (blanket-implemented, used by registry/machine):
  AnyRap             — bundles ChipSpec + all Air<AB> bounds into one object-safe trait
  DynTraceContributor — object-safe wrapper for TraceContributor

Composition (replaces define_chip_set! + ChipSet + TabulaAir):
  ChipRegistry       — dynamic chip registration, bus manifest, setup validation
  ChipExtension      — extension package (registers chips + buses + witness data)
  TabulaMachine      — owns registry + config, provides prove/verify

Removed:
  ChipSet trait       — replaced by ChipRegistry
  StarkAir trait      — replaced by AnyRap
  define_chip_set!    — replaced by ChipRegistry
  TabulaAir enum      — replaced by core_chips() function
```

---

## Work Items

### Phase 1: Machine Layer

> **Goal**: Soundness, shared PCS, single FRI, registry-based composition.
> **Ref**: [master-roadmap.md §5](master-roadmap.md#5-phase-1-machine-layer)

#### 1.1 AnyRap + DynTraceContributor (foundation for everything else)

**Motivation**: Enable type-erased chip references. Prerequisite for ChipRegistry and TabulaMachine.

| Task | Files | Description |
|------|-------|-------------|
| 1.1.1 | `stark/src/air/any_rap.rs` | `AnyRap` trait: bundles `ChipSpec` + all `Air<AB>` bounds. Blanket impl for any type satisfying the bounds. Methods: `chip_id()`, `chip_name()`, `has_interactions()`, `num_public_values()`, `preprocessed_width()`, `width()`. |
| 1.1.2 | `stark/src/trace/contributor.rs` | `DynTraceContributor` trait: object-safe version of `TraceContributor`. Blanket impl: `impl<T: TraceContributor + Send + Sync> DynTraceContributor for T`. |
| 1.1.3 | `stark/src/air/mod.rs` | Re-export `AnyRap`. |

#### 1.2 ChipRegistry + TabulaMachine

**Motivation**: Replace `define_chip_set!` enum dispatch with runtime registration. Eliminate `ChipSet` trait, `StarkAir` alias, `TabulaAir` enum.

| Task | Files | Description |
|------|-------|-------------|
| 1.2.1 | `machine/src/registry.rs` | `ChipRegistry` struct: `chips: Vec<RegisteredChip>`, `buses: BTreeSet<BusId>`, `public_value_chip: Option<ChipId>`. Methods: `register()`, `register_core()`, `set_public_value_chip()`, `validate()`. |
| 1.2.2 | `machine/src/registry.rs` | `RegisteredChip` struct: `air: Box<dyn AnyRap>`, `contributor: Box<dyn DynTraceContributor>`, cached `interactions: InteractionDescriptor<BabyBear>`, `log_quotient_degree: usize`. Populated at `validate()`. |
| 1.2.3 | `machine/src/machine.rs` | `TabulaMachine` struct: `config`, `registry`, `proving_key`, `verifying_key`. Builder pattern: `TabulaMachine::builder().with_core_chips().with_chip(X).build()`. `build()` calls `registry.validate()` and runs keygen. |
| 1.2.4 | `chips/src/lib.rs` | `core_chips() -> Vec<(Box<dyn AnyRap>, Box<dyn DynTraceContributor>)>` function — returns all 9 core chips. Replaces `TabulaAir` enum. |
| 1.2.5 | `machine/src/lib.rs` | Public API: `TabulaMachine::builder()`, `machine.prove()`, `machine.verify()`. Remove `prove::<CS>()` / `verify::<CS>()` free functions. |

#### 1.3 Remove Superseded Abstractions

| Task | Files | Description |
|------|-------|-------------|
| 1.3.1 | `stark/src/air/chip_set.rs` | Delete `ChipSet` trait and `define_chip_set!` macro entirely. |
| 1.3.2 | `chips/src/lib.rs` | Remove `TabulaAir` enum. Remove `define_chip_set!` invocation. Keep individual chip structs and their `ChipSpec` / `Air<AB>` / `TraceContributor` impls unchanged. |
| 1.3.3 | `machine/src/prover.rs` | Remove `StarkAir` trait alias. Prover works with `&ChipRegistry` internally. |
| 1.3.4 | `stark/src/bridge.rs` | Review: `EmptyMessageBuilder` impls may need updating for `AnyRap` bounds. |
| 1.3.5 | All test files | Migrate from `prove::<TabulaAir>(...)` to `machine.prove(...)`. Replace `TabulaAir::Execution(...)` with direct chip construction. |

#### 1.4 ProvingKey / VerifyingKey

| Task | Files | Description |
|------|-------|-------------|
| 1.4.1 | `machine/src/keys.rs` | `TabulaProvingKey`: per-chip keygen info, interaction descriptors, preprocessed data, log_quotient_degree. Computed once at `TabulaMachine::build()`. |
| 1.4.2 | `machine/src/keys.rs` | `TabulaVerifyingKey`: per-chip verify info (chip_id, main_width, preprocessed commitment, num_public_values, public_value_chip). Serializable. Sufficient for standalone verification without the machine. |
| 1.4.3 | `machine/src/verifier.rs` | `verify_with_key(vk: &TabulaVerifyingKey, proof: &TabulaProof)` — standalone verification without machine instance. |

#### 1.5 Directory Structure Cleanup

| Task | Files | Description |
|------|-------|-------------|
| 1.5.1 | `machine/src/permutation/` | Split `permutation.rs` (545 lines) into `challenges.rs` (~100 lines) + `trace.rs` (~200 lines). |
| 1.5.2 | `machine/src/rap/` | Split `rap_folder.rs` (524 lines) into `prover_folder.rs` (~260 lines) + `verifier_folder.rs` (~260 lines). |
| 1.5.3 | `machine/src/verifier.rs` | Remove hardcoded `core_chips::SMT_TABLE_PATH`. Use `verifying_key.public_value_chip` instead. |

#### 1.6 RAP Phase Abstraction

| Task | Files | Description |
|------|-------|-------------|
| 1.6.1 | `machine/src/rap/mod.rs` | `RapPhase` trait: `generate_perm_trace()`, `eval_rap_constraints()`. Current folders become implementations. |
| 1.6.2 | `machine/src/prover.rs`, `verifier.rs` | Prover/verifier use `RapPhase` trait instead of directly constructing folders. |

#### 1.7 Shared PCS + Single FRI

| Task | Files | Description |
|------|-------|-------------|
| 1.7.1 | `machine/src/prover.rs` | Batch all chip traces into single PCS commitment. Two rounds: Round 1 (main), Round 2 (perm after challenge). |
| 1.7.2 | `machine/src/verifier.rs` | Verify single batched PCS opening. |
| 1.7.3 | `machine/src/proof.rs` | `TabulaProof` with single `main_commitment`, `perm_commitment`, `quotient_commitment`, `opening_proof`. |
| 1.7.4 | `machine/src/config.rs` | Remove `p3-uni-stark` dependency. Use `p3-commit`, `p3-fri`, `p3-dft` directly. |

#### 1.8 Dependent Chip Collection Generalization

| Task | Files | Description |
|------|-------|-------------|
| 1.8.1 | `stark/src/trace/contributor.rs` | `DependentChip` trait: `required_buses() -> Vec<BusId>`, `collect_and_store(interactions, store)`. |
| 1.8.2 | `witness/src/trace/orchestration.rs` | Replace hardcoded `collect_dependent_inputs` with generic collection driven by `DependentChip::required_buses()`. |

**Phase 1 completion gate**: All existing tests pass on new registry-based infrastructure. Proof size < 50% of current. `define_chip_set!`, `ChipSet`, `TabulaAir`, `StarkAir` all deleted.

**Phase 1 internal dependencies:**
```
1.1 (AnyRap) ─→ 1.2 (Registry) ─→ 1.3 (Remove old) ─→ 1.7 (Shared PCS)
                 1.2 ─→ 1.4 (ProvingKey)
1.5 (Directory), 1.6 (RAP), 1.8 (Dependent) — independent, can parallelize
```

---

### Phase 2: Shard Architecture

> **Goal**: Per-column proof decomposition. Untouched columns = zero cost.
> **Ref**: [master-roadmap.md §6](master-roadmap.md#6-phase-2-shard-architecture)

#### 2.1 Column Strategy + ProofPlan

| Task | Description |
|------|-------------|
| 2.1.1 | `ColumnStrategy` enum: `SortedMemory`, `ShortRun`, `ReadOnly`, per-column VC choice. |
| 2.1.2 | `ProofPlan` struct: `Vec<ColumnPlan>` mapping each (t,c) to strategy + width + chip variant. |
| 2.1.3 | `ProvingKey` generation from `ProofPlan` — shard variant definitions, preprocessed data. |

#### 2.2 Per-Column Chip Migration

| Task | Description |
|------|-------------|
| 2.2.1 | `MemorySegment<W>` — per-column sorted memory (replaces global `InterTxOrderChip`). |
| 2.2.2 | `StateSegment<W>` — per-column SSMC+Merge (replaces global `StateColumnChip`). |
| 2.2.3 | `MetaSegment` — per-column ColumnMeta. |
| 2.2.4 | `SmtColPathSegment` — per-column Merkle path. |

#### 2.3 Gadget Simplification

| Task | Description |
|------|-------------|
| 2.3.1 | Delete `gadgets/lex.rs` (~170 lines) — no cross-column ordering needed. |
| 2.3.2 | Delete `gadgets/segment.rs` (~131 lines) — no column boundary detection needed. |
| 2.3.3 | Simplify `gadgets/key_rc.rs` — `(r,τ)` only, no `(t,c)` comparison. |

#### 2.4 Parallel Trace Building

| Task | Description |
|------|-------------|
| 2.4.1 | `rayon::par_iter()` over columns for independent shard trace generation. |
| 2.4.2 | 4-stage witness pipeline: Collector → RowBuilder → ColumnAssembler → Orchestrator. |

**Phase 2 completion gate**: All tests rewritten for shard architecture. Untouched columns produce zero trace rows.

---

### Phase 3: Extensibility Framework

> **Goal**: Zero-Modification Principle — apps customize purely in their own crate.
> **Ref**: [master-roadmap.md §7](master-roadmap.md#7-phase-3-extensibility-framework), [extensibility-architecture.md](extensibility-architecture.md)

#### 3.1 ChipExtension Trait

**Motivation**: Standardized extension packaging. Each extension registers its chips, buses, and witness population logic as a unit.

| Task | Description |
|------|-------------|
| 3.1.1 | `ChipExtension` trait: `register_chips(&self, registry)`, `populate_witness(&self, store, context)`, `name()`. |
| 3.1.2 | `CoreChipExtension` — wraps `core_chips()` as a `ChipExtension`. |
| 3.1.3 | `TabulaMachine::builder().with_extension(ext)` — registers all chips from an extension. |
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

#### 3.3 State Commitment Extension

| Task | Description |
|------|-------------|
| 3.3.1 | `VectorCommitment` trait — abstract SSMC/SMT operations. Existing `HybridVC` implements it. |
| 3.3.2 | `PropertyOpening` trait — abstract proof-of-inclusion/update. |

#### 3.4 Template Chip Framework

| Task | Description |
|------|-------------|
| 3.4.1 | `TemplateChip` trait — `matches(tx_type)`, specialized AIR, equivalence test harness. |
| 3.4.2 | Equivalence test infrastructure — same bus messages as interpreter for identical inputs. |

**Phase 3 completion gate**: Example app crate compiles and proves with custom chip + bus + precompile, without modifying Tabula.

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

**Phase 4 completion gate**: Proving time < 50% of Phase 2. All 8 optimizations active.

---

## Completed Work

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
Phase 1: Machine Layer
  │  1.1 AnyRap + DynTraceContributor
  │  1.2 ChipRegistry + TabulaMachine
  │  1.3 Remove define_chip_set!/ChipSet/TabulaAir/StarkAir
  │  1.4 ProvingKey / VerifyingKey
  │  1.5 Directory structure cleanup
  │  1.6 RAP phase abstraction
  │  1.7 Shared PCS + single FRI
  │  1.8 Dependent chip collection generalization
  │
  ▼
Phase 2: Shard Architecture
  │  2.1 ColumnStrategy + ProofPlan
  │  2.2 Per-column chip migration
  │  2.3 Gadget simplification
  │  2.4 Parallel trace building
  │
  ├──────────────────────────────────┐
  ▼                                  ▼
Phase 3: Extensibility           Phase 4a-b: Optimization (partial)
  │  3.1 ChipExtension trait         │  4.1 Late binding
  │  3.2 Precompile framework        │  4.2 Width specialization
  │  3.3 State commitment ext.       │
  │  3.4 Template chip framework     │
  │                                  │
  └──────────────┬───────────────────┘
                 ▼
             Phase 4c-d: Optimization (remaining)
                 │  4.3 NF elision
                 │  4.4 Pipeline parallelism
                 │
                 ▼
             Phase 5: Advanced (future)
                 Template chips, recursive aggregation,
                 compiled per-program AIR, distributed proving
```

### Internal Dependencies

```
Phase 1:  1.1 (AnyRap) ─→ 1.2 (Registry) ─→ 1.3 (Remove old) ─→ 1.7 (Shared PCS)
                            1.2 ─→ 1.4 (Keys)
          1.5 (Directory), 1.6 (RAP), 1.8 (Dependent) — independent

Phase 2:  2.1 ─→ 2.2 ─→ 2.4
          2.3 (independent)

Phase 3:  3.1 ─→ 3.2 ─→ 3.4
          3.3 (independent after 3.1)

Phase 4:  4.1, 4.2 (independent, after Phase 2)
          4.3 (after Phase 3.1)
          4.4 (after Phase 2.4)
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
| 2 | Per-column parallel proving, untouched columns = zero cost | Parallelism benchmark |
| 3 | App can add custom chip + bus + precompile without modifying Tabula | Example app crate compiles and proves |
| 4 | All 8 optimizations active, proving time < 50% of Phase 2 | Optimization benchmark |

---

## Appendix: App Developer Experience (Target)

```rust
// ── my-app/src/main.rs ──────────────────────────────────────────

use tabula_machine::prelude::*;
use tabula_machine::{TabulaMachine, ChipExtension};

// App defines a precompile chip (in its own crate)
struct KeccakChip;
impl ChipSpec for KeccakChip { /* chip_id, name, ... */ }
impl<AB: InteractionAirBuilder> Air<AB> for KeccakChip { /* constraints */ }
impl TraceContributor for KeccakChip { /* trace generation */ }

// App defines an extension package
struct MyExtension;
impl ChipExtension for MyExtension {
    fn register_chips(&self, registry: &mut ChipRegistry) {
        registry.register(KeccakChip::default());
    }
    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext) {
        let events = ctx.precompile_events(KECCAK_ID);
        store.put::<Vec<KeccakEvent>>("keccak_events", events);
    }
    fn name(&self) -> &str { "keccak" }
}

fn main() {
    // Compose: core + app extension (zero Tabula modification)
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_extension(MyExtension)
        .with_config(production_config())
        .build()
        .expect("setup failed");

    let proof = machine.prove(&traces, statement);
    machine.verify(&proof).expect("verification failed");
}
```

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
