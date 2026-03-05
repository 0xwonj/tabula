# Tabula Master Roadmap

> **Version 1.0** — Complete path from current state to production-ready proving framework.
> Date: 2026-03-05
> Depends on: [tabula-machine-architecture.md](tabula-machine-architecture.md) (v0.5)

---

## Current State

| Metric | Value |
|--------|-------|
| Crates | 17 (`core`, `contract`, `ir`, `executor`, `commitment`, `stark`, `gadgets`, `chips`, `witness`, `machine`, `lang`, `artifact`, `driver`, `cli`, `daemon`, `web`) |
| Source LOC | ~34,500 |
| Tests | ~860+ functions across workspace |
| Milestones complete | M1–M13 (Phase 0 complete) |
| Active milestone | Phase 1 (machine layer) |
| Chips | 9 (Execution, InterTxOrder, StateColumn, ColumnMeta, Poseidon, RangeCheck, StaticTable, SmtColPath, SmtTablePath) |
| LogUp buses | 11 (C5, C6, C8–C16) — all with positive/negative tests |
| E2E STARK tests | 6 passing (DSL → compile → execute → witness → trace → prove → verify) |
| Known soundness gap | C1: LogUp cumsums not PCS-committed (Phase 1 fix) |
| Architecture target | Per-column sharding, shared PCS, single FRI, extensibility framework — none implemented |

---

## §1 Principles

