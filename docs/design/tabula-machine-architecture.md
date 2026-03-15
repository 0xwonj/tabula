# Tabula Proving Architecture

> **Version 0.5** — Unified Architecture: Machine + Extensibility
> Supersedes: v0.4 (three-axis), extensibility-architecture.md (absorbed)
> Status: Design — no implementation changes
>
> **This document describes the target architecture, not the current implementation.**
> The current codebase (M11 complete, M12 in progress) uses per-chip `p3-uni-stark`
> proofs, a global `InterTxOrderChip`/`StateColumnChip`, and has no `machine/` or
> `shard/` modules. The `ProofPlan`, `ColumnStrategy`, and extensibility framework
> (F1-F12) are not yet implemented. See [master-roadmap.md](master-roadmap.md) for
> the implementation path from current state to this target.

---

## §1 Design Philosophy

### 1.1 What Tabula Is

Tabula is a **framework for building purpose-built verifiable state machines**. It is not a zkVM. It does not execute arbitrary binaries. It provides composable building blocks — instructions, chips, state strategies, and proof infrastructure — that application developers assemble into optimized, application-specific proving systems.

### 1.2 Five Design Principles

**P1: Derive architecture from protocol invariants.**
The architecture is not designed; it is *derived*. Six invariant properties of the Tabula protocol uniquely determine the proof structure. Any architecture that doesn't reflect these invariants fights the problem domain.

**P2: Own the critical path, outsource commodities.**
Field arithmetic, PCS, and FRI are commodities — use Plonky3 (p3 0.4). Multi-chip orchestration, LogUp wiring, and domain-specific sharding are the critical path — own them entirely. Every successful STARK project (SP1, OpenVM, Risc0, Miden) follows this pattern.

**P3: Bus as universal interface.**
LogUp bus balance (Σ = 0) is the **sole cross-component soundness mechanism**. Components communicate exclusively through typed buses. This makes composition additive: adding a new chip never requires modifying existing chips. Adding a new memory strategy never requires modifying execution chips. The bus is the API contract.

**P4: Zero-Modification Principle.**
Applications customize Tabula **purely in their own crate**. They never fork or modify Tabula's codebase. All customization — chips, buses, state strategies, precompiles — is additive, consumed through Cargo dependency. This is the difference between a framework and a monolith.

**P5: Graduated complexity.**
80% of apps need only DSL-level knowledge (`.tab` files). 15% use standard library opcodes and precompiles. 5% build custom AIR chips. The architecture must serve all three tiers without penalizing the simpler ones.

### 1.3 What Tabula Is NOT

- Not a general-purpose zkVM (no arbitrary binary execution)
- Not a fixed-instruction-set machine (instruction set is extensible)
- Not field-agnostic (KoalaBear is fixed — this is a feature, not a limitation)
- Not a recursive proof system (recursion is a future extension, not a prerequisite)

---

## §2 Foundational Invariants

Six properties of the Tabula protocol **constrain** the architecture. They are not design choices — they are structural facts.

| ID | Invariant | Source | Architectural Consequence |
|----|-----------|--------|---------------------------|
| I1 | **Hierarchical state** | Protocol | State is `{Table → Column → Row → Value}`. Proof decomposes along this hierarchy. |
| I2 | **Static column addressing** | IR definition | `(t, c)` are compile-time constants in every instruction. Only `r` is dynamic. Decomposition is static — no runtime dispatch needed. |
| I3 | **SSA memory model (NF-1~4)** | Compiler | Each `(t,c,r)` read/written at most once per tx. No read-after-write. Constraints that re-enforce these properties are redundant and can be elided. |
| I4 | **Schema typing** | `TableSchema` | Each column has a fixed type (`Bool`, `U64`, `I64`, `Digest`). Trace width `w(T)` is compile-time known. Chips are width-specialized. |
| I5 | **Trusted compiler** | Trust model | `Program::register()` validates all NF rules. Compiler output is part of the trusted setup. Compile-time knowledge can safely flow into proof construction. |
| I6 | **LogUp soundness** | Proof model | Cross-component consistency is solely via bus balance. The verifier checks constraints and bus balance — nothing else. Strategy selection is a prover optimization that cannot affect soundness. |

**These six invariants are PERMANENT.** Any future optimization must be a consequence of these invariants, not a violation of them. This is how the architecture supports unknown future optimizations — anything that follows from the invariants will naturally fit.

---

## §3 Architecture: Three Axes

The architecture decomposes into three orthogonal axes. Each is derived from a subset of invariants. Together they span the complete design space.

