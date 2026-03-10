# Tabula Architecture Overview

> Architecture principles, composition model, and design decisions for the Tabula proving framework.
> Date: 2026-03-09
> Related: [commitment-architecture-research.md](commitment-architecture-research.md), [full-sharding-research.md](full-sharding-research.md), [proof-optimization-architecture.md](proof-optimization-architecture.md)

---

## Current State

| Metric | Value |
|--------|-------|
| Crates | 17 (`core`, `contract`, `ir`, `executor`, `commitment`, `stark`, `gadgets`, `chips`, `witness`, `machine`, `lang`, `artifact`, `driver`, `cli`, `daemon`, `web`) |
| Source LOC | ~34,500 |
| Tests | 979 functions across workspace |
| Chips | 9 core + 3 shard types (MemoryShard, StateShard, MetaShard) |
| LogUp buses | 11 — all with positive/negative tests |
| E2E STARK tests | 6 passing (DSL → compile → execute → witness → trace → prove → verify) |
| Known soundness gap | None (C1 resolved: cumsums PCS-committed) |

---

## Invariants

Six properties constrain the architecture:

| ID | Invariant |
|----|-----------|
| I1 | Hierarchical state: `{Table → Column → Row → Value}` |
| I2 | Static column addressing: `(t,c)` compile-time constants |
| I3 | SSA memory model (NF-1~4) |
| I4 | Schema typing: per-column type, compile-time width |
| I5 | Trusted compiler: `Program::register()` validates NF |
| I6 | LogUp soundness: bus balance as sole cross-chip mechanism |

## Design Principles

1. **Derive architecture from invariants** — the architecture is discovered, not designed
2. **Own the critical path** — field/PCS from p3, orchestration/LogUp/sharding owned
3. **Bus as universal interface** — LogUp is the sole composition mechanism
4. **Zero-Modification Principle** — apps never fork Tabula's codebase
5. **Every optimization is a consequence** — if it follows from I1-I6, it fits; if not, it doesn't belong

---

## Non-Goals

| Non-Goal | Rationale |
|----------|-----------|
| Base field change | BabyBear is fixed. The entire ecosystem (p3, chips, encoding) depends on it. |
| General-purpose computation | Tabula is a state machine framework, not a zkVM. No arbitrary binary execution. |
| Recursive proofs as prerequisite | Recursion is a future extension. The architecture works without it. |
| GPU/hardware acceleration | Implementation detail of trace builders. Not an architectural concern. |
| L1 bridge / data availability | Separate crates, separate roadmap. |
| Formal verification | Desirable but separate effort. Correctness comes from testing + specification. |

---

## Three-Tier Proof Architecture

Full sharding is the base architecture. The proof system produces C+2 independent proofs per batch (1 execution + C column + 1 root).

```
┌──────────────────────────────────────────────────────────────┐
│  Tier 1: Execution Proof (1, global)                         │
│                                                              │
│  ExecutionChip, StaticTableChip, PoseidonLocal, RCLocal      │
│  Memory bus SENDS → cross-proof cumsum export                │
├──────────────────────────────────────────────────────────────┤
│  Tier 2: Column Proofs (C, parallel — pluggable commitment)  │
│                                                              │
│  ColumnCommitment trait (per-column API)                      │
│    "ssmc" → SsmcCommitment → MemoryShard<W> + StateShard<W>  │
│    "smt"  → SmtCommitment  → (lightweight)                   │
│    "custom" → CustomCommitment → user-defined chips           │
│  + MetaShard, PoseidonLocal, RCLocal per column proof         │
│  Memory bus RECEIVES → cross-proof cumsum export              │
│                                                              │
│  Width polymorphism: each column proof uses its own W.        │
│  SSMC/SMT are default-provided plugins, not hardcoded core.   │
├──────────────────────────────────────────────────────────────┤
│  Tier 3: Root Proof (1, lightweight)                         │
│                                                              │
│  Cumsum balance: cumsum_exec + Σ cumsum_col[i] = 0           │
│  SMT paths: Com_old/Com_new inclusion → root transition      │
│  (internal RootProof trait)                                   │
├──────────────────────────────────────────────────────────────┤
│  Bus Consumers (auto-collected, per proof instance)           │
│                                                              │
│  BusConsumer trait: declare consumed_buses(), collect()        │
│  PoseidonLocal, RangeCheckLocal, ...extensible                │
└──────────────────────────────────────────────────────────────┘
```

**Key insight**: StateShard is NOT core — it is the SSMC scheme's implementation detail. It registers via `with_commitment("ssmc", ...)` alongside custom schemes. The root proof receives commitment values from ANY scheme via cumsum + public values without knowing the source.

**Internal traits** (`pub(crate)`, not app-facing):
- `RootProof` — currently `SmtRootProof` (SMT path chips + cumsum balance). Enables future replacement.

---

## Builder API