Six invariant properties of the Tabula protocol constrain the architecture
(see [tabula-machine-architecture.md §2](tabula-machine-architecture.md#2-foundational-invariants)):

| ID | Invariant |
|----|-----------|
| I1 | Hierarchical state: `{Table → Column → Row → Value}` |
| I2 | Static column addressing: `(t,c)` compile-time constants |
| I3 | SSA memory model (NF-1~4) |
| I4 | Schema typing: per-column type, compile-time width |
| I5 | Trusted compiler: `Program::register()` validates NF |
| I6 | LogUp soundness: bus balance as sole cross-chip mechanism |

Five design principles guide all decisions:

1. **Derive architecture from invariants** — the architecture is discovered, not designed
2. **Own the critical path** — field/PCS from p3, orchestration/LogUp/sharding owned
3. **Bus as universal interface** — LogUp is the sole composition mechanism
4. **Zero-Modification Principle** — apps never fork Tabula's codebase
5. **Every optimization is a consequence** — if it follows from I1-I6, it fits; if not, it doesn't belong

---

## §2 Non-Goals

These are deliberately out of scope for this roadmap:

| Non-Goal | Rationale |
|----------|-----------|
| Runtime-pluggable chips | p3's `Air<AB>` requires static dispatch. Composition is compile-time via `define_chip_set!`. |
| Base field change | BabyBear is fixed. The entire ecosystem (p3, chips, encoding) depends on it. |
| General-purpose computation | Tabula is a state machine framework, not a zkVM. No arbitrary binary execution. |
| Recursive proofs as prerequisite | Recursion is a Phase 5 extension. The architecture works without it. |
| GPU/hardware acceleration | Implementation detail of trace builders. Not an architectural concern. |
| L1 bridge / data availability | Separate crates (`tabula-da`, `tabula-bridge`), separate roadmap. |
| Production operations | Monitoring, alerting, deployment — infrastructure, not proof architecture. |
| Formal verification | Desirable but separate effort. Correctness comes from testing + specification. |

---

## §3 Success Criteria

Each phase has a strict completion gate:

| Phase | Gate | Measurable |
|-------|------|------------|
| 0 | End-to-end proof works for any valid DSL program | `cargo test --features stark` all green, 5+ E2E tests |
| 1 | Single FRI proof, C1 resolved, proof size < 50% of Phase 0 | Proof size benchmark, cumsum PCS-committed |
| 2 | Per-column parallel proving, untouched columns = zero cost | Parallelism benchmark, `shard/` module complete |
| 3 | App can add custom chip + bus + opcode without modifying Tabula | Example app crate compiles and proves |
| 4 | All 8 optimizations active, proving time < 50% of Phase 2 | Optimization benchmark suite |

---

## §4 Phase 0: Foundation Completion

> **Goal**: Working end-to-end proof system for any valid Tabula program.
> **Milestone IDs**: M12 (trace assembly) + M13 (STARK prover/verifier) completion.
> **Principle**: Correctness first. No optimization. No shortcuts.

### 4.1 Core Value

**Soundness and correctness.** Every constraint is checked. Every bus balances. Every proof verifies. This is the foundation everything else builds on. If Phase 0 is wrong, nothing above it matters.

### 4.2 Scope

**M12: Trace Assembly (complete remaining gates)**

| Gate | Description | Status |
|------|-------------|--------|
| M12-G1 | Single trace orchestrator covering all 9 chips | **Complete** — Poseidon/RangeCheck auto-assembly via collectors |
| M12-G2 | ContractMetadataEnvelope fail-closed validation | **Complete** — all 6 error variants + tests |
| M12-G3 | E-trace identity anchor (`tx_index` + `effect_ordinal`) | **Complete** — wired into ExecutionCols |
| M12-G4 | All 11 buses with positive/negative tests | **Complete** — C12 EmptyColRead added |
| M12-G5 | E2E: DSL → execute → witness → all-chip trace → constraint check | **Complete** — 6 diverse tests |

**M13: STARK Prover/Verifier (hardening)**

| Task | Description | Status |
|------|-------------|--------|
| Permutation trace PCS | Prepare for C1 fix (current cumsums are non-committed) | Phase 1 — architecture ready |
| Challenge derivation | Improve Fiat-Shamir to observe PCS commitments (not just heights) | Phase 1 |
| Multi-program support | ProvingKey/VerifyingKey for arbitrary registered programs | Phase 1 |
| E2E test expansion | 5+ diverse programs (multi-tx, mul, select, cmp, arith, read/write) | **Complete** — 6 tests |

### 4.3 Non-Goals for Phase 0

- Proof size optimization (per-chip FRI is acceptable)
- Performance tuning (correctness over speed)
- Extensibility (closed chip set is fine)
- Per-column parallelism (global traces are fine)

### 4.4 Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Poseidon/RC auto-assembly miscount | Bus imbalance → proof failure | Compare auto-collected vs manually-computed counts in tests |
| `p3-uni-stark` API limitations | Cannot commit perm trace separately | Phase 1 replaces p3-uni-stark |
| Multi-tx ordering bugs | Wrong `(r,τ)` ordering in InterTxOrder | Exhaustive multi-tx E2E tests |

### 4.5 Metrics

| Metric | Target |
|--------|--------|
| Tests passing | 400+ (from current 391 in proof) |
| E2E STARK tests | 5+ diverse programs |
| Proof verification | All E2E proofs verify correctly |

---

## §5 Phase 1: Machine Layer

> **Goal**: Replace `stark/` with `machine/` — shared PCS, two-round protocol, single FRI, C1 resolved.
> **Principle**: Own the critical path. Soundness is non-negotiable.

### 5.1 Core Value

**Soundness and proof efficiency.** The C1 gap (non-committed cumsums) is a real vulnerability. Phase 1 eliminates it by PCS-committing all cumsums. As a bonus, shared PCS produces a single FRI proof instead of 9, dramatically reducing proof size.

### 5.2 Scope

| Task | Description | LOC Estimate |
|------|-------------|--------------|
| `machine/config.rs` | STARK config (same BabyBear + Poseidon2 + FRI) | ~100 |
| `machine/prover.rs` | Two-round shared-PCS prover (Round 1: main, Round 2: perm) | ~300 |
| `machine/verifier.rs` | Multi-trace verifier with shared opening | ~200 |
| `machine/permutation.rs` | EF4 fingerprint + cumsum (migrated from `stark/permutation.rs`) | ~200 |
| `machine/challenges.rs` | Fiat-Shamir bound to PCS commitments | ~100 |
| `machine/proof.rs` | New `TabulaProof` with shared commitments | ~100 |
| `machine/rap.rs` | `DiscardInteractionBuilder` (replaces `EmptyMessageBuilder`) | ~50 |
| `machine/symbolic.rs` | `SymbolicInteractionBuilder` (replaces `extractor.rs`) | ~150 |
| Remove `stark/` | Delete prover, verifier, permutation, bridge, config, proof | -1600 |
| Remove `air/extractor.rs` | Replaced by symbolic interaction capture | -360 |
| Adapt `debug/` | Use new interaction collection | ~200 |
| **Net** | | **~-600 LOC** |

### 5.3 Key Technical Decisions

- **p3-uni-stark removed.** We use p3 PCS primitives directly (`p3-commit`, `p3-fri`, `p3-dft`). This is required for shared PCS across chips.
- **DiscardInteractionBuilder** is a concrete wrapper (not blanket impl). Explicit > implicit.
- **SymbolicInteractionBuilder** captures interactions at keygen time. One-pass evaluation. Stored in ProvingKey.
- **ProvingKey / VerifyingKey** introduced as first-class types.

### 5.4 Non-Goals for Phase 1

- Per-column sharding (Phase 2)
- ProofPlan / ColumnStrategy (Phase 2)
- Extensibility framework (Phase 3)
- Any chip changes — existing 9 chips are unchanged

### 5.5 Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| p3 PCS API complexity | Incorrect batched commitment | Port openvm patterns (proven in production) |
| Extension field commitment | EF4 trace commitment requires `ExtensionMmcs` wrapping | Reference SP1's implementation |
| FRI parameter tuning | Proof too large or too slow | Start with conservative params, benchmark later |

### 5.6 Completion Gate

- All existing tests pass on new `machine/` infrastructure
- C1 resolved: cumsums are PCS-committed and verified
- Proof size < 50% of Phase 0 (single FRI vs 9 FRIs)
- `stark/` directory completely removed

---

## §6 Phase 2: Shard Architecture

> **Goal**: Per-column proof decomposition. Each `(t,c)` becomes an independent shard.
> **Principle**: Derive decomposition from I1 (hierarchical state) + I2 (static addressing).

### 6.1 Core Value

**Parallelism and modularity.** Per-column sharding enables embarrassingly parallel proving, fault isolation, and incremental state proofs (untouched columns = zero cost). This is the structural foundation for all subsequent optimizations.

### 6.2 Scope

| Task | Description |
|------|-------------|
| `shard/mod.rs` | `ColumnStrategy`, `VcStrategy`, `Shard` types |
| `shard/builder.rs` | `build_shard_traces()` dispatch by strategy |
| `shard/memory.rs` | `MemorySegment<W>` — per-column sorted memory |
| `shard/state.rs` | `StateSegment<W>` — per-column SSMC |
| `shard/meta.rs` | `MetaSegment` — per-column ColumnMeta |
| `shard/short_run.rs` | `ShortRunSegment<W>` — lightweight single-tx |
| `shard/path.rs` | `SmtColPathSegment` — per-column Merkle path |
| Migrate `InterTxOrderChip` | Split global trace → per-column segments |
| Migrate `StateColumnChip` | Split global trace → per-column segments |
| Eliminate `gadgets/lex.rs` | No cross-column ordering needed (~170 LOC) |
| Eliminate `gadgets/segment.rs` | No column boundary detection (~131 LOC) |
| Simplify `gadgets/key_rc.rs` | `(r,τ)` only, no `(t,c)` comparison |
| `machine/plan.rs` | `ProofPlan`, `ColumnPlan`, `TraceBounds`, `PreprocessedCatalog` |
| `machine/keygen.rs` | `ProofPlan → ProvingKey / VerifyingKey` |
| Per-column parallel trace build | `rayon::par_iter()` over columns |
| 4-stage witness pipeline | Collector → RowBuilder → ColumnAssembler → Orchestrator |

### 6.3 Key Technical Decisions

- **Unified ColumnStrategy**: proof structure + VC scheme in one decision per column.
- **ProofPlan as first-class type**: bridges compile-time guarantees to prove-time.
- **Width parameterization**: `MemorySegment<W>` with W determined by schema type.
- **ProvingKey contains all variant definitions**: prover selects active subset per batch.
- **Zero-height traces** for unused shard variants (not excluded from chip set).

### 6.4 Non-Goals for Phase 2

- Extensibility (chip set is still closed)
- ShortRun activation (defined but unused until Phase 4)
- Template chips (Phase 4)
- Width specialization (defined but W=3 only until Phase 4)

### 6.5 Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Global → per-column migration | Massive refactor of 2 largest chips | Incremental: extract segments one at a time |
| Dynamic shard count vs static ProvingKey | Prover instantiation complexity | Zero-height traces for inactive shards |
| Bus fingerprint changes | All tests break | Migrate one bus at a time, verify balance |

### 6.6 Completion Gate

- All existing tests pass (rewritten for shard architecture)
- `InterTxOrderChip` and `StateColumnChip` replaced by per-column segments
- `gadgets/lex.rs` and `gadgets/segment.rs` deleted
- Untouched columns produce zero trace rows
- Per-column parallel trace building benchmarked

---

## §7 Phase 3: Extensibility Framework

> **Goal**: Zero-Modification Principle — apps customize purely in their own crate.
> **Principle**: Bus as universal interface (I6). Graduated complexity.

### 7.1 Core Value

**Composability and ecosystem.** Tabula becomes a framework, not a monolith. 80% of apps need only DSL. 15% use standard library. 5% build custom chips. All without forking Tabula.

### 7.2 Sub-Phases

**Phase 3a: Foundation (~500 LOC)**

| Prerequisite | Change | Scope |
|-------------|--------|-------|
| F1 | `BusId(u16)` newtype replacing `InteractionKind` enum | ~50 LOC |
| F2 | `define_chip_set!` `include` support | ~100 LOC macro |
| F3 | `TraceContributor` trait | ~200 LOC |
| F4 | `WitnessStore` typed key-value store | ~100 LOC |
| F12 | `tabula-machine::prelude` (stable p3 re-exports) | ~50 LOC |

Dependency chain: `F1 → F2 → F3+F4`, `F12` independent.

**Phase 3b: Instructions (~500 LOC)**

| Prerequisite | Change | Scope |
|-------------|--------|-------|
| F7 | `OpcodeSpec` trait (execute + constrain + witness) | ~150 LOC |
| F8 | `define_instruction_set!` macro | ~300 LOC |
| F9 | `Precompile` IR variant | ~50 LOC |
| F10 | `PrecompileHandler` trait (executor-side) | ~50 LOC |

Dependency chain: `F7 → F8 → F9+F10`.

**Phase 3c: State (~200 LOC)**

| Prerequisite | Change | Scope |
|-------------|--------|-------|
| F5 | `VectorCommitment` trait | ~100 LOC |
| F6 | `PropertyOpening` trait | ~100 LOC |

Extract existing SSMC/SMT into trait implementations.

**Phase 3d: Execution (~200 LOC)**

| Prerequisite | Change | Scope |
|-------------|--------|-------|
| F11 | `TemplateChip` trait + equivalence test harness | ~200 LOC |

**Total: ~1,450 LOC of framework changes.**

### 7.3 API Stability Tiers

| Tier | Guarantee | Examples |
|------|-----------|---------|
| **S (Stable)** | Breaking only on major versions | `Value`, `CellKey`, `Transaction`, `Batch`, `Program` |
| **A (Extension)** | May evolve on minor (additive) | `ChipSpec`, `VectorCommitment`, `OpcodeSpec`, macros |
| **I (Internal)** | No guarantee | Chip internals, column layouts, gadgets |

### 7.4 Validation

- **Example app crate**: Build a minimal app that adds one custom chip, one custom bus, one custom opcode — verify it compiles and proves without modifying Tabula.
- **Lighter DEX skeleton**: Verify all 7 blocked requirements (from review) are unblocked by F1-F12.

### 7.5 Non-Goals for Phase 3

- Actual app implementations (validation only)
- Standard library opcodes (BitwiseOp, WideMul — separate work)
- Standard precompiles (ECDSA — separate work)
- Performance optimization of the framework itself

### 7.6 Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| `define_chip_set! include` macro complexity | Compile errors, wrong variant forwarding | Comprehensive macro tests, edge cases |
| `OpcodeSpec` trait too broad/narrow | Apps can't express their opcodes | Design with 3+ example opcodes before finalizing |
| `BusId` migration breaks all tests | Massive churn | Automated sed + test-by-test migration |

### 7.7 Completion Gate

- F1-F12 all implemented
- Example app crate compiles and proves successfully
- All existing Tabula tests pass
- `extensibility-architecture.md` trait signatures match implementation

---

## §8 Phase 4: Optimization

> **Goal**: Realize all 8 Tabula-native optimizations.
> **Principle**: Every optimization is a consequence of I1-I6.

### 8.1 Core Value

**Proving efficiency.** Phase 0-3 prioritize correctness and composability. Phase 4 makes it fast. The architecture was designed so that every optimization slots in naturally — this phase proves that claim.

### 8.2 Sub-Phases

**Phase 4a: Late-Binding Proof Strategy**

| Task | Description |
|------|-------------|
| Activate ShortRun routing | `route_keys()` produces `ShortRun` for eligible keys |
| `ShortRunChip<W>` | Lightweight chip for single-tx access patterns |
| Strategy selection logic | `select_strategy()` in witness pipeline |
| LogUp bus integration | ShortRunChip sends on Memory + MergeCompleteness buses |

Derives from: I6 (bus balance ensures any valid strategy is sound).

**Phase 4b: Schema-Driven Width Specialization**

| Task | Description |
|------|-------------|
| Width-class chip instantiation | `MemorySegment<1>`, `MemorySegment<3>`, `MemorySegment<8>` |
| ProofPlan width map | `ColumnPlan.schema_type → W` |
| Keygen deduplication | Same-width variants share definitions |

Derives from: I4 (schema typing).

**Phase 4c: NF-Aware Constraint Elision**

| Task | Description |
|------|-------------|
| `PreprocessedCatalog` | Batch-invariant data: range check, Poseidon RC, NF selectors |
| `ConstraintElision` enum | NF-1~4 elision variants |
| Program-specific preprocessed selectors | Replace `slot_written` flags (~15 columns saved) |
| Keygen specialization | ProofPlan.elisions → specialized ExecutionChip constraints |

Derives from: I3 (SSA/NF) + I5 (trusted compiler).

**Phase 4d: Pipeline Parallelism**

| Task | Description |
|------|-------------|
| Witness Stage 3 → trace overlap | ColumnAssembler completion triggers shard trace building |
| Async shard processing | Per-column pipeline without barrier synchronization |
| Level 0 independence | Execution/Poseidon/RangeCheck traces built in parallel with shards |

Derives from: I1 (hierarchical state) + I2 (static addressing).

### 8.3 Non-Goals for Phase 4

- Template chips (Phase 5 — requires pattern recognition infrastructure)
- Recursive aggregation (Phase 5)
- Compiled per-program AIR (Phase 5)

### 8.4 Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| ShortRun constraint correctness | Soundness bug | Equivalence test: ShortRun produces same bus messages as Full |
| Width specialization W=1 edge cases | Bool columns with unexpected encoding | Comprehensive W=1 tests |
| NF elision over-aggressive | Elide a constraint that was actually needed | Conservative: elide only with proof of NF guarantee |
| Pipeline parallelism races | Non-deterministic test failures | Deterministic parallel framework (rayon), no shared mutable state |

### 8.5 Completion Gate

- All 8 optimizations from [tabula-native-optimizations.md](../research/tabula-native-optimizations.md) active
- Proving time benchmark < 50% of Phase 2
- Untouched columns = zero proving cost verified
- Per-column parallelism demonstrated

---

## §9 Phase 5: Advanced (Future Research)

> **Goal**: Push the boundaries of what's possible with Tabula's architecture.
> **Status**: Design-phase only. No implementation commitment.

### 9.1 Template Chips

Specialized execution chips for hot-path transaction patterns (e.g., `fill_order`, `transfer`). ~60 columns vs 278 for generic interpreter. Same LogUp bus fingerprints — provable equivalence via test harness.

**Prerequisite**: Phase 3d (TemplateChip trait) + pattern recognition infrastructure.

### 9.2 Compiled Per-Program AIR

Generate a program-specific AIR at compile time. The entire instruction sequence becomes a single fixed constraint system. Maximum efficiency, minimum generality.

**Prerequisite**: Phase 4c (compiler-proof co-design infrastructure).

### 9.3 Recursive Proof Aggregation

Layer STARK proofs: Block → Segment → Batch. Each layer compresses N proofs into 1 by re-proving verification. Enables constant-size proofs regardless of batch size.

**Prerequisite**: Phase 1 (machine layer must support standalone per-shard proofs).

### 9.4 Distributed Proving

Per-shard independence (from Phase 2) enables distributing shards across machines. Each machine proves its assigned columns independently. Aggregation at the end.

**Prerequisite**: Phase 2 (shard architecture) + network protocol design.

### 9.5 Cross-Batch State Caching

Persist per-column commitments between batches. Only re-prove columns that changed. Amortize fixed costs across batch sequences.

**Prerequisite**: Phase 2 (shard commitment persistence infrastructure).

### 9.6 Conditional Branching

Basic block CFG in IR, `if/else` in DSL, AIR constraints for block transitions. Enables more expressive programs without sacrificing NF properties.

**Prerequisite**: IR extension (CB1-3 design in `docs/research/conditional-branching.md`).

---

## §10 Dependency Graph

```
Phase 0: Foundation Completion
  │  M12 (trace assembly) + M13 (STARK prover/verifier)
  │  ← current work
  │
  ▼
Phase 1: Machine Layer
  │  shared PCS, two-round protocol, C1 fix
  │  ← replaces stark/ with machine/
  │
  ▼
Phase 2: Shard Architecture
  │  per-column decomposition, ProofPlan, ColumnStrategy
  │  ← structural refactor of chips + trace pipeline
  │
  ├─────────────────────────────────┐
  ▼                                 ▼
Phase 3: Extensibility          Phase 4a-b: Optimization (partial)
  │  F1-F12, zero-modification      Late binding, width specialization
  │  ← can start after Phase 2      ← can start after Phase 2
  │
  ▼
Phase 4c-d: Optimization (remaining)
  │  NF elision, pipeline parallelism
  │  ← requires Phase 3 (PreprocessedCatalog)
  │
  ▼
Phase 5: Advanced
  │  templates, recursion, distributed, compiled AIR
  │  ← future research
```

**Critical path**: Phase 0 → Phase 1 → Phase 2 → Phase 3/4

**Parallelizable**: Phase 3 and Phase 4a-b can proceed concurrently after Phase 2.

### 10.1 Internal Dependencies

```
Phase 3 internal:     F1 → F2 → F3+F4 → F5
                      F7 → F8 → F9+F10
                      F12 (independent)
                      F11 (depends on F2)

Phase 4 internal:     4a (late binding) — independent
                      4b (width) — independent
                      4c (NF elision) — needs Phase 3a (PreprocessedCatalog)
                      4d (pipeline) — needs Phase 2 completion
```

---

## §11 Risk Registry

### 11.1 Technical Risks

| ID | Risk | Phase | Likelihood | Impact | Mitigation |
|----|------|-------|-----------|--------|------------|
| R1 | C1 soundness gap exploitable before Phase 1 | 0-1 | Low | Critical | Document as known limitation; Phase 1 is highest priority after Phase 0 |
| R2 | p3 0.4 → 0.5 breaking changes | 1 | Medium | High | Pin exact p3 versions; test against p3 CI |
| R3 | Per-column migration breaks all tests | 2 | High | High | Incremental migration (one chip at a time); feature flag |
| R4 | `define_chip_set! include` macro too complex | 3 | Medium | Medium | Prototype first, test exhaustively, consider proc-macro fallback |
| R5 | Extractor affine assumption violated | 1 | Low | Medium | SymbolicInteractionBuilder eliminates this class of bugs |
| R6 | ShortRun soundness | 4 | Medium | Critical | Mandatory equivalence test: ShortRun ≡ Full for same access pattern |
| R7 | Pipeline parallelism non-determinism | 4 | Low | Medium | Deterministic parallel framework, no shared mutable state |

### 11.2 Strategic Risks

| ID | Risk | Mitigation |
|----|------|------------|
| S1 | Scope creep — adding features before foundation is solid | Phase gates are strict. No Phase N+1 work until Phase N gate passes. |
| S2 | Over-engineering extensibility before real apps exist | Phase 3 validates with example app only. Real app feedback drives Phase 3 revisions. |
| S3 | Performance optimization before correctness | Phase 4 is explicitly after Phase 0-2. Benchmarks only after soundness is proven. |
| S4 | Design document divergence from code | Each Phase completion includes doc review and update. |

---

## §12 Metrics

### 12.1 Per-Phase Quantitative Targets

| Phase | LOC Change | Test Count | Proof Size | Proving Time |
|-------|-----------|------------|------------|--------------|
| 0 | +500 (M12/M13 completion) | 450+ | Baseline (per-chip FRI) | Baseline |
| 1 | -600 (net: remove stark/, add machine/) | 480+ | < 50% of Phase 0 | ~same |
| 2 | +1500 (shard/ + ProofPlan) | 550+ | ~same as Phase 1 | < 80% of Phase 1 (parallelism) |
| 3 | +1450 (F1-F12 framework) | 600+ | ~same | ~same |
| 4 | +1000 (optimizations) | 650+ | ~same | < 50% of Phase 2 |

### 12.2 Soundness Milestones

| Milestone | Phase | Description |
|-----------|-------|-------------|
| All constraints proven | 0 | Every AIR constraint checked in E2E tests |
| All buses balanced | 0 | Cross-chip LogUp Σ=0 in every test |
| C1 resolved | 1 | Cumsums PCS-committed |
| Per-column independence | 2 | Shards provable independently |
| Extension soundness | 3 | Custom chips cannot break bus balance |
| Strategy soundness | 4 | Misclassification detected by bus imbalance |

### 12.3 Extensibility Milestones

| Milestone | Phase | Description |
|-----------|-------|-------------|
| App chip compiles | 3a | `define_chip_set! { include TabulaCoreAir; + AppChip }` works |
| App bus works | 3a | `BusId::app(1)` + `define_bus!` in app crate |
| App opcode works | 3b | `impl OpcodeSpec` in app crate, dispatched by interpreter |
| App VC works | 3c | `impl VectorCommitment` in app crate, used by prover |
| Lighter DEX feasible | 3d | All 7 blocked requirements unblocked |

---

## §13 Relationship to Existing Documents

| Document | Relationship |
|----------|-------------|
| [tabula-machine-architecture.md](tabula-machine-architecture.md) | Target architecture — this roadmap implements it |
| [extensibility-architecture.md](extensibility-architecture.md) | Detailed API definitions — Phase 3 implements them |
| [tabula-native-optimizations.md](../research/tabula-native-optimizations.md) | 8 optimizations — Phase 2+4 realize them |
| [machine-layer-architecture.md](../research/machine-layer-architecture.md) | Option C decision — Phase 1 executes it |
| [stark-backend-landscape.md](../research/stark-backend-landscape.md) | Backend evaluation — informed Phase 1 design |
| [roadmap-m11-m13.md](roadmap-m11-m13.md) | M11-M13 detail — Phase 0 completes this |
| [proof-optimization-architecture.md](proof-optimization-architecture.md) | KeyRoute, templates — Phase 4+5 implement these |
| [m12-completion-gate.md](m12-completion-gate.md) | M12 gates — Phase 0 satisfies these |

This document supersedes `roadmap-m11-m13.md` for work beyond M13. The M14+ section of that document is absorbed into Phases 1-5 of this roadmap.