```
                Axis 2: Specialization
                (I3+I4+I5: compile-time → prove-time)
                │
                │   ProofPlan
                │   ├── constraint elisions (from NF rules)
                │   ├── width map (from schema types)
                │   ├── execution variant (from pattern recognition)
                │   ├── preprocessed data (batch-invariant)
                │   └── trace bounds (from program budgets)
                │
Axis 1: ────────┼──────── Axis 3:
Hierarchy       │         Composition
(I1+I2)         │         (I6)
│               │         │
│ L0: Batch     │         │  LogUp buses
│ L1: Table     │         │  ├── Memory
│ L2: Column    │         │  ├── Poseidon
│ L3: Access    │         │  ├── RangeCheck
│               │         │  ├── Commitment
│               │         │  ├── Precompile (extensible)
│               │         │  └── App-defined (extensible)
```

| Axis | Derived From | Determines | Extensible? |
|------|-------------|------------|-------------|
| **Hierarchy** | I1 + I2 | How the proof decomposes into levels | New levels (e.g., cross-batch aggregation) |
| **Specialization** | I3 + I4 + I5 | How compile-time knowledge shapes the proof | New elisions, variants, width classes |
| **Composition** | I6 | How components connect without coupling | New buses, new chips on existing buses |

### 3.1 Axis 1: Hierarchical Decomposition

The proof decomposes along the state hierarchy into four levels:

| Level | Scope | Cost Model | Components |
|-------|-------|------------|------------|
| **L0: Batch** | Entire batch | Fixed per batch | ExecutionChip, PoseidonChip, RangeCheckChip |
| **L1: Table** | Per touched table | Per table | SmtTablePathChip |
| **L2: Column (= Shard)** | Per touched `(t,c)` | Per column, strategy-dependent | Memory/State/ShortRun segments, ColumnMeta, SmtColPath |
| **L3: Access** | Per read/write | Per operation | Rows within L2 segments |

**The Shard** (L2) is the fundamental proof unit: one column's complete state transition certificate. Each shard is independent — no cross-column data dependency. This enables embarrassingly parallel proving, fault isolation, and distributed execution.

**Cost decomposition:**
```
cost(batch) = L0_fixed + Σ_tables L1_cost(t) + Σ_columns L2_cost(t,c)
            → per-tx cost approaches L0_fixed/N + marginal as N grows
```

### 3.2 Axis 2: Compile-Time Specialization

The trusted compiler (I5) produces structural guarantees. These flow through **ProofPlan** — a first-class data structure — from compile-time into keygen.

```
tabula-lang          tabula-ir            keygen             prover
──────────          ─────────            ──────             ──────
Source code  ──→    Program     ──→    ProofPlan  ──→   ProvingKey  ──→  Proof
                    │ NF rules          │ elisions         │
                    │ types             │ widths           │
                    │ schemas           │ bounds           │
                    │ budgets           │ preprocessed     │
```

ProofPlan enables:
- **NF-aware constraint elision** — skip constraints guaranteed by compiler
- **Schema-driven width specialization** — W=1/3/8 per column type
- **Template chip selection** — specialized execution for known patterns
- **Preprocessed data reuse** — range check, Poseidon RC, NF selectors computed once

### 3.3 Axis 3: Bus-Mediated Composition

Components communicate exclusively through typed LogUp buses (11 buses in the current implementation: C5 PoseidonPermutation, C6 CommitmentVerification, C8 RangeCheck, C9 StaticTableLookup, C10 ReadAccess, C11 WriteAccess, C12 EmptyColRead, C13 BaseStateEntry, C14 CoalescedWrite, C15 SmtLeafDigest, C16 SmtTableRoot). No component has direct knowledge of any other component's internals.

**Open-world architecture:** Any component can send/receive on any bus if it matches the signature:

```
Execution Axis (L0)             Memory Axis (L2)
──────────────────              ────────────────
Interpreter                     Full + Ssmc
TransferTemplate    ←─ bus ─→   Full + Smt
CompiledChip                    ShortRun
App-defined                     ReadOnly
                                App-defined VC

M execution × N memory = M×N valid combinations
```

Soundness comes from bus balance alone. The verifier doesn't know or care which chip produced which bus message.

---

## §4 Core Components

### 4.1 Component Map

Organized by cluster. Each component has a single responsibility and maps to exactly one module.

**Program Knowledge** — compile-time information that shapes the proof:

| Component | Role | Why It Exists |
|-----------|------|---------------|
| Compiler (`tabula-lang`) | Source → IR, NF enforcement | I5: trusted compiler produces guarantees |
| NF Rules | NF-1~4 structural invariants | I3: SSA memory model |
| Schema/Types | `TableSchema`, `ValueType`, width classes | I4: schema typing |
| Program/Budgets | Registered tx types + resource bounds | DoS prevention, trace sizing |
| ProgramInfo/ProofPlan | Compile-time → prove-time bridge | Axis 2: specialization |

**Execution** — transaction processing:

