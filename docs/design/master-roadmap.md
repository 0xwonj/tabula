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

## Layered Composition Model

The proof system is organized into three layers. Only Layer 1 is app-customizable.

```
┌──────────────────────────────────────────────────────────────┐
│  Layer 0: Core (fixed — Tabula's identity)                   │
│                                                              │
│  Execution:    ExecutionChip, StaticTableChip                 │
│  Memory:       InterTxOrderChip  (internal MemoryModel trait) │
│  Root Proof:   ColumnMetaChip, SmtColPathChip,               │
│                SmtTablePathChip  (internal RootProof trait)    │
│  Bus Consumers: PoseidonChip, RangeCheckChip (BusConsumer)   │
├──────────────────────────────────────────────────────────────┤
│  Layer 1: Column Commitment (pluggable — app choice)         │
│                                                              │
│  ColumnCommitment trait (batch API)                           │
│    "ssmc" → SsmcCommitment → StateColumnChip (global)        │
│    "smt"  → SmtCommitment  → (no extra chip)                 │
│    "custom" → CustomCommitment → CustomChip (global or shard)│
│                                                              │
│  SSMC/SMT are default-provided plugins, not hardcoded core.  │
│  Bus contract: Memory bus receive → CommitVerif bus send.     │
├──────────────────────────────────────────────────────────────┤
│  Layer 2: Bus Consumers (auto-collected)                     │
│                                                              │
│  BusConsumer trait: declare consumed_buses(), collect()        │
│  PoseidonChip, RangeCheckChip, ...extensible                  │
└──────────────────────────────────────────────────────────────┘
```

**Key insight**: StateColumnChip is NOT core — it is the SSMC scheme's implementation detail. It registers via `with_commitment("ssmc", ...)` alongside custom schemes. ColumnMetaChip receives commitment values from ANY scheme via CommitVerif bus without knowing the source.

**Internal traits** (`pub(crate)`, not app-facing):
- `MemoryModel` — currently `GlobalSortedMemory` (InterTxOrderChip). Enables future A/B benchmarking.
- `RootProof` — currently `SmtRootProof` (ColumnMetaChip + SmtPath chips). Enables future replacement.

---

## Builder API

```rust
// Default usage
let machine = TabulaMachine::builder()
    .with_core_chips()            // Layer 0 (memory + root proof)
    .with_default_commitments()   // "ssmc" + "smt" plugins
    .build();

// App developer: add custom commitment
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_default_commitments()
    .with_commitment("accumulator", AccumulatorCommitment::new())
    .with_proof_plan(|plan| {
        plan.set_scheme(TableId(5), ColId(0), "accumulator");
    })
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

Four confirmed optimization directions, each derived from invariants:

| Direction | Derives From | Effect |
|-----------|-------------|--------|
| D1: Poseidon chain delegation | I6 (bus composition) | Eliminate StateColumn hash chains, 261→163 cols |
| D2+D3: Algebraic accumulator | I6 (bus composition) | Replace hash chain with order-independent accumulator, 163→73 cols |
| D4: Recursive composition | I2 (static addressing) | Per-column parallel proving, O(1) proof size |
| KeyRoute (ShortRun) | I3 (SSA) + I4 (schema) | Lightweight chip for single-tx access patterns |
| Template chips | I3 (SSA) | Specialized execution for known tx patterns, 278→~60 cols |
| NF-aware constraint elision | I3 (SSA) + I5 (trusted compiler) | Remove redundant AIR constraints, ~15 cols saved |
| Width specialization | I4 (schema typing) | Per-type chip instantiation (W=1, W=3, W=8) |

See [proof-optimization-architecture.md](proof-optimization-architecture.md) for detailed analysis. See [full-sharding-research.md](full-sharding-research.md) for sharding-specific research.

---

## Success Criteria

| Milestone | Measurable |
|-----------|------------|
| End-to-end proof for any valid DSL program | All E2E tests green |
| C1 soundness resolved | Cumsums PCS-committed and verified |
| Pluggable commitment schemes | ColumnCommitment trait with SSMC/SMT implementations |
| Zero-modification extensibility | Example app crate compiles and proves with custom chip + bus + precompile |
| D1 Poseidon delegation | Global width 261→163 cols |
| Proving time < 50% of baseline | Performance benchmark |

---

## Related Documents

| Document | Relationship |
|----------|-------------|
| [commitment-architecture-research.md](commitment-architecture-research.md) | Global vs shard quantitative analysis |
| [full-sharding-research.md](full-sharding-research.md) | Per-column sharding research and ideal protocol |
| [proof-optimization-architecture.md](proof-optimization-architecture.md) | Two orthogonal optimization axes (execution + memory layer) |
| [extensibility-architecture.md](extensibility-architecture.md) | Detailed API definitions for extensibility traits |
| [custom-type-extensibility.md](custom-type-extensibility.md) | Type system extension: TypeTag, TypeEncoding, bus width |
| [sharded-protocol-design.md](sharded-protocol-design.md) | Shard chip protocol design |
| [architecture.md](architecture.md) | Workspace-level architecture specification |