```rust
// Default usage (sharded prover)
let machine = TabulaMachine::builder()
    .with_execution_chips()       // Tier 1 (ExecutionChip, StaticTable, PoseidonLocal, RCLocal)
    .with_default_commitments()   // Tier 2: "ssmc" + "smt" plugins
    .with_root_proof()            // Tier 3 (SMT paths, cumsum balance)
    .build();

// App developer: add custom commitment
let machine = TabulaMachine::builder()
    .with_execution_chips()
    .with_default_commitments()
    .with_commitment("accumulator", AccumulatorCommitment::new())
    .with_proof_plan(|plan| {
        plan.set_scheme(TableId(5), ColId(0), "accumulator");
    })
    .with_root_proof()
    .build();
```

---

## API Stability Tiers

| Tier | Guarantee | Examples |
|------|-----------|---------|
| **S (Stable)** | Breaking only on major versions | `Value`, `CellKey`, `Transaction`, `Batch`, `Program` |
| **A (Extension)** | May evolve on minor (additive) | `ChipSpec`, `AnyRap`, `ChipRegistry`, `ChipExtension`, `TabulaMachine`, `VectorCommitment`, `PrecompileHandler`, macros |
| **I (Internal)** | No guarantee | Chip internals, column layouts, gadgets |

---

## Optimization Directions

Confirmed optimization directions, each derived from invariants:

| Direction | Derives From | Effect | Design Doc |
|-----------|-------------|--------|------------|
| D1: Poseidon chain delegation | I6 (bus composition) | Delegate SSMC hash chains to PoseidonChip, significant memory-layer col reduction | [proof-optimization-architecture.md](proof-optimization-architecture.md) |
| D2+D3: Algebraic accumulator | I6 (bus composition) | Replace hash chain with order-independent accumulator | [proof-optimization-architecture.md](proof-optimization-architecture.md) |
| D4: Recursive composition | I2 (static addressing) | Per-column parallel proving, O(1) proof size | [full-sharding-research.md](full-sharding-research.md) |
| KeyRoute (ShortRun) | I3 (SSA) + I4 (schema) | Lightweight chip for single-tx access patterns | [proof-optimization-architecture.md](proof-optimization-architecture.md) |
| Template chips / Level 4 AIR | I3 (SSA) + I5 (trusted compiler) | Specialized execution, 278→~60 cols | [execution-chip-evolution.md](execution-chip-evolution.md) |
| NF-aware constraint elision | I3 (SSA) + I5 (trusted compiler) | Remove redundant AIR constraints, ~15 cols saved | [execution-chip-evolution.md](execution-chip-evolution.md) |
| Width specialization | I4 (schema typing) | Per-type chip instantiation (W=1, W=3, W=8) | [execution-chip-evolution.md](execution-chip-evolution.md) |
| Constraint CSE | — (infrastructure) | 5-15× constraint eval speedup | [constraint-compilation.md](constraint-compilation.md) |
| Prover pipeline acceleration | — (infrastructure) | BLAKE3, batch inversion, GPU: 30-80% proving | [prover-pipeline-acceleration.md](prover-pipeline-acceleration.md) |
| Coprocessor factoring | I6 (bus composition) | ExecutionChip 278→~100 cols | [execution-chip-evolution.md](execution-chip-evolution.md) |

See [proof-optimization-architecture.md](proof-optimization-architecture.md) for the two-axis model. See [full-sharding-research.md](full-sharding-research.md) for sharding-specific research.

---

## Success Criteria

| Milestone | Measurable |
|-----------|------------|
| End-to-end proof for any valid DSL program | All E2E tests green |
| C1 soundness resolved | Cumsums PCS-committed and verified |
| Pluggable commitment schemes | ColumnCommitment trait with SSMC/SMT implementations |
| Zero-modification extensibility | Example app crate compiles and proves with custom chip + bus + precompile |
| D1 Poseidon delegation | Memory-layer chip width reduction via hash chain delegation |
| Proving time < 50% of baseline | Performance benchmark |

---

## Related Documents

| Document | Relationship |
|----------|-------------|
| [commitment-architecture-research.md](commitment-architecture-research.md) | Global vs shard quantitative analysis |
| [full-sharding-research.md](full-sharding-research.md) | Per-column sharding research and ideal protocol |
| [proof-optimization-architecture.md](proof-optimization-architecture.md) | Two orthogonal optimization axes (execution + memory layer) |
| [constraint-compilation.md](constraint-compilation.md) | Constraint CSE via symbolic DAG extraction |
| [prover-pipeline-acceleration.md](prover-pipeline-acceleration.md) | Infrastructure: BLAKE3, batch inversion, GPU, NTT |
| [execution-chip-evolution.md](execution-chip-evolution.md) | ExecutionChip Level 0-4 evolution path |
| [extensibility-architecture.md](extensibility-architecture.md) | Detailed API definitions for extensibility traits |
| [custom-type-extensibility.md](custom-type-extensibility.md) | Type system extension: TypeTag, TypeEncoding, bus width |
| [sharded-protocol-design.md](sharded-protocol-design.md) | Shard chip protocol design |
| [proving-layer-architecture.md](proving-layer-architecture.md) | Protocol math migration + ProofInstance design |
| [codebase-architecture-review.md](codebase-architecture-review.md) | Layer assessment, extension patterns, sharding readiness |
| [architecture.md](architecture.md) | Workspace-level architecture specification |