| Component | Role | Why It Exists |
|-----------|------|---------------|
| Interpreter | IR dispatch + opcode execution | Core computation engine |
| Expr Resolution | `RowExpr`/`ValueExpr` evaluation | Operand evaluation |
| Batch Execution | Sequential tx lifecycle + rollback | Batch semantics |
| Overlay/State | Read cache, write buffer, undo log | NF-based implementation optimization |

**Column-Level Proof** — per-`(t,c)` state transition:

| Component | Role | Why It Exists |
|-----------|------|---------------|
| ColumnStrategy | Unified proof structure + VC selection | I1+I2: one decision per column |
| Memory Consistency | Sorted memory proof per column | I3: SSA + sorted-memory argument |
| SSMC (list + commit + merge) | Hash chain commitment + 3-way merge | Small-column state binding |
| ColumnMeta | Commitment ↔ SMT binding metadata | Column-level bookkeeping |
| Sharding | Per-`(t,c)` proof decomposition | I2: static column addressing |

**Global Binding** — cross-column consistency:

| Component | Role | Why It Exists |
|-----------|------|---------------|
| SMT | Sparse Merkle tree for state root | Protocol requires single state root |
| Poseidon | Domain-separated hash (width-16) | Protocol hash function |

**Proof Infrastructure** — the proving machinery:

| Component | Role | Why It Exists |
|-----------|------|---------------|
| Fiat-Shamir | Challenge derivation (α, β) | LogUp security |
| LogUp Fingerprint | EF4 RLC + cumulative sums | I6: cross-chip soundness |
| AIR Framework | Per-chip polynomial constraints | Constraint expression |
| PCS/FRI | Polynomial commitment scheme | STARK proof backend (from p3) |
| RangeCheck | Preprocessed lookup table [0, 2^16) | Bound checking primitive |
| Gadgets | Reusable constraint primitives | DRY for AIR chips |
| Contract/PublicValues | Proof ↔ protocol binding | Governance compatibility |

**Witness Pipeline** — execution result → proof inputs:

| Stage | Role | Parallelism |
|-------|------|-------------|
| Collector | Touched columns + type map | Sequential (fast) |
| RowBuilder | Per-`(t,c)` init/access/write rows | Embarrassingly parallel |
| ColumnAssembler | Strategy selection + commitment | Embarrassingly parallel |
| Orchestrator | State root computation | Sequential (aggregation) |

### 4.2 Component Relationships

Three relationship types define the topology:

**Synergy (S):** Components that are MORE effective together than apart.
- Static `(t,c)` + Sharding → zero-cost per-column decomposition
- NF rules + ProofPlan → constraint elision
- Schema typing + Width specialization → type-native trace widths

**Orthogonality (O):** Components that are completely independent.
- Execution (L0) ⊥ Memory (L2) — connected only by bus
- SSMC ⊥ SMT — alternative VC strategies, same bus contract
- Interpreter ⊥ Template chips — alternative execution strategies, same bus contract

**Dependency (D):** One component requires another.
- LogUp fingerprint → PCS (cumsums must be committed)
- ColumnMeta → SMT (binds commitments to state root)
- Gadgets → RangeCheck (integer constraints need range checks)

### 4.3 What Is NOT a Component

Deliberately excluded from the component list:

| Omission | Rationale |
|----------|-----------|
| Gas/fee mechanism | Protocol layer, not proof layer |
| Networking/P2P | Infrastructure, orthogonal to proving |
| Data availability | External concern (`tabula-da` crate, future) |
| L1 bridge | External concern (`tabula-bridge` crate, future) |
| Recursion | Extension, not prerequisite |
| GPU acceleration | Implementation detail of trace builder, not architectural |
| `tabula-compiler` | Execution orchestration CLI — infrastructure layer, not proof architecture |
| `tabula-daemon` | HTTP control-plane server — infrastructure layer |
| `tabula-artifact` | Canonical artifact models (ProgramArtifact, BatchFile, etc.) — serialization layer |
| `tabula-web` | Leptos CSR frontend — presentation layer |
| `Emit` instruction | Out-of-protocol: produces no AIR rows, no bus messages, not proven. Emitted events are debug/UX convenience only — the verifier cannot verify them |

---

## §5 Key Design Decisions

### 5.1 Unified ColumnStrategy

**Decision:** Merge proof-structure selection (Untouched/ReadOnly/ShortRun/Full) and commitment-scheme selection (SSMC/SMT) into one per-column decision.

**Rationale:** Both are per-`(t,c)` decisions. They are co-dependent — a ReadOnly column with SMT has different cost characteristics than ReadOnly with SSMC. Unifying them eliminates a combinatorial decision point and ensures the cheapest valid combination is always selected.

```rust
pub enum ColumnStrategy {
    Untouched,                                    // zero cost
    ReadOnly { vc: VcStrategy },                  // meta + SMT path only
    ShortRun { pattern: AccessPattern, vc: VcStrategy }, // lightweight
    Full { vc: VcStrategy },                      // full sorted memory
}

pub enum VcStrategy { Ssmc, Smt }
```

