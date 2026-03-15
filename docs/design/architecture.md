# Tabula Kernel — Architecture

> **Status**: Draft v0.6.0
> **Scope**: Design philosophy, crate structure, data flow, and architectural decisions for the Tabula Kernel.
> **Prerequisites**: None
> **Normative specs**: [semantics-spec.md](../spec/semantics-spec.md), [proof-spec.md](../spec/proof-spec.md)
> **Target-state companion**: [final-target-architecture.md](./final-target-architecture.md)

---

## Table of Contents

1. [Overview](#1-overview)
2. [Workspace Structure](#2-workspace-structure)
3. [Crate Specifications](#4-crate-specifications)
4. [Core Abstractions](#4-core-abstractions)
5. [Tabula IR: The Instruction Set](#5-tabula-ir-the-instruction-set)
6. [Data Flow: ApplyBatch Pipeline](#6-data-flow-applybatch-pipeline)
7. [Tech Stack](#7-tech-stack)
8. [Design Decisions](#8-design-decisions)
9. [Implementation Phases](#9-implementation-phases)

---

## 1. Overview

Tabula is a **table-native zk state-transition VM kernel**. Its architecture is organized around a strict separation between three stages:

- **Execution** — deterministic state transitions using a local overlay
- **Commitment** — column/table commitments via SSMC/SMT over KoalaBear
- **Proving** — end-to-end STARK correctness proofs for `ApplyBatch`

Two central design constraints govern the architecture:

1. **The executor has zero cryptographic dependencies** — execution correctness is testable and verifiable without any proof system. This is enforced at compile time by the crate dependency graph.
2. **All cryptographic primitives are trait-based** — the system is agnostic to the concrete hash function, signature scheme, and encoding. Phase 1 uses mock implementations; real backends are swapped in later.

The target proof system is **STARK (FRI-based) via Plonky3 over KoalaBear**, aligning with the broader zkVM ecosystem (SP1, RISC Zero, Stwo). The trait-based design keeps SNARK (KZG-based) as a viable alternative.

---

## 2. Workspace Structure

Seven crates, organized by concern:

```
tabula/
├── crates/
│   ├── tabula-core/          # types, traits, errors — zero heavy deps
│   ├── tabula-ir/            # IR definitions + NF validation passes
│   ├── tabula-executor/      # deterministic execution engine
│   ├── tabula-commitment/    # Plonky3/KoalaBear state commitment
│   ├── tabula-proof/         # AIR chips, witness gen, STARK proof
│   ├── tabula-lang/          # DSL compiler (lex → parse → lower)
│   └── tabula-cli/           # binary entry point
├── docs/
│   ├── spec/                 # normative: proof-spec.md, semantics-spec.md
│   ├── design/               # this document, AIR chip guide, optimization design
│   ├── research/             # future: conditional branching, DSL philosophy
│   └── archive/              # completed milestone designs
```

### Dependency DAG

```
              tabula-core
           /   |    |    \      \
  tabula-ir    |    |  tabula-commitment
     |    \    |    |     /
  tabula-executor  |    /
               |   |   /
           tabula-proof
                |
  tabula-lang   |
      |    \    |
           tabula-cli
```

| Crate | Depends On | Key Constraint |
|-------|-----------|----------------|
| `tabula-core` | (none) | Pure types/traits. No heavy deps. |
| `tabula-ir` | core | Compilation logic separated from core. |
| `tabula-executor` | core, ir | **Zero crypto deps** — all injected via traits. |
| `tabula-commitment` | core | Plonky3 behind `stark` feature flag. |
| `tabula-proof` | core, commitment | AIR chips behind `stark` feature flag. |
| `tabula-lang` | core, ir | Zero parser deps — hand-rolled. |
| `tabula-cli` | all | Binary entry point + JSON I/O. |

**Why this matters:** `tabula-executor` cannot import crypto crates — enforced at compile time. Touching executor code never triggers recompilation of the proof system. Each component is independently testable.

---

## 3. Crate Specifications

### 3.1 tabula-core

Foundational types, trait definitions, and error types. Everything else depends on this crate.

- **`state/`** — `TableId`, `ColId`, `RowKey`, `CellKey`, `Value`, `ValueType`, `TableSchema`, `Digest`, `StateRoot`
- **`traits/`** — 8 pluggable abstractions (see §4)
- **`tx.rs`** — `Transaction`, `Batch`, `ProgramBudgets`
- **`event.rs`** — `ExecutionEvent`, `TxOutcome`, `ExecutionResult`
- **`mock.rs`** — Test doubles behind `feature = "mock"`

### 3.2 tabula-ir

IR definitions and validation passes. Extracted from core during M3 to keep core free of compilation logic.

- **`instruction.rs`** — `Instruction` enum (14 variants), `Slot`, `RowExpr`, `ValueExpr`, `ArithOp`, `CmpOp`
- **`program.rs`** — `Program` struct with pass pipeline: canonicalize → typecheck → validate
- **`pass/`** — NF-1 read dedup, NF-2~NF-4 validation, slot renumbering

### 3.3 tabula-executor

Deterministic execution engine. Produces `ExecutionResult` from a batch + state snapshot.

- **`overlay.rs`** — Internally composed of **ExecutionState** (write buffer, read cache, undo log) and **TraceRecorder** (events, logical time). O(1) checkpoint, O(k) rollback.
- **`interpreter.rs`** — Reference interpreter for Tabula IR. Slot-based variable environment.
- **`batch.rs`** — `BatchExecutor`: signature/nonce verification, per-tx rollback orchestration.
- **`consistency.rs`** — Key-local RAM consistency checker.

**Zero crypto dependencies** — all crypto operations injected via traits.

### 3.4 tabula-commitment

State commitment layer over KoalaBear field elements. Manages column and table commitments.

- **`hasher.rs`** — `FieldHasher` trait (field-element-level hashing). See §4.2.
- **`poseidon.rs`** — `PoseidonHasher`: Poseidon2 width-16. Implements both `FieldHasher` and core `Hasher`.
- **`codec.rs`** — `KoalaBearCodec`: schema-typed value ↔ field element encoding.
- **`smt.rs`** — `SparseMerkleTree` over `FieldHasher`.
- **`ssmc.rs`** — `SsmcList`, `SsmcCommitment`, `MergeTrace`: sorted key-value list commitment + 3-way merge.
- **`hybrid.rs`** — `HybridVC`: per-column SSMC/SMT dispatch with threshold-based auto-selection.

Behind `stark` feature flag (Plonky3 deps).

### 3.5 tabula-proof

Proof generation and verification for the `ApplyBatch` statement.

- **`witness/`** — `WitnessGenerator` transforms `ExecutionResult` → `BatchWitness`. Key routing classifies keys into ReadOnly / ShortRun / SortedMemory proof paths.
- **`air/builder.rs`** — `InteractionAirBuilder` trait (extends `AirBuilder + PairBuilder`) for LogUp send/receive declarations.
- **`air/interaction.rs`** — `InteractionKind` enum (8 bus types), `Interaction` struct, `VirtualPairCol` for column references.
- **`air/debug.rs`** — `DebugConstraintBuilder` with `PairBuilder` support for preprocessed traces; `check_logup_balance()` for cross-chip verification.
- **`air/gadgets/`** — Reusable constraint primitives: boolean prefix, integer limbs, null canonicalization.
- **`air/chips/`** — 7 AIR chips (3-file pattern: columns / air / trace):

| Chip | Main Cols | Preprocessed | Role |
|------|-----------|-------------|------|
| ExecutionChip | 170 | — | Instruction-level constraints, SSA slot carry, operand-to-slot linkage |
| GlobalSortedMem | 42 | — | Inter-tx memory consistency (sorted by key + timestamp) |
| GlobalSSMC | 45 | — | SSMC hash chain + boundary constraints |
| GlobalMerge | 52 | — | 3-way merge proof (old + write → new) |
| PoseidonChip | 93 | 17 | Poseidon2 permutation (28 rows/perm), RC verification |
| ColumnMeta | 28 | — | Wires column commitments to state root proofs |
| RangeCheck | 2 | — | 2^16 preprocessed range table |

Behind `stark` feature flag (Plonky3 deps).

### 3.6 tabula-lang

DSL compiler. Hand-rolled lexer, recursive-descent parser with Pratt expression parsing, zero parser dependencies. Pipeline: `compile(&str) → Result<CompiledProgram>` — lex → parse → lower. See [dsl-philosophy.md](../research/dsl-philosophy.md).

### 3.7 tabula-cli

Binary entry point. Thin dispatch in `main.rs`; commands: `execute`, `inspect`, `example`. JSON I/O types in `io.rs`.

---

## 4. Core Abstractions

### 4.1 Type Design Philosophy

**State is `(Table, Column, Row) → Value`.** Four types address this space: `TableId(u32)`, `ColId(u16)`, `RowKey(u64)`, composed into `CellKey { table, col, row }`.

> **Canonical order.** The protocol-level ordering for `CellKey` is **`(t, c, r)`** — table, column, row. All sorting (GlobalSortedMem), hashing (Poseidon sponge), and Merkle encoding use this order. The Rust struct field order matches, so derived `Ord` gives the correct lexicographic ordering.

**`Value` is application-level only.** Four variants: `U64`, `I64`, `Bool`, `Bytes32`. No `Null` variant — absence is modeled as `Option<Value>` in collections and a separate `val_is_null: bool` flag in execution events. This matches the normative two-slot `Read/Write` design ([semantics-spec](../spec/semantics-spec.md) §1.5). No `Field` variant — field element encoding is a commitment-layer concern handled by `ValueCodec`.

**`ExecutionResult` is the Stage 1 → Stage 2 handoff.** Contains `read_set_old`, `write_set_final`, execution events, emitted events, and per-tx outcomes. This struct is the strict boundary — everything upstream is deterministic execution, everything downstream is cryptographic commitment and proving. The proof system constructs a separate AIR trace layout from it; the two are not 1:1.

**Stable types** (changes require spec update): `Instruction` enum, `CellKey`/`Value`/`ValueType`, `ExecutionResult`/`ExecutionEvent`, `ApplyBatchStatement`, `Digest = [u8; 32]`.

### 4.2 Two-Layer Hash Architecture

Hashing operates at two distinct levels, matching the execution/commitment boundary:

```
  tabula-core                    tabula-commitment
  ─────────────                  ──────────────────
  trait Hasher                   trait FieldHasher
  ───────────                    ─────────────────
  hash(&[u8]) → [u8;32]         hash(&[KoalaBear]) → NativeDigest
  byte-level                     field-element-level
  used by: executor, IR Hash     used by: SSMC, SMT, commitment

  PoseidonHasher implements BOTH:
    Hasher    (bytes → KoalaBear limbs → Poseidon → squeeze → bytes)
    FieldHasher (native KoalaBear FE → Poseidon → NativeDigest)
```

**Why two layers?** The executor must remain field-agnostic — it hashes bytes via `Hasher`. The commitment layer operates natively on KoalaBear field elements for STARK efficiency. `PoseidonHasher` bridges both worlds. `MockFieldHasher` provides fast unit tests without real Poseidon.

Domain separation tags prevent cross-context collisions: `0x00` = SSMC, `0x01` = SMT, `0x02` = IR Hash, `0x10` = leaf, `0x11` = tables, `0x12` = cols.

### 4.3 Pluggable Traits

All traits live in `tabula-core/src/traits/`. They keep the system crypto-agnostic — mock implementations for testing, real backends for production.

| Trait | Defined In | Purpose | v1.0 Mock | Future |
|-------|-----------|---------|-----------|--------|
| `Hasher` | `crypto.rs` | Byte-level hashing | Blake3 | Poseidon (in-circuit) |
| `StateSnapshot` | `state.rs` | Read-only state access | InMemoryState (BTreeMap) | RocksDB, SQLite |
| `SigVerifier` | `crypto.rs` | Signature verification | Always-accept | EdDSA-over-KoalaBear |
| `ValueCodec` | `codec.rs` | Value ↔ field element encoding | Borsh bytes | KoalaBearCodec (limb decomp) |
| `NoncePolicy` | `codec.rs` | Replay protection | Sequential | Epoch-based, bitfield |
| `MembershipScheme` | `crypto.rs` | Program membership proofs | Flat hash | Merkle tree |
| `BatchDigester` | `crypto.rs` | Batch → digest | Borsh + Blake3 | Merkle tree of txs |
| `StaticTableProvider` | `state.rs` | Fixed lookup tables | BTreeMap | LogUp/Lasso-linked |

**Usage by crate:**
- **tabula-executor**: StateSnapshot, SigVerifier, NoncePolicy, StaticTableProvider, Hasher (IR Hash instruction)
- **tabula-commitment**: Hasher, ValueCodec, MembershipScheme, BatchDigester + FieldHasher (commitment-local)
- **tabula-proof**: All of the above

> **Note**: The original PCS and ColumnCommitment traits were removed during M4. State commitment now uses `FieldHasher` + SSMC/SMT directly, rather than a generic PCS abstraction.

---

## 5. Tabula IR: The Instruction Set

> **Normative definition**: [semantics-spec.md](../spec/semantics-spec.md) §1. This section describes design philosophy; the spec is authoritative for semantics and NF rules.

### 5.1 Design Philosophy

Tabula uses a **slot-based flat IR** with **True SSA** semantics. Key properties:

- **1 instruction = 1 constraint group.** Direct mapping from IR to AIR trace rows. No hidden expansions.
- **True SSA.** Each destination slot is assigned at most once. Slots are **wires** in the constraint system — no register-file propagation, no intra-tx memory argument needed. Validated by `Program::register()`.
- **Linear execution.** No branches, no loops, no function calls. Straight-line read → compute → assert → write. This is not a limitation — it is a design property that enables efficient STARK proving. See [conditional-branching.md](../research/conditional-branching.md) for the future CFG design.
- **Two-slot absence model.** `Read` produces `(dst_val, dst_is_null)`; `Write` takes `(src_val, src_is_null)`. No `Value::Null` — absence is a separate boolean flag.

### 5.2 Instruction Summary

| Category | Instructions | Notes |
|----------|-------------|-------|
| State | `Read`, `Write`, `Lookup` | Read/Write are 2-slot. Lookup is for static (fixed) tables. |
| Arithmetic | `Arith` (Add/Sub/Mul), `DivMod` | DivMod produces two slots (quotient, remainder). |
| Comparison | `Cmp` (Eq/Ne/Lt/Lte/Gt/Gte) | Produces `Bool`. |
| Boolean | `Not`, `And`, `Or` | Bool → Bool logic. |
| Control | `Assert`, `Select` | Assert = abort-on-false. Select = conditional value (both branches evaluate). |
| Output | `Hash`, `Emit` | Hash = domain-separated Poseidon. Emit = out-of-protocol events. |

14 instruction variants total. See `tabula-ir/src/instruction.rs` for definitions.

### 5.3 Normal Form (NF) Rules

The IR enforces four structural invariants, validated at program registration:

| Rule | Name | Guarantees |
|------|------|-----------|
| NF-1 | Unique-Read | At most one Read per `(t, c, r)` per tx. Enables 1:1 init rows in proof. |
| NF-2 | Unique-Write | At most one Write per `(t, c, r)` per tx. Eliminates intra-tx coalescing. |
| NF-3 | No-Read-After-Write | Cannot Read a cell that was Written in the same tx. Eliminates intra-tx RAW hazards. |
| NF-4 | Key-Alias Resolvability | Row expressions must be provably equal or provably distinct. Rejects ambiguous aliasing. |

These are **compile-time invariants**, not runtime checks. The proof system relies on them to avoid an intra-tx RAM-consistency argument ([proof-spec](../spec/proof-spec.md) §10.6).

### 5.4 Example: Token Transfer

```
// tx type: transfer(from: RowKey, to: RowKey, amount: U64)

READ    s0,s1 ← Balances.balance[Param(0)]   // sender balance (val, is_null)
READ    s2,s3 ← Balances.balance[Param(1)]   // receiver balance (val, is_null)
CMP     s4 ← Gte(Slot(s0), Param(2))         // sender has enough?
ASSERT  Slot(s4)                              // fail if false
ARITH   s5 ← Sub(Slot(s0), Param(2))         // new sender balance
ARITH   s6 ← Add(Slot(s2), Param(2))         // new receiver balance
WRITE   Balances.balance[Param(0)] ← Slot(s5), Bool(false)
WRITE   Balances.balance[Param(1)] ← Slot(s6), Bool(false)
```

8 instructions → 8 trace rows → 8 constraint groups. Source order = execution order = IR instruction order. The developer's mental model matches the machine.

---

## 6. Data Flow: ApplyBatch Pipeline

> **Naming convention.** This document uses **Stage 1/2/3** for pipeline stages (Execution → Commitment → Proving). Proof-spec uses **Layer B/C** for proof scope (intra-tx → inter-tx batch). These are orthogonal.

### 6.1 High-Level Pipeline

```
                    INPUT
                      │
         ┌────────────┴────────────┐
         │ oldStateRoot             │
         │ programRoot              │
         │ Batch (ordered txs)      │
         │ Program (tx type defs)   │
         └────────────┬────────────┘
                      │
       ┌──────────────▼──────────────┐
       │     STAGE 1: EXECUTION       │  ← tabula-executor
       │                              │
       │  Overlay Δ (write-buffer)    │
       │  + Interpreter (walks IR)    │
       │  + Per-tx checkpoint/rollback│
       │                              │
       │  → ExecutionResult           │
       └──────────────┬──────────────┘
                      │
       ┌──────────────▼──────────────┐
       │    STAGE 2: COMMITMENT       │  ← tabula-commitment
       │                              │
       │  SSMC/SMT openings + merges  │
       │  + Value encoding (KoalaBear) │
       │  + State root recomputation  │
       │                              │
       │  → newStateRoot              │
       └──────────────┬──────────────┘
                      │
       ┌──────────────▼──────────────┐
       │    STAGE 3: PROVING          │  ← tabula-proof
       │                              │
       │  ExecutionResult → Witness   │
       │  → AIR trace (7 chips)      │
       │  → STARK proof               │
       │                              │
       │  → proof + newStateRoot      │
       └─────────────────────────────┘
```

### 6.2 Stage 1: Overlay Semantics

The `Overlay` sits on top of a `StateSnapshot` and serves two distinct roles:

**Inter-tx role (required):** Transaction `i+1` must see transaction `i`'s writes. The overlay's write buffer accumulates writes across the batch. This is essential for batch semantics ([semantics-spec](../spec/semantics-spec.md) §2.2).

**Intra-tx role (convenience):** Within a single transaction, the NF rules (§5.3) guarantee that read-cache hits and read-your-writes can never actually trigger. The overlay provides them as defensive depth, but the proof system does not rely on them — no intra-tx RAM-consistency argument is needed.

Three rules govern overlay behavior:

| Rule | Name | Behavior |
|------|------|----------|
| A | Read-Your-Writes | READ(k) checks write buffer first (inter-tx: sees prior txs' writes). |
| B | Read Deduplication | Cache reads from committed state. One opening per unique key. |
| C | Write Coalescing | Last write wins across txs. NF-2 guarantees uniqueness within a tx. |

```
READ(k):
  1. Check write buffer (Rule A) → hit? return
  2. Check read cache (Rule B)   → hit? return
  3. Read from StateSnapshot     → cache, return

WRITE(k, v):
  1. Insert/overwrite in write buffer (Rule C)
```

### 6.3 Stage 1: Per-Transaction Rollback

Failed transactions (ASSERT failure) must not pollute the batch state:

1. **Checkpoint** overlay state (O(1) — records undo-log position)
2. **Execute** transaction
3. **If failure**: capture partial events → rollback overlay (O(k) — replays undo log) → record `TxOutcome::Failed` → continue
4. **If success**: discard checkpoint → record `TxOutcome::Success`

This matches Ethereum semantics: a failed tx is skipped, successful txs persist.

### 6.4 Stage 2: Commitment Update

`ExecutionResult` drives the commitment update:

1. **Group** `ReadSet_old` by `(tableId, colId)` — proof cost scales with #groups, not #reads
2. **Verify** old commitments via SSMC membership proofs / SMT openings
3. **Apply** `WriteSet_batch_final` — SSMC 3-way merge (old + writes → new) for small columns, SMT updates for large columns
4. **Recompute** column commitments → table commitments → `newStateRoot`

Per-column strategy is auto-selected by `HybridVC` based on row count (SSMC ≤ threshold, SMT > threshold).

### 6.5 Stage 3: Proof Generation

The proof system transforms `ExecutionResult` into a STARK proof that the state transition is correct:

```
ExecutionResult
    │
    ▼
WitnessGenerator ─── key routing (ReadOnly / ShortRun / SortedMemory)
    │
    ▼
BatchWitness (per-column: init rows + access rows)
    │
    ▼
AIR Trace Generation (7 chips, each: witness → matrix)
    │
    ▼
LogUp Cross-Chip Wiring (memory bus, SSMC bus, merge bus, range check, ...)
    │
    ▼
STARK Proof (Plonky3/FRI over KoalaBear)
```

**Chip composition.** Each chip constrains one aspect of the state transition:

- **ExecutionChip** — verifies each IR instruction produced the correct output. SSA slot carry propagates values between instructions.
- **GlobalSortedMem** — sorts all memory accesses by `(t, c, r, timestamp)` and verifies last-write semantics via LogUp. Init rows (τ=0) seed base state values.
- **GlobalSSMC** — verifies SSMC hash chain integrity for small columns.
- **GlobalMerge** — verifies the 3-way merge (old list + write set → new list) is complete and correct.
- **PoseidonChip** — constrains Poseidon2 permutations (28 rows per permutation, width-16).
- **ColumnMeta** — wires column commitments to SMT inclusion/update proofs.
- **RangeCheck** — 2^16 preprocessed lookup table for integer limb range checks.

Chips communicate via **LogUp buses** — multiset equality arguments that enforce cross-chip consistency without merging traces. See [proof-spec](../spec/proof-spec.md) §7-§8 and [air-chip-architecture.md](./air-chip-architecture.md) for constraint details.

> **`receiptsDigest`** is currently out-of-protocol: `Emit` events are not verified by the proof system. `receiptsDigest` is a convenience output for debugging — not part of `ApplyBatchStatement` public inputs.

---

## 7. Tech Stack

### 7.1 Core Dependencies

| Concern | Crate | Rationale |
|---------|-------|-----------|
| Deterministic serialization | `borsh` 1.x | Canonical binary encoding for commitments. Byte-level determinism. |
| Human-readable serialization | `serde` 1.x | JSON for configs, debugging, test fixtures. |
| Hashing (out-of-circuit) | `blake3` 1.x | Fast, 256-bit, well-audited. Mock implementations. |
| Error handling | `thiserror` 2.x / `anyhow` 1.x | Typed errors in libraries, ergonomic errors in CLI. |
| Structured logging | `tracing` 0.1.x | Span-based logging for execution trace inspection. |
| CLI | `clap` 4.x | Standard CLI framework. |
| Property testing | `proptest` 1.x | Critical for overlay/consistency logic. |
| Collections | `std` BTreeMap/BTreeSet | **Deterministic iteration order. No HashMap in executor.** |

### 7.2 STARK Dependencies (behind `stark` feature flag)

| Concern | Crate | Rationale |
|---------|-------|-----------|
| STARK proof system | `p3-uni-stark`, `p3-fri` | Plonky3 — production-ready, audited, SP1-validated. |
| Field arithmetic | `p3-field`, `p3-koala-bear` | KoalaBear (p = 2^31 − 2^24 + 1 = 2130706433). |
| Hash (in-circuit) | `p3-poseidon2` | Poseidon2 over KoalaBear. Width-16, rate-8. |
| AIR framework | `p3-air`, `p3-matrix` | Native LogUp support for memory consistency arguments. |
| SNARK wrapping | TBD | Optional: STARK→Groth16 for on-chain verification (~200 bytes). |

---

## 8. Design Decisions

### D1: Multi-crate workspace

**Choice**: 7 crates with compile-time dependency enforcement.

- `tabula-executor` cannot accidentally import crypto — the compiler prevents it.
- Proof crate changes don't rebuild executor. Each component is independently testable.
- **Con**: More boilerplate (`Cargo.toml`, re-exports). Justified by the clear boundaries.

### D2: BTreeMap everywhere in executor

**Choice**: `BTreeMap`/`BTreeSet` exclusively. No `HashMap`.

- Deterministic iteration order is **non-negotiable** for reproducible execution.
- The log factor is negligible — touch set per batch is thousands of keys, not millions.

### D3: Concrete `Value` enum

**Choice**: Application-level types only (`U64`, `I64`, `Bool`, `Bytes32`). No `Null`, no `Field`.

- No field size assumptions leak into the executor. KoalaBear (31-bit), Goldilocks (64-bit), BN254 (254-bit) all supported via `ValueCodec`.
- Absence is `Option<Value>`, not a variant — cleaner type safety, matches 2-slot Read/Write design.
- Field encoding is a commitment-layer concern. Conversion cost is negligible vs. proof operations.

### D4: Slot-based flat IR with True SSA

**Choice**: `Vec<Instruction>` with `Slot` indices. Each slot assigned at most once.

- 1 instruction = 1 constraint group. Direct mapping to proof system.
- True SSA means slots are **wires** — no register-file propagation, no intra-tx memory argument.
- No control flow in v1.0. This is deliberate: the proof system requires linear execution. See [conditional-branching.md](../research/conditional-branching.md) for the future CFG design (Path B).

### D5: ExecutionResult as stage boundary

**Choice**: `ExecutionResult` is the strict handoff between execution and commitment/proving.

- Stages are independently testable: mock state for execution, synthetic data for commitment.
- Clear ownership: executor owns Stage 1, commitment owns Stage 2, proof owns Stage 3.
- **Con**: Full materialization in memory. Streaming may be needed for very large batches.

### D6: Mock-first development

**Choice**: Every pluggable component gets a mock before any real implementation.

- Full pipeline testable from day one. Tests stay fast (no trusted setup, no field arithmetic).
- Separates "is the logic correct?" from "is the crypto correct?"

### D7: Crypto-agnostic via traits

**Choice**: 8 traits in `tabula-core` cover all crypto touchpoints (see §4.3).

- STARK or SNARK backend swappable without touching executor or commitment logic.
- **Con**: More generic parameters on structs (`BatchExecutor<S, V, N>`).
- **Mitigation**: Type aliases and config structs bundle implementations for ergonomics.

### D8: Shared overlay with per-tx rollback

**Choice**: Single overlay for the entire batch. Each tx sees prior txs' writes. Failed txs rolled back.

- Matches Ethereum semantics (txs in a block see prior state).
- Simplest model — no merge conflicts, no parallel execution complexity.
- Per-tx rollback prevents one bad tx from invalidating the batch.
- **Con**: Serial execution (but proving can be parallelized via table/column sharding).

### D9: STARK (FRI) proof backend

**Choice**: Plonky3 over KoalaBear.

- Transparent setup (no trusted ceremony). Post-quantum secure. Faster prover.
- Ecosystem alignment: SP1, RISC Zero, Stwo all use STARK.
- Native LogUp for memory consistency arguments.
- **Con**: Larger proofs (~tens of KB vs hundreds of bytes for SNARK).
- **Mitigation**: STARK→Groth16 recursive wrapping (SP1 approach).
- State commitment: Hybrid SSMC + SMT ([proof-spec](../spec/proof-spec.md) §10.1). FRI = STARK backend; SMT/SSMC = state VC — separate roles.

### D10: Two-layer hash architecture

**Choice**: `Hasher` (byte-level, core) + `FieldHasher` (field-element-level, commitment).

- Executor remains field-agnostic — hashes bytes, never touches KoalaBear.
- Commitment layer operates natively on field elements for STARK efficiency.
- `PoseidonHasher` bridges both: implements `Hasher` (bytes → limbs → Poseidon → squeeze → bytes) and `FieldHasher` (native KoalaBear FE → Poseidon → NativeDigest).
- Domain separation tags prevent cross-context collisions across all hash uses.

---

## 9. Implementation Phases

### Phase 1: Reference Interpreter — COMPLETED

Deterministic execution, all crypto mocked. 191 tests at completion.

### Phase 2: Proof Foundation — COMPLETED (M1-M8)

- **M1**: IR Housekeeping (SSA, CellKey, Lookup, Select, Hash encoding)
- **M2**: 2-Slot Migration (Read/Write, Value::Null removal, budgets, statement)
- **M3**: NF Validation (NF-1~NF-4, tabula-ir extraction, canonicalization passes)
- **M4**: Plonky3 Foundation (Poseidon2, KoalaBear codec, SMT, SSMC, Hybrid VC)
- **M5**: Witness Generation (WitnessGenerator, BatchWitness, key routing)
- **M6**: AIR Foundation (chip/gadget patterns, ColumnMetaChip, debug checker)
- **M7**: Gadgets + Memory Layer (integer/memory gadgets, GlobalSortedMem, RangeCheck)
- **M8**: Execution + Hashing (ExecutionChip, PoseidonChip, GlobalSSMC, GlobalMerge)
- **M9**: LogUp Bus Wiring (8 buses, operand-to-slot linkage, Poseidon RC verification, multi-chip integration)

250 tests in tabula-proof. Zero clippy warnings.

### Phase 3: LogUp Wiring + End-to-End — IN PROGRESS

- **Post-M9**: Range-check full wiring (M10), SmtPathChip, StarkProver/Verifier, proof chaining, benchmarks
- **Optimization**: Phases 2-4 from [proof-optimization-architecture.md](./proof-optimization-architecture.md)