### 5.2 ProofPlan as First-Class Concept

**Decision:** A dedicated data structure bridges compile-time guarantees to prove-time behavior.

**Rationale:** Without ProofPlan, compiler optimizations are invisible to the proof system. With ProofPlan, the compiler can optimize for proving cost — slot reuse reduces trace width, access grouping enables ShortRun, template recognition triggers specialized execution.

### 5.3 Per-Column Sharding (Not Per-Chip)

**Decision:** The proof decomposes per-`(t,c)` column, not per chip type.

**Rationale:** `(t,c)` is a compile-time constant (I2). Columns are independent namespaces (no cross-column memory ordering). Per-column decomposition enables embarrassingly parallel proving, eliminates cross-column ordering gadgets (~300 LOC), and naturally supports incremental state proofs (untouched columns = zero cost).

### 5.4 Two-Round Commitment Protocol

**Decision:** Round 1 commits all main traces, Round 2 commits all permutation traces (LogUp cumsums), then single FRI.

**Rationale:** PCS-committed cumsums solve the C1 soundness gap. Shared PCS across all chips eliminates per-chip FRI overhead. This is the pattern proven by SP1 and OpenVM.

### 5.5 DiscardInteractionBuilder + SymbolicInteractionBuilder

**Decision:** Replace the EmptyMessageBuilder blanket impl with two concrete builders.

**Rationale:**
- `DiscardInteractionBuilder` wraps p3 builders that don't need interaction data (quotient computation). Concrete wrapper, not blanket impl — explicit is better than implicit.
- `SymbolicInteractionBuilder` captures interactions symbolically at keygen time, replacing column-scanning extraction. One-pass evaluation instead of two-pass.

### 5.6 Runtime ChipRegistry (Not Static Enum)

**Decision:** Chip sets are runtime-composed via `ChipRegistry` + `AnyRap`, not compile-time enums.

**Rationale:** OpenVM proves `dyn AnyRap` works in production with negligible overhead (<1% of constraint eval cost). Runtime composition eliminates the need for proc macros (`define_chip_set! include`), enables `.with_chip()` / `.with_extension()` composition, and simplifies the trait hierarchy. The `define_chip_set!` macro, `ChipSet` trait, `TabulaAir` enum, and `StarkAir` alias are all superseded. See [implementation-workplan.md Decision Record](implementation-workplan.md#composition-model-compile-time-enum-vs-runtime-registry).

### 5.7 Typed Buses (Not Raw Indices)

**Decision:** `define_bus!` macro generates typed bus traits, not raw `bus_index: u16`.

**Rationale:** Type safety across 9+ buses prevents subtle fingerprint mismatches. The bus signature IS the API contract between chips. Type errors should be compile-time errors, not soundness bugs.

---

## §6 Extension Model

### 6.1 The Zero-Modification Principle

An application MUST be able to define all customizations **purely in its own crate**:

```
┌─────────────────────────────────────────────────────────────────┐
│ Tabula (immutable Cargo dependency)                              │
│ tabula-core, tabula-ir, tabula-executor, tabula-machine,        │
│ tabula-gadgets, tabula-lang, tabula-std                          │
└──────────────────────────┬──────────────────────────────────────┘
                    Cargo dependency (read-only)
┌──────────────────────────▼──────────────────────────────────────┐
│ App Crate (100% of customization)                                │
│                                                                  │
│  TabulaMachine::builder()                                        │
│      .with_core_chips()                                          │
│      .with_extension(MyExtension)                                │
│      .with_config(production_config())                           │
│      .build()                                                    │
│                                                                  │
│  impl ChipSpec + Air<AB> for AppChip  (auto AnyRap blanket)     │
│  impl ChipExtension for MyExtension                              │
│  impl VectorCommitment for AppVc                                 │
│  impl PrecompileHandler for AppPrecompiles                       │
│  define_bus! for app buses                                       │
│  .tab files (DSL tx types)                                       │
└──────────────────────────────────────────────────────────────────┘
```

### 6.2 Seven Extension Axes

The three architectural axes decompose into seven extension axes for app developers:

| Ext Axis | Maps To | What Apps Customize | Mechanism |
|----------|---------|---------------------|-----------|
| 1. Instruction Set | Axis 2 | Custom computation via precompiles | `PrecompileHandler` trait + precompile chip |
| 2. Chip Composition | Axis 3 | Custom AIR chips | `ChipRegistry` + `AnyRap` + `ChipExtension` |
| 3. Trace Pipeline | Axis 1 | Custom witness flow | `TraceContributor` trait + `WitnessStore` |
| 4. State Commitment | Axis 1 L2 | Custom VC schemes | `VectorCommitment` trait |
| 5. State Opening | Axis 1 L2 | Structural queries (min, successor) | `PropertyOpening` trait |
| 6. Execution Strategy | Axis 2 | Template chips for hot paths | `TemplateChip` trait |
| 7. Proof Composition | Axis 1 L0 | Recursive aggregation | `ProofAggregator` trait |

Each extension axis is **independently extensible**. An extension on one axis composes with all existing strategies on other axes through LogUp buses.

### 6.3 Extension Soundness

Extensions cannot break soundness. The bus contract enforces correctness:

| Attack | Defense |
|--------|---------|
| Custom chip sends wrong bus message | Bus fingerprint mismatch → Σ ≠ 0 |
| Custom VC computes wrong digest | CommitmentVerification bus imbalance |
| Template chip omits a write | Memory bus send with no receive → Σ ≠ 0 |
| Wrong strategy classification | ShortRun constraint violation or bus imbalance |

The verifier is agnostic to which chips produced which messages. It checks constraints and bus balance.

### 6.4 Developer Experience Spectrum

| Tier | Who | What They Write | ZK Knowledge |
|------|-----|-----------------|--------------|
| DSL | App developer | `.tab` files | None |
| Standard library | App developer | `include StdLib;`, `@ecdsa_verify()` | None |
| Custom opcode | Framework contributor | `OpcodeSpec` impl (~100 LOC) | Low |
| Custom precompile | App developer (ZK) | Precompile chip (~300-500 LOC) | Medium |
| Custom VC | App developer (ZK) | `VectorCommitment` + AIR chip (~500-1000 LOC) | High |

### 6.5 Framework Prerequisites

The Zero-Modification Principle requires one-time framework changes (~1,450 LOC):

| ID | Change | Scope | Enables |
|----|--------|-------|---------|
| F1 | `BusId` newtype (replaces closed enum) | ~50 LOC | App-defined buses |
| F2 | `ChipExtension` trait (registers chips + witness logic) | ~150 LOC | Extension packaging |
| F3 | `DynTraceContributor` (object-safe `TraceContributor`) | Phase 1 | Auto-wired trace pipeline |
| F4 | `WitnessStore` typed key-value store | ~100 LOC | Chip data exchange |
| F5 | `VectorCommitment` trait | ~100 LOC | Custom state commitments |
| F6 | `PropertyOpening` trait | ~100 LOC | Structural queries |
| F9 | `Precompile` IR variant | ~50 LOC | Generic computation dispatch |
| F10 | `PrecompileHandler` trait | ~50 LOC | App-defined execution |
| F11 | `TemplateChip` trait | ~200 LOC | Specialized tx execution |
| F12 | `tabula-machine::prelude` | ~50 LOC | Stable p3 re-exports |
| F13 | `op_precompile` selector + `PrecompileBus` | ~100 LOC | ExecutionChip precompile support |

> **Note:** F7 (`OpcodeSpec`) and F8 (`define_instruction_set!`) removed — precompile pattern replaces per-opcode extensibility. F2 changed from `define_chip_set! include` to `ChipExtension`. `AnyRap` + `ChipRegistry` (Phase 1) provide the underlying composition mechanism.

After these changes, all app development requires zero changes to Tabula's codebase.

### 6.6 API Stability Tiers

| Tier | Guarantee | What's Included |
|------|-----------|-----------------|
| **S (Stable)** | Breaking only on major versions | `Value`, `CellKey`, `Transaction`, `Batch`, `Program`, core types |
| **A (Extension)** | May evolve on minor versions (additive) | `ChipSpec`, `VectorCommitment`, `OpcodeSpec`, `define_bus!`, macros |
| **I (Internal)** | No guarantee | Chip internals, column layouts, gadgets, constraint details |

Apps depending only on S+A survive all minor version upgrades.

---

## §7 Required Properties

Properties that MUST hold for the architecture to be sound and correct.

### 7.1 Soundness Requirements

| Property | Why Required | Consequence If Violated |
|----------|-------------|------------------------|
| LogUp bus balance Σ = 0 | Sole cross-chip soundness mechanism (I6) | Forged memory operations, broken state transitions |
| PCS-committed cumsums | Without this, cumsums are free variables | Prover can fabricate bus balance |
| Fiat-Shamir challenges bound to commitments | Prevents adaptive attacks | Challenge manipulation |
| Single state root transition | Protocol requires verifiable state transition | Invalid state proven valid |
| `(t,c)` static in IR | Memory decomposition correctness (I2) | Column independence violated |
| NF-1~4 enforced by compiler | Constraint elision correctness (I3+I5) | Elided constraints were actually needed |

### 7.2 Correctness Requirements

| Property | Why Required |
|----------|-------------|
| Per-column independence at L2 | Shards must be independently provable |
| Two-round commitment ordering | Challenges must follow commitments |
| Preprocessed data immutability | Keygen-time data must not vary per batch |
| Width-class matching (schema → trace width) | Value encoding must be consistent |
| ColumnStrategy covers all access patterns | No column left unproven |

### 7.3 Performance Requirements

| Property | Why Required |
|----------|-------------|
| Untouched columns = zero cost | Batch efficiency proportional to touched state |
| Per-column parallelism | Practical proving time for many-column state |
| Shared PCS | Proof size independent of chip count |

---

## §8 Non-Required Properties (Deliberate Design Choices)

These are choices, not requirements. They could change without breaking the architecture.

| Choice | Current | Alternatives | Why Current |
|--------|---------|-------------|-------------|
| Base field | KoalaBear (2^31-2^24+1) | Goldilocks, Mersenne31 | p3 ecosystem, 32-bit friendly |
| Hash function | Poseidon2 width-16 | Rescue, Griffin, Blake3 | ZK-native, p3 native support |
| Extension field | KoalaBear^4 (~124-bit) | KoalaBear^3, larger extension | Balance of security/performance |
| FRI parameters | log_blowup=3, queries=TBD | Different blowup/query tradeoffs | Degree-9 constraint support |
| VC threshold | ~100-300 rows | Different threshold | Calibrated by benchmark (B7) |
| Chip set dispatch | Runtime `ChipRegistry` + `dyn AnyRap` | Static enum | Runtime composition, negligible vtable overhead |
| Instruction format | 13 core + extensible | Different core set | Minimal but sufficient |
| SMT depth | 16-24 levels | Different depth | Expected table/column count |
| Value encoding split | 30+30+4 for U64 | 31+31+2 (WRONG: exceeds p) | Fits KoalaBear range cleanly |

---

## §9 Optimization Landscape

### 9.1 All Optimizations as Consequences

Every optimization is a consequence of the six invariants, not an ad-hoc addition.

> **Implementation status:** Optimizations #2 (sharding), #3 (late binding/ShortRun), #4 (width
> specialization), and #7 (co-design/templates) are **not yet implemented**. ShortRun routing
> is defined in `witness/route.rs` but `route_keys()` always produces `SortedMemory` for written
> keys. `TemplateId` and `ProgramInfo` exist as data types only — no program analyzer populates
> them. These are Phase 2-4 work items in the [master roadmap](master-roadmap.md).

| # | Optimization | Derives From | How |
|---|-------------|-------------|-----|
| 1 | NF-aware constraint elision | I3 + I5 | ProofPlan.elisions from compiler NF guarantees |
| 2 | Static-coordinate sharding | I1 + I2 | Hierarchy IS per-column decomposition |
| 3 | Witness-driven late binding | I6 | Bus balance ensures any valid strategy is sound |
| 4 | Schema-driven width | I4 | `w(T)` determines shard trace width |
| 5 | Dual-axis composition | I6 | Bus decouples execution from memory |
| 6 | Batch amortization | I1 | L0=fixed, L2=marginal; cost model itself |
| 7 | Compilation-proving co-design | I5 | ProofPlan IS the co-design interface |
| 8 | Incremental state proofs | I1 + I2 | Untouched shards = zero cost |

### 9.2 Emergent Optimizations (from component analysis)

| Optimization | Source | Effect |
|-------------|--------|--------|
| Unified strategy selection | ColumnStrategy | One decision determines proof + commitment |
| Preprocessed reuse | PreprocessedCatalog | Range check, Poseidon RC computed once |
| Pipeline parallelism | 4-stage witness pipeline | Shard trace building overlaps with assembly |
| Gadget elimination | Per-column sharding | ~300 LOC removed (lex.rs, segment.rs) |
| Ordering simplification | Per-column isolation | `(r,τ)` ordering only (no `(t,c)` comparison) |

### 9.3 Future Optimization Space

The architecture naturally supports optimizations that don't exist yet:

| Future Direction | Extension Point | Why It Fits |
|-----------------|-----------------|-------------|
| New shard strategy (e.g., DirectVC) | `ColumnStrategy` variant | L2 decomposition |
| New VC scheme | `VcStrategy` variant | Bus contract unchanged |
| New execution variant | `ExecutionVariant` | L0 ⊥ L2 orthogonality |
| New data type (e.g., W=16) | `SchemaType` variant | Width parameterization |
| GPU-accelerated NTT | Trace builder internal | No architectural change |
| Recursive aggregation | Per-shard standalone proofs | Shard independence |
| Cross-batch caching | Shard commitment persistence | Shard is the caching unit |
| Compiled per-program AIR | `ExecutionVariant::Compiled` | Axis 2 specialization |

---

## §10 Proving Protocol

### 10.1 End-to-End Flow

```
Source → Compiler → Program → ProofPlan → keygen → (ProvingKey, VerifyingKey)
                                                           │
Batch → Executor → ExecutionResult → Witness Pipeline → BatchWitness
                                                           │
                                        ProvingKey + BatchWitness → Traces
                                                           │
Round 1: Commit main traces ─────────────────────→ C_main
Round 2: Derive (α,β), build permutation traces ──→ C_perm  (cumsums PCS-committed)
Round 3: Quotient polynomials ─────────────────────→ C_quot
Round 4: Single FRI opening ───────────────────────→ TabulaProof
```

### 10.2 Proof Structure

```rust
pub struct TabulaProof {
    pub commitments: [Commitment; 3],      // main, permutation, quotient
    pub opened_values: OpenedValues,        // all chips' opened values
    pub fri_proof: FriProof,                // single FRI for all polynomials
    pub chip_data: Vec<ChipProofData>,      // per-chip metadata + cumsums
    pub public_values: PublicValues,         // protocol binding
}

pub struct PublicValues {
    pub old_root: [KoalaBear; 8],
    pub new_root: [KoalaBear; 8],
    pub plan_digest: [u8; 32],              // which program
    pub batch_digest: [u8; 32],             // which batch
    pub tx_outcomes_digest: [u8; 32],       // success/failure per tx
    pub schema_version: u32,                // governance compatibility
}
```

### 10.3 Verification

```
1. Check schema_version against contract compatibility
2. Validate chip manifest against VerifyingKey
3. Verify plan_digest matches expected program
4. Re-derive Fiat-Shamir challenges (α, β from C_main)
5. For each chip: check AIR constraints + permutation constraints
6. Verify single FRI opening proof
7. Check Σ cumsum_final = 0 across ALL chips
8. Check public values (old_root, new_root, batch_digest)
```

---

## §11 Module Layout

```
crates/proof/src/
├── machine/                   # Proving infrastructure (Axis 2+3)
│   ├── config.rs              # STARK config (KoalaBear + Poseidon2 + FRI)
│   ├── plan.rs                # ProofPlan, ExecutionPlan, ColumnPlan
│   ├── keygen.rs              # ProofPlan → ProvingKey / VerifyingKey
│   ├── prover.rs              # Two-round shared-PCS prover
│   ├── verifier.rs            # Multi-trace verifier
│   ├── proof.rs               # TabulaProof, PublicValues
│   ├── permutation.rs         # EF4 fingerprint + cumsum
│   ├── challenges.rs          # Fiat-Shamir
│   ├── rap.rs                 # DiscardInteractionBuilder
│   └── symbolic.rs            # SymbolicInteractionBuilder
│
├── shard/                     # Per-column proof units (Axis 1 L2)
│   ├── mod.rs                 # ColumnStrategy, VcStrategy, Shard
│   ├── builder.rs             # build_shard_traces() dispatch
│   ├── memory.rs              # MemorySegment<W>
│   ├── state.rs               # StateSegment<W> (SSMC)
│   ├── meta.rs                # MetaSegment (ColumnMeta)
│   ├── short_run.rs           # ShortRunSegment<W>
│   └── path.rs                # SmtColPathSegment
│
├── air/                       # Constraint framework (Axis 3)
│   ├── any_rap.rs             # AnyRap trait (type-erased chip interface)
│   ├── builder.rs             # InteractionAirBuilder
│   ├── bus_macro.rs           # define_bus!
│   ├── bus.rs                 # Bus definitions
│   ├── interaction.rs         # BusId, AirInteraction
│   └── columns.rs             # Column struct pattern
│
├── chips/                     # Shared chips (Axis 1 L0)
│   ├── execution/             # ExecutionChip (with variants)
│   ├── poseidon/              # PoseidonChip
│   ├── range_check/           # RangeCheckChip (preprocessed)
│   ├── static_table/          # StaticTableChip
│   └── smt_table_path/        # SmtTablePathChip (L1)
│
├── gadgets/                   # Constraint primitives
├── trace/                     # Trace building (follows hierarchy)
├── witness/                   # 4-stage pipeline
└── debug/                     # Dev constraint checker
```

---

## §12 Final Direction

### 12.1 Implementation Phases

**Phase 1: Machine Layer Foundation**
- `machine/` module: config, prover, verifier, proof, permutation
- Replace `stark/` with shared-PCS architecture
- DiscardInteractionBuilder + SymbolicInteractionBuilder
- All existing 339+ tests pass on new infrastructure

**Phase 2: Shard Architecture**
- `shard/` module: ColumnStrategy, per-column trace building
- Migrate chips from global traces to shard-local traces
- Eliminate lex.rs, segment.rs gadgets
- Per-column parallel trace building

**Phase 3: Extensibility Framework** (prerequisites F1-F13)
- BusId newtype, ChipExtension, WitnessStore
- Precompile variant, PrecompileHandler, PrecompileBus
- VectorCommitment, PropertyOpening, TemplateChip traits
- `tabula-machine::prelude` re-exports

**Phase 4: Optimization**
- NF-aware constraint elision (PreprocessedCatalog)
- Late-binding proof strategy (unified ColumnStrategy)
- Schema-driven width specialization (W=1/3/8)
- Pipeline parallelism (witness → trace overlap)

**Phase 5: Advanced (Future)**
- Template chips for hot-path transactions
- Recursive proof aggregation
- Compiled per-program AIR
- Distributed proving (per-shard independent proofs)

### 12.2 Success Criteria

The architecture is correct when:
1. All 8 optimizations are consequences of the invariants, not additions
2. Any new optimization that follows from the 6 invariants fits without refactoring
3. Apps customize purely in their own crate (zero Tabula modification)
4. Proving cost is proportional to touched state (untouched = zero)
5. Components connect only through buses (no implicit coupling)
6. A single FRI proof covers all chips (proof size ∝ log, not linear)

### 12.3 The ~5-10% Composability Tax

Compared to a fully purpose-built circuit, the framework adds ~5-10% proving overhead from LogUp bus fingerprint computation. In exchange: development velocity, upgrade safety, ecosystem interoperability, and the ability for 80% of applications to require zero ZK knowledge.

---

## Appendix A: Key Type Definitions

```rust
// Axis 2: Specialization
pub struct ProofPlan {
    pub execution: ExecutionPlan,
    pub columns: Vec<ColumnPlan>,
    pub bounds: TraceBounds,
    pub preprocessed: PreprocessedCatalog,
}

// Axis 1: Hierarchy
pub enum ColumnStrategy {
    Untouched,
    ReadOnly { vc: VcStrategy },
    ShortRun { pattern: AccessPattern, vc: VcStrategy },
    Full { vc: VcStrategy },
}
pub enum VcStrategy { Ssmc, Smt }

// Machine
pub struct ProvingKey {
    pub chips: Vec<ChipKey>,
    pub interactions: Vec<Vec<SymbolicInteraction>>,
    pub preprocessed: Vec<Option<RowMajorMatrix<KoalaBear>>>,
    pub plan: ProofPlan,
}

pub struct TabulaProof {
    pub commitments: [Commitment; 3],
    pub opened_values: OpenedValues,
    pub fri_proof: FriProof,
    pub chip_data: Vec<ChipProofData>,
    pub public_values: PublicValues,
}
```

## Appendix B: Mapping extensibility-architecture.md

This document absorbs and supersedes `extensibility-architecture.md`. The mapping:

| extensibility-architecture.md Section | This Document |
|---------------------------------------|---------------|
| §1 Vision + Zero-Modification | §1.2 P4, §6.1 |
| §2 Extension Axes | §6.2 |
| §3 Instruction Set (Axis 1) | §6.2 row 1, §6.5 F9-F10 (Precompile) |
| §4 Chip Composition (Axis 2) | §5.6, §6.2 row 2, §6.5 F2 (ChipExtension) |
| §5 Trace Pipeline (Axis 3) | §6.2 row 3, §6.5 F3-F4 |
| §6 State Commitment (Axis 4) | §6.2 row 4, §6.5 F5 |
| §7 State Opening (Axis 5) | §6.2 row 5, §6.5 F6 |
| §8 Execution Strategy (Axis 6) | §6.2 row 6, §6.5 F11 |
| §9 Proof Composition (Axis 7) | §6.2 row 7, §12.1 Phase 5 |
| §10 Precompile System | §6.5 F9-F10 |
| §11 Lighter DEX Case Study | Out of scope (app-level, separate doc) |
| §12 Implementation Roadmap | §12.1 |
| §13 Developer Experience | §6.4 |
| §14 Completeness Checklist | §6.2 (all axes covered) |
| §15 API Stability | §6.6 |

For detailed API definitions (trait signatures, macro syntax, bus definitions), see
the original `extensibility-architecture.md` which remains as a reference for
implementation-level details.

## Appendix C: Related Documents

- [STARK Backend Landscape](../research/stark-backend-landscape.md) — evaluation of external backends
- [Machine Layer Architecture Decision](../research/machine-layer-architecture.md) — Option C rationale
- [Tabula-Native Optimizations](../research/tabula-native-optimizations.md) — 8 optimization patterns
- [Extensibility Architecture](extensibility-architecture.md) — detailed extension API definitions
- [Proof Optimization Architecture](proof-optimization-architecture.md) — KeyRoute, template chips
- [AIR Chip Architecture](air-chip-architecture.md) — chip 3-file pattern, column structs

## Appendix D: Version History

| Version | Core Idea | Weakness |
|---------|-----------|----------|
| v0.1 | Hook-based machine layer | Optimizations bolted on |
| v0.2 | Column-Shard (per-column proof) | 2/8 optimizations still bolt-ons |
| v0.3 | Three-axis architecture | HybridVC/ShardStrategy separate; coarse components |
| v0.4 | Unified ColumnStrategy + 32 components | Machine + extensibility separate documents |
| v0.5 | Unified architecture | Current |
