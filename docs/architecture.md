# Tabula Kernel — Architecture

> **Status**: Draft v0.4.5
> **Scope**: Crate structure, core abstractions, tech stack, data flow, and implementation phasing for the Tabula Kernel.
> **Prerequisites**: [summary.md](./summary.md)

---

## Table of Contents

1. [Overview](#1-overview)
2. [Workspace Structure](#2-workspace-structure)
3. [Dependency DAG](#3-dependency-dag)
4. [Crate Specifications](#4-crate-specifications)
5. [Core Types](#5-core-types)
6. [Core Traits (Pluggable Abstractions)](#6-core-traits-pluggable-abstractions)
7. [DB-IR Instruction Set](#7-db-ir-instruction-set)
8. [Data Flow: ApplyBatch Pipeline](#8-data-flow-applybatch-pipeline)
9. [Abstraction Layers](#9-abstraction-layers)
10. [Tech Stack](#10-tech-stack)
11. [Design Decisions](#11-design-decisions)
12. [Implementation Phases](#12-implementation-phases)

---

## 1. Overview

Tabula is a **table-native zk state-transition VM kernel**. Its architecture is organized around a strict separation between:

- **Execution** (deterministic state transitions using a local overlay)
- **Commitment** (column/table commitments, batched openings/updates)
- **Proving** (end-to-end correctness proofs for `ApplyBatch`)

The project is structured as a Cargo workspace with six crates. Two central design constraints govern the architecture:

1. **The executor has zero cryptographic dependencies** — execution correctness is testable and verifiable without any proof system.
2. **All cryptographic primitives are trait-based** — the system is agnostic to the concrete proof backend (SNARK vs STARK), hash function (Blake3 vs Poseidon), signature scheme (EdDSA vs ECDSA), and PCS (KZG vs FRI). Phase 1 uses mock implementations; real backends are swapped in later.

The target proof system direction is **STARK (FRI-based)**, aligning with the performance characteristics Tabula needs (fast prover, transparent setup) and the broader zkVM ecosystem (SP1, RISC Zero, Stwo). However, the trait-based design keeps SNARK (KZG-based) as a viable alternative.

---

## 2. Workspace Structure

```
tabula/
├── Cargo.toml                       # workspace root
├── docs/
│   ├── summary.md                   # design spec
│   ├── architecture.md              # this document
│   ├── proof-spec.md                # STARK proof system constraints (AIR, LogUp, trace layout)
│   ├── semantics-spec.md            # Core IR contract, canonical state normal form, execution semantics
│   ├── dsl-philosophy.md            # DSL design philosophy
│   ├── m4-design.md                 # M4 commitment layer design
│   └── m6-air-foundation.md         # M6 AIR foundation plan
│
├── crates/
│   ├── tabula-core/                 # fundamental types, traits, errors
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs
│   │       ├── schema.rs
│   │       ├── ir.rs
│   │       ├── tx.rs
│   │       ├── state.rs
│   │       ├── event.rs
│   │       ├── error.rs
│   │       ├── traits.rs
│   │       └── mock.rs             # MockHasher, etc. (feature = "mock")
│   │
│   ├── tabula-executor/             # deterministic execution engine
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── overlay.rs
│   │       ├── interpreter.rs
│   │       ├── resolve.rs           # expression resolution (extracted from interpreter)
│   │       ├── batch.rs
│   │       ├── consistency.rs
│   │       ├── program.rs
│   │       ├── test_fixtures.rs     # shared test doubles (#[cfg(test)])
│   │       └── proptest_tests.rs    # property-based tests (#[cfg(test)])
│   │
│   ├── tabula-lang/                 # DSL compiler (lex → parse → lower)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lexer.rs
│   │       ├── parser.rs
│   │       └── lower.rs
│   │
│   ├── tabula-commitment/           # cryptographic state commitment (Plonky3 / BabyBear)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── field.rs             # NativeDigest, domain tags, u64 limb encoding
│   │       ├── codec.rs             # BabyBearCodec: value ↔ field element encoding
│   │       ├── poseidon.rs          # PoseidonHasher (Poseidon2 width-16)
│   │       ├── hasher.rs            # FieldHasher trait + MockFieldHasher
│   │       ├── smt.rs              # SparseMerkleTree
│   │       ├── ssmc.rs             # SsmcList, SsmcCommitment, MergeTrace
│   │       └── hybrid.rs           # HybridVC, ColumnMeta, CommitmentStrategy
│   │
│   ├── tabula-proof/                # proof generation & verification
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── statement.rs         # ApplyBatchStatement (no feature gate)
│   │       ├── trace.rs             # BatchWitness, ColumnWitness, AccessRow, InitRow (stark)
│   │       ├── witness.rs           # WitnessGenerator (stark)
│   │       └── air/                 # AIR constraint infrastructure (stark)
│   │           ├── mod.rs
│   │           ├── columns.rs       # zero-copy borrow (AlignedBorrow pattern)
│   │           ├── bus.rs           # LogUp bus IDs
│   │           ├── gadgets.rs       # is_real prefix gadget
│   │           ├── debug.rs         # constraint checker
│   │           └── chips/
│   │               ├── mod.rs
│   │               └── column_meta.rs  # ColumnMetaChip AIR + trace generator
│   │
│   └── tabula-cli/                  # binary entry point
│       └── src/
│           ├── main.rs              # thin dispatch
│           ├── io.rs                # JSON I/O types (ProgramFile, BatchFile, etc.)
│           └── commands/
│               ├── mod.rs
│               ├── execute.rs       # execute command handler
│               ├── inspect.rs       # inspect command handler
│               └── example.rs       # example command handler
```

---

## 3. Dependency DAG

```
              tabula-core
           /   |    |     \
  tabula-executor  |  tabula-commitment
               |   |     /
           tabula-proof
                |
  tabula-lang   |
           \    |
           tabula-cli
```

| Crate | Depends On |
|-------|-----------|
| `tabula-core` | (none — pure types/traits) |
| `tabula-executor` | `tabula-core` |
| `tabula-commitment` | `tabula-core` |
| `tabula-lang` | `tabula-core` |
| `tabula-proof` | `tabula-core`, `tabula-commitment` |
| `tabula-cli` | all of the above |

**Why this matters:**

- `tabula-executor` literally cannot import crypto crates — enforced at compile time.
- Touching executor code never triggers recompilation of the proof system.
- Each component is independently testable.

---

## 4. Crate Specifications

### 4.1 tabula-core

**Purpose**: Foundational types, trait definitions, and error types. Zero heavy dependencies. Everything else depends on this crate.

| Module | Responsibility | Key Exports |
|--------|---------------|-------------|
| `types.rs` | Primitive identifiers and values | `TableId`, `ColId`, `RowKey`, `Value`, `CellKey`, `ValueType` |
| `schema.rs` | Table/column schema definitions | `ColumnDef`, `TableSchema` |
| `ir.rs` | DB-IR instruction set | `Instruction`, `Predicate`, `Slot`, `RowExpr`, `ValueExpr` |
| `tx.rs` | Transaction model | `TxTypeId`, `TxTypeDef`, `ParamDef`, `Transaction`, `Batch` |
| `state.rs` | State root types | `StateRoot`, `TableCommitmentId`, `ColumnCommitmentId` |
| `event.rs` | Execution output types | `ExecutionEvent`, `OpKind`, `LogicalTime`, `ExecutionResult` |
| `error.rs` | Error enumeration | `TabulaError` |
| `traits.rs` | Core trait definitions | `Hasher`, `PCS`, `ValueCodec`, `StateSnapshot`, `SigVerifier`, `NoncePolicy`, `MembershipScheme`, `BatchDigester`, `StaticTableProvider` |

**Dependencies**: `serde`, `borsh`, `thiserror`

### 4.2 tabula-executor

**Purpose**: Deterministic execution engine. Runs a batch of transactions against a state snapshot using a local overlay. Produces `ExecutionResult` (ReadSet, WriteSet, events).

| Module | Responsibility |
|--------|---------------|
| `overlay.rs` | Local overlay Δ (write-buffer). Implements read-your-writes, read deduplication, write coalescing. Undo-log based checkpoint/rollback (O(1) checkpoint, O(k) rollback). |
| `interpreter.rs` | Reference interpreter for DB-IR. Slot-based variable environment. Returns `InterpreterError` on failure (wraps `TabulaError` with `instruction_index`). |
| `resolve.rs` | Expression resolution: `resolve_row_expr`, `resolve_value_expr`, `evaluate_predicate`. Extracted from interpreter for cohesion. |
| `batch.rs` | `BatchExecutor`: iterates transactions, resolves tx types from program, verifies signatures and nonces, orchestrates interpretation with per-tx rollback. Captures partial events from failed txs before rollback. |
| `consistency.rs` | Key-local RAM consistency checker. Validates that execution events satisfy last-write semantics. |
| `program.rs` | `Program` struct: holds tx type definitions and table schemas, resolves `TxTypeId` to `TxTypeDef`. |
| `test_fixtures.rs` | Shared test doubles (`#[cfg(test)]`): `TestSnapshot`, `CountingSnapshot`, `XorHasher`, `AlwaysValidSig`, `SeqNonce`, `TestStaticTables`, etc. |
| `proptest_tests.rs` | Property-based tests (`#[cfg(test)]`) for overlay semantics and consistency. |

**Dependencies**: `tabula-core`, `tracing`

**Zero crypto dependencies** — this is a hard constraint. All crypto operations (hashing, signature verification) are injected via traits.

### 4.3 tabula-commitment

**Purpose**: State commitment layer. Manages column commitments, table commitments, and the global state root. Provides the `OpeningPlan` that bridges execution output to batched proofs.

| Module | Responsibility |
|--------|---------------|
| `field.rs` | `NativeDigest([BabyBear; 8])`, domain tags (`DOMAIN_SSMC`, `DOMAIN_SMT`, `DOMAIN_LEAF`, `DOMAIN_TABLE`, `DOMAIN_COL`), u64 limb encoding (30+30+4 split). |
| `codec.rs` | `BabyBearCodec`: schema-typed value ↔ field element encoding. Per-type width: `w(Bool)=1, w(U64)=w(I64)=3, w(Digest)=8`. |
| `poseidon.rs` | `PoseidonHasher`: Poseidon2 width-16 over BabyBear. Implements `FieldHasher` (FE-level) and core `Hasher` (byte-level). |
| `hasher.rs` | `FieldHasher` trait: `hash(&[F])→Digest`, `compress(D,D)→D`, `hash_domain(tag,&[F])→D`. `MockFieldHasher` for fast unit tests. |
| `smt.rs` | `SparseMerkleTree`: depth-parameterized SMT over `FieldHasher`. Insert, prove, verify. |
| `ssmc.rs` | `SsmcList`, `SsmcCommitment`, `MergeTrace`: sorted key-value list commitment via Poseidon chain + 3-way merge proof. |
| `hybrid.rs` | `HybridVC`, `ColumnMeta`, `ColumnState`, `CommitmentStrategy`: per-column SSMC/SMT dispatch with threshold-based auto-selection. |

**Dependencies**: `tabula-core` (+ Plonky3 crates behind `stark` feature flag)

### 4.4 tabula-proof

**Purpose**: Proof generation and verification for the `ApplyBatch` statement. Wires together execution and commitment to produce end-to-end proofs.

| Module | Responsibility |
|--------|---------------|
| `statement.rs` | `ApplyBatchStatement`: public inputs struct (`oldStateRoot`, `newStateRoot`, `programRoot`, `AppliedTxDigest`, `StaticTableRoot`, `budgets`). See proof-spec §5.1 for the full definition. |
| `trace.rs` | Witness data types: `BatchWitness`, `ColumnWitness`, `AccessRow`, `InitRow`. Bridges `ExecutionResult` to proof-friendly structures. (`stark` feature) |
| `witness.rs` | `WitnessGenerator`: transforms `ExecutionResult` + `ColumnMeta` into `BatchWitness`. (`stark` feature) |
| `air/` | AIR constraint infrastructure: `columns.rs` (zero-copy borrow), `bus.rs` (LogUp bus IDs), `gadgets.rs` (`is_real` prefix), `debug.rs` (constraint checker), `chips/` (per-chip `BaseAir` + `Air` impls). (`stark` feature) |
| `air/chips/column_meta.rs` | `ColumnMetaChip`: boolean, `is_real` prefix, strict `(t,c)` ordering, untouched binding constraints. Trace generator. |

**Dependencies**: `tabula-core`, `tabula-commitment` (+ Plonky3 crates behind `stark` feature flag)

### 4.5 tabula-lang

**Purpose**: DSL compiler. Compiles a human-readable program definition into `CompiledProgram` (tx type defs + table schemas). Hand-rolled lexer, recursive-descent parser with Pratt expression parsing, zero parser dependencies.

**Pipeline**: `compile(&str) → Result<CompiledProgram>` — lex → parse → lower.

**Dependencies**: `tabula-core`, `thiserror`

### 4.6 tabula-cli

**Purpose**: Binary entry point. Thin dispatch in `main.rs`; command handlers in `commands/` module. JSON I/O types in `io.rs` (`ProgramFile`, `BatchFile`, `OutputFile`).

| Module | Responsibility |
|--------|---------------|
| `main.rs` | CLI arg parsing, dispatch to command handlers. |
| `io.rs` | JSON-serializable types for program/batch/output files. |
| `commands/execute.rs` | Execute a batch against a program, output results. |
| `commands/inspect.rs` | Inspect a program file (list tx types, schemas). |
| `commands/example.rs` | Generate an example program/batch for demos. |

**Dependencies**: all crates + `clap`, `anyhow`, `tracing-subscriber`, `serde_json`

---

## 5. Core Types

### 5.1 Primitive Identifiers

```rust
/// Identifies a table in the state.
pub struct TableId(pub u32);

/// Identifies a column within a table.
pub struct ColId(pub u16);

/// Row key. Dense integer keys for kernel v1.0.
pub struct RowKey(pub u64);

/// A fully-qualified cell address.
pub struct CellKey {
    pub table: TableId,
    pub row: RowKey,
    pub col: ColId,
}
```

> **Canonical order.** The protocol-level canonical ordering for `CellKey` is **`(t, c, r)`** (table, column, row). All sorting (GlobalSortedMem), hashing (domain tags, Poseidon sponge), and Merkle leaf encoding use this canonical order. The Rust struct field order (`table, row, col`) is an implementation detail — serialization and comparison MUST use `(t, c, r)`.

### 5.2 Value Type

```rust
/// A typed value stored in a table cell.
/// Application-level only — field element encoding is handled by ValueCodec.
pub enum Value {
    U64(u64),
    I64(i64),
    Bool(bool),
    Bytes32([u8; 32]),
}

/// Describes the type of a column or parameter.
pub enum ValueType {
    U64,
    I64,
    Bool,
    Bytes32,
}
```

`Value` is a concrete enum representing **application-level** types only. There is no `Null` variant — absence is modeled as a separate `val_is_null: bool` flag in `ExecutionEvent` and as `Option<Value>` in `ExecutionResult` collections (see §5.4). This matches the normative two-slot `Read/Write` design in [semantics-spec.md](./semantics-spec.md) §1.5. Field element encoding is a PCS-layer concern handled by the `ValueCodec` trait (see Section 6.5).

### 5.3 Transaction Model

```rust
/// Unique identifier for a transaction type.
pub struct TxTypeId(pub u32);

/// A transaction type definition (part of the program).
pub struct TxTypeDef {
    pub id: TxTypeId,
    pub name: String,
    pub param_schema: Vec<ParamDef>,
    pub body: Vec<Instruction>,   // DB-IR body
}

/// Describes a parameter of a transaction type.
pub struct ParamDef {
    pub name: String,
    pub value_type: ValueType,
}

/// A concrete transaction in a batch.
pub struct Transaction {
    pub tx_type: TxTypeId,
    pub params: Vec<Value>,
    pub sender: [u8; 32],
    pub nonce: u64,
    pub signature: Vec<u8>,
}

/// An ordered batch of transactions.
pub struct Batch {
    pub transactions: Vec<Transaction>,
}
```

### 5.4 Execution Output

```rust
pub type LogicalTime = u64;

pub enum OpKind { Read, Write }

/// A single execution event for the consistency module.
pub struct ExecutionEvent {
    pub key: CellKey,
    pub op: OpKind,
    pub value: Value,            // canonical zero when absent
    pub val_is_null: bool,       // true = cell is absent
    pub time: LogicalTime,
    pub tx_index: u32,           // index of the transaction within the batch
}

/// Per-transaction execution outcome.
pub enum TxOutcome {
    /// Transaction executed successfully.
    Success,
    /// Transaction failed (e.g., ASSERT failure). All its state changes were rolled back.
    Failed {
        reason: String,
        partial_events: Vec<ExecutionEvent>,  // trace up to the failure point
        failed_instruction: Option<usize>,    // instruction index that failed (None for pre-checks)
    },
}

/// The output of deterministic batch execution.
/// This is the handoff point between Stage 1 (execution) and Stage 2 (commitment).
pub struct ExecutionResult {
    /// Cells read from oldStateRoot (not from overlay). Deduplicated.
    /// `None` = cell was absent.
    pub read_set_old: Vec<(CellKey, Option<Value>)>,
    /// Final writes to apply to committed state. Coalesced across the entire batch (last-write-wins).
    /// `None` = delete (write null).
    /// This is `WriteSet_batch_final` in proof-spec terminology (§8.6).
    /// Within a single tx, NF-2 guarantees at most one Write per key, so no intra-tx coalescing occurs.
    pub write_set_final: Vec<(CellKey, Option<Value>)>,
    /// Full execution trace for consistency proving.
    pub events: Vec<ExecutionEvent>,
    /// Emitted application events / receipts.
    pub emitted: Vec<EmittedEvent>,
    /// Per-transaction outcomes (success/failure).
    pub tx_outcomes: Vec<TxOutcome>,
}
```

> **Proof interface note**: `ExecutionResult` is the **logical** handoff between execution and proving. The proof system consumes it and constructs a separate **trace layout** (instruction columns, slot columns, access columns, GlobalSortedMem — see [proof-spec.md](./proof-spec.md) §6). The trace is not a 1:1 reflection of `ExecutionResult`; it is a constraint-friendly representation derived from it. Future phases may add a `TraceBuilder` that transforms `ExecutionResult` into the AIR trace.

```rust
// Conceptual future interface (not yet implemented):
// let result: ExecutionResult = batch_executor.execute(...)?;
// let trace: AirTrace = TraceBuilder::new(&result, &program).build()?;
// let proof = prover.prove(&statement, &trace)?;
```

---

## 6. Core Traits (Pluggable Abstractions)

All traits live in `tabula-core/src/traits.rs`. They are the primary mechanism for keeping the system crypto-agnostic. Phase 1 provides mock implementations for every trait; real backends are swapped in later phases.

### Trait Map

```
                    tabula-core traits
  ┌─────────────────────────────────────────────────┐
  │                                                 │
  │  Crypto Primitives        Execution Policies    │
  │  ─────────────────        ──────────────────    │
  │  Hasher                   SigVerifier           │
  │  PCS + ColumnCommitment   NoncePolicy           │
  │  ValueCodec               StaticTableProvider   │
  │  MembershipScheme                               │
  │  BatchDigester            State Access           │
  │                           ──────────────         │
  │                           StateSnapshot          │
  │                                                 │
  └─────────────────────────────────────────────────┘

  Usage by crate:
  ┌──────────────────────────────────────────────────────┐
  │ tabula-executor:                                     │
  │   StateSnapshot, SigVerifier, NoncePolicy,           │
  │   StaticTableProvider, Hasher (for IR HASH instr)    │
  │                                                      │
  │ tabula-commitment:                                   │
  │   PCS, ValueCodec, Hasher, ColumnCommitment,         │
  │   MembershipScheme, BatchDigester                    │
  │                                                      │
  │ tabula-proof:                                        │
  │   All traits                                         │
  └──────────────────────────────────────────────────────┘
```

### 6.1 Hasher

```rust
pub type Digest = [u8; 32];

/// Cryptographic hash function abstraction.
/// Out-of-circuit: Blake3. In-circuit: Poseidon or other SNARK/STARK-friendly hash.
pub trait Hasher: Send + Sync {
    fn hash(&self, data: &[u8]) -> Digest;
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest;
    fn hash_many(&self, items: &[&[u8]]) -> Digest;
}
```

Used for: `stateRoot`, `tableCom`, `programRoot`, `AppliedTxDigest`, `schemaHash`, and the IR `Hash` instruction.

**v1.0**: `Blake3Hasher`.
**Future**: Poseidon (STARK-friendly, in-circuit).

### 6.2 PCS (Polynomial / Vector Commitment Scheme)

The central pluggable abstraction of the entire system.

```rust
pub trait ColumnCommitment: Clone + Send + Sync + fmt::Debug {
    fn to_bytes(&self) -> Vec<u8>;
}

/// Polynomial / Vector Commitment Scheme interface.
pub trait PCS: Send + Sync {
    type Commitment: ColumnCommitment;
    type OpenProof: Clone + Send + Sync;
    type UpdateProof: Clone + Send + Sync;
    type Codec: ValueCodec;

    /// Access the value codec used by this PCS.
    fn codec(&self) -> &Self::Codec;

    /// Commit to a column vector.
    fn commit(&self, values: &[Value]) -> Result<Self::Commitment, TabulaError>;

    /// Open a single position.
    fn open(
        &self,
        commitment: &Self::Commitment,
        values: &[Value],
        row: RowKey,
    ) -> Result<(Value, Self::OpenProof), TabulaError>;

    /// Verify a single opening.
    fn verify_open(
        &self,
        commitment: &Self::Commitment,
        row: RowKey,
        value: &Value,
        proof: &Self::OpenProof,
    ) -> Result<bool, TabulaError>;

    /// Batch open: multiple rows from one column.
    fn batch_open(
        &self,
        commitment: &Self::Commitment,
        values: &[Value],
        rows: &[RowKey],
    ) -> Result<(Vec<Value>, Self::OpenProof), TabulaError>;

    /// Update a commitment after changing one cell.
    fn update(
        &self,
        commitment: &Self::Commitment,
        row: RowKey,
        old_value: &Value,
        new_value: &Value,
    ) -> Result<(Self::Commitment, Self::UpdateProof), TabulaError>;
}
```

**v1.0**: `MockPCS` (hash of borsh-serialized values, empty proofs).
**Future**: FRI-based (Plonky3) or KZG (`ark-poly-commit`).

### 6.3 StateSnapshot

```rust
/// Read-only access to the committed state (snapshot).
/// The executor uses this to resolve reads that miss the overlay.
pub trait StateSnapshot: Send + Sync {
    fn read(&self, key: &CellKey) -> Result<Value, TabulaError>;
    fn table_exists(&self, table: TableId) -> bool;
}
```

**v1.0**: `InMemoryState` (BTreeMap-backed).
**Future**: Persistent storage (RocksDB, SQLite).

### 6.4 SigVerifier

```rust
/// Signature verification abstraction.
/// In-circuit cost varies dramatically by scheme (ECDSA vs EdDSA vs Schnorr).
/// The executor must not know which scheme is used.
pub trait SigVerifier: Send + Sync {
    fn verify(
        &self,
        sender: &[u8; 32],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), TabulaError>;
}
```

**v1.0**: `MockSigVerifier` — always returns `Ok(())`.
**Future**: EdDSA-over-BabyBear (STARK-friendly), or ECDSA/Ed25519 with precompile.

### 6.5 ValueCodec

```rust
/// Encodes/decodes application-level Values to/from the field elements used by the PCS.
///
/// Why this exists:
/// - BN254 scalar field: ~254 bits → one Value::U64 fits in one field element
/// - Goldilocks: 64 bits → one Value::U64 fits in one field element
/// - BabyBear: 31 bits → one Value::U64 requires 3 field elements (limb decomposition)
///
/// The executor never sees field elements. This trait lives at the PCS boundary.
pub trait ValueCodec: Send + Sync {
    type FieldRepr: Clone + Send + Sync;

    /// Encode a Value into field elements.
    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError>;

    /// Decode field elements back into a Value.
    fn decode(
        &self,
        field_elements: &[Self::FieldRepr],
        target_type: ValueType,
    ) -> Result<Value, TabulaError>;

    /// How many field elements a given ValueType requires.
    fn field_elements_per(&self, value_type: ValueType) -> usize;
}
```

**v1.0**: `MockValueCodec` — borsh-serializes `Value` into `Vec<u8>` as the "field representation".
**Future**: Schema-Typed Digest-Native encoding for BabyBear — per-type width `w(Bool)=1, w(U64)=w(I64)=3, w(Digest)=8` field elements. See proof-spec §10.3.

### 6.6 NoncePolicy

```rust
/// Replay protection policy abstraction.
pub trait NoncePolicy: Send + Sync {
    /// Validate that a transaction's nonce is acceptable given the sender's current nonce.
    fn validate(
        &self,
        sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError>;

    /// Compute the next nonce after a successful transaction.
    fn next_nonce(&self, sender: &[u8; 32], current_nonce: u64) -> u64;
}
```

**v1.0**: `SequentialNonce` — `tx_nonce == current_nonce`, next = `current + 1`.
**Future**: Epoch-based, bitfield-based, or application-defined policies.

### 6.7 MembershipScheme

```rust
/// Proves that a tx type is a member of the committed program (programRoot).
/// The proof mechanism varies: Merkle tree, vector commitment opening, direct list, etc.
pub trait MembershipScheme: Send + Sync {
    type Proof: Clone + Send + Sync;

    /// Compute programRoot from a set of tx type definitions.
    fn compute_root(&self, tx_types: &[TxTypeDef]) -> Result<Digest, TabulaError>;

    /// Generate a membership proof for a specific tx type.
    fn prove(
        &self,
        tx_types: &[TxTypeDef],
        index: usize,
    ) -> Result<Self::Proof, TabulaError>;

    /// Verify a membership proof.
    fn verify(
        &self,
        root: &Digest,
        tx_type: &TxTypeDef,
        proof: &Self::Proof,
    ) -> Result<bool, TabulaError>;
}
```

**v1.0**: `FlatHashMembership` — concatenate all tx type hashes, hash the result. Proof is the full list (brute-force verification).
**Future**: Merkle tree membership proofs (log-sized proofs).

### 6.8 BatchDigester

```rust
/// Computes AppliedTxDigest from a Batch.
/// Separates serialization rules from the hash function.
pub trait BatchDigester: Send + Sync {
    fn digest(&self, batch: &Batch) -> Result<Digest, TabulaError>;
}
```

**v1.0**: `SimpleBatchDigester` — borsh-serialize, then Blake3 hash.
**Future**: Merkle tree of transactions (enables per-tx inclusion proofs).

### 6.9 StaticTableProvider

```rust
/// Provides read-only access to static (fixed) tables.
/// Used by the LOOKUP instruction for range checks, byte decomposition, enum sets, etc.
/// In the proof system, these map to lookup arguments (LogUp/Lasso).
pub trait StaticTableProvider: Send + Sync {
    fn lookup(
        &self,
        table: TableId,
        key: RowKey,
        col: ColId,
    ) -> Result<Value, TabulaError>;

    fn contains(&self, table: TableId, key: RowKey) -> Result<bool, TabulaError>;
}
```

**v1.0**: `InMemoryStaticTables` — `BTreeMap`-backed fixed data.
**Future**: Connected to LogUp/Lasso lookup arguments in the proof system.

---

## 7. DB-IR Instruction Set

> **Normative definition**: [semantics-spec.md](./semantics-spec.md) §1 (Core IR Contract). This section describes the Rust implementation; the semantics-spec is authoritative for IR semantics, normal-form rules, and type contracts.

Tabula uses a **slot-based flat IR** with **True SSA** semantics — not an AST, not a bytecode VM. Each destination slot is assigned at most once (validated at registration time), and each instruction maps to a single operation, making it trivially interpretable and directly traceable for proving (1 instruction = 1 constraint group). The SSA property eliminates the need for intra-tx memory arguments.

> **Note**: The Rust IR uses two-slot `Read { dst_val, dst_is_null }` and `Write { src_val, src_is_null }`, matching the normative semantics-spec. `Value::Null` has been removed; absence is a separate `Bool` flag. See [semantics-spec.md](./semantics-spec.md) §1.5.

### 7.1 Slot Environment

Each transaction execution maintains a `Vec<Value>` indexed by `Slot`. Instructions read inputs from and write outputs to slots.

```rust
/// Slot index for local variables within a tx execution.
pub type Slot = u16;
```

### 7.2 Expression Types

```rust
/// Where a row key comes from.
pub enum RowExpr {
    Literal(RowKey),     // hardcoded row key
    Slot(Slot),          // cast slot value to RowKey
    Param(u16),          // tx parameter index, cast to RowKey
}

/// Where a value comes from.
pub enum ValueExpr {
    Literal(Value),      // hardcoded value
    Slot(Slot),          // reference to a local variable
    Param(u16),          // tx parameter index
}
```

### 7.3 Instructions

```rust
pub enum Instruction {
    // ── State Operations ──────────────────────────────────

    /// Read a cell from state, store value in `dst_val` and null flag in `dst_is_null`.
    Read {
        dst_val: Slot,
        dst_is_null: Slot,
        table: TableId,
        col: ColId,
        row: RowExpr,
    },

    /// Write a value to a cell in state.
    Write {
        table: TableId,
        col: ColId,
        row: RowExpr,
        src_val: ValueExpr,
        src_is_null: ValueExpr,
    },

    /// Lookup in a static (fixed) table, store result in `dst`.
    Lookup {
        dst: Slot,
        static_table: TableId,
        key: RowExpr,
        col: ColId,
    },

    // ── Arithmetic Operations ─────────────────────────────

    /// dst = lhs + rhs
    Add {
        dst: Slot,
        lhs: ValueExpr,
        rhs: ValueExpr,
    },

    /// dst = lhs - rhs
    Sub {
        dst: Slot,
        lhs: ValueExpr,
        rhs: ValueExpr,
    },

    /// dst = lhs * rhs
    Mul {
        dst: Slot,
        lhs: ValueExpr,
        rhs: ValueExpr,
    },

    /// dst_q = lhs / rhs, dst_r = lhs % rhs
    DivMod {
        dst_q: Slot,
        dst_r: Slot,
        lhs: ValueExpr,
        rhs: ValueExpr,
    },

    // ── Control & Output ──────────────────────────────────

    /// Assert a predicate. Execution of this tx fails if false.
    Assert {
        predicate: Predicate,
    },

    /// Hash inputs, store result in `dst`.
    Hash {
        dst: Slot,
        inputs: Vec<ValueExpr>,
    },

    /// Emit an event (for receipts).
    Emit {
        topic: Vec<u8>,
        data: Vec<ValueExpr>,
    },
}
```

### 7.4 Predicates

```rust
pub enum Predicate {
    Eq(ValueExpr, ValueExpr),
    Lt(ValueExpr, ValueExpr),
    Lte(ValueExpr, ValueExpr),
    Gt(ValueExpr, ValueExpr),
    Gte(ValueExpr, ValueExpr),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}
```

### 7.5 Example: Token Transfer

```
// tx type: transfer(from: RowKey, to: RowKey, amount: U64)
// params: [from=Param(0), to=Param(1), amount=Param(2)]

READ    s0 ← Balances.balance[Param(0)]    // sender balance
READ    s1 ← Balances.balance[Param(1)]    // receiver balance
ASSERT  Gte(Slot(s0), Param(2))            // sender has enough
SUB     s2 ← Slot(s0), Param(2)           // new sender balance
ADD     s3 ← Slot(s1), Param(2)           // new receiver balance
WRITE   Balances.balance[Param(0)] ← Slot(s2)
WRITE   Balances.balance[Param(1)] ← Slot(s3)
```

---

## 8. Data Flow: ApplyBatch Pipeline

> **Naming convention.** This document uses **Stage 1/2/3** for the pipeline stages (Execution → Commitment → Proving). Proof-spec uses **Layer B/C** for proof scope (intra-tx core → inter-tx batch). These are orthogonal taxonomies — do not confuse them.

### 8.1 High-Level Pipeline

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
       │  StateSnapshot (read-only)   │
       │  SigVerifier                 │
       │  NoncePolicy                 │
       │  StaticTableProvider         │
       │  Hasher (for IR Hash instr)  │
       │        +                     │
       │  Overlay Δ (write-buffer)    │
       │        +                     │
       │  Interpreter (walks IR)      │
       │                              │
       │  Per-tx: checkpoint before,  │
       │  rollback on ASSERT failure  │
       │                              │
       │  Outputs:                    │
       │   • ReadSet_old              │
       │   • WriteSet_batch_final     │
       │   • ExecutionEvents[]        │
       │   • EmittedEvents[]          │
       │   • TxOutcomes[]             │
       └──────────────┬──────────────┘
                      │
                ExecutionResult
                      │
       ┌──────────────▼──────────────┐
       │    STAGE 2: COMMITMENT       │  ← tabula-commitment
       │                              │
       │  PCS + ValueCodec            │
       │  Hasher                      │
       │  MembershipScheme            │
       │  BatchDigester               │
       │                              │
       │  1. Build OpeningPlan        │
       │     group ReadSet by         │
       │     (tableId, colId)         │
       │                              │
       │  2. BatchOpen per column     │
       │     → opening proofs         │
       │                              │
       │  3. Apply WriteSet_batch_final│
       │     → update proofs          │
       │     → new column commitments │
       │                              │
       │  4. Recompute tableComs      │
       │     → H(colComs...)          │
       │                              │
       │  5. Compute newStateRoot     │
       │     → H(tableComs...)        │
       └──────────────┬──────────────┘
                      │
       ┌──────────────▼──────────────┐
       │    STAGE 3: PROVING          │  ← tabula-proof
       │                              │
       │  Prove:                      │
       │   • Batch binds to digest    │
       │   • tx types ∈ programRoot   │
       │   • Signatures valid         │
       │   • Execution correct        │
       │   • Key-local consistency    │
       │   • oldRoot → newRoot        │
       │                              │
       │  Output:                     │
       │   • newStateRoot             │
       │   • proof                    │
       │   • receiptsDigest (*)       │
       └─────────────────────────────┘
```

> (*) **`receiptsDigest`** is currently **out-of-protocol**: `Emit` events are not committed in `AppliedTxDigest` and not verified by the proof system ([semantics-spec](./semantics-spec.md) §1.5.3). `receiptsDigest` is a convenience output for debugging and UX — it is **not** part of the `ApplyBatchStatement` public inputs. If events become protocol-relevant in the future (e.g., committed in a receipt trie), this will require explicit spec changes.

### 8.2 Stage 1 Detail: Overlay Semantics

The `Overlay` sits on top of a `StateSnapshot` and provides two **distinct** roles:

**Inter-tx role (required for batch semantics):** Transaction `i+1` must see transaction `i`'s writes. The overlay's write buffer accumulates writes across the batch, making prior txs' state changes visible to later txs. This is essential and cannot be removed — see [semantics-spec.md](./semantics-spec.md) §2.2 (Batch Semantics).

**Intra-tx role (convenience, not required for correctness):** Within a single transaction, the IR normal form ([semantics-spec.md](./semantics-spec.md) §2.3, rules NF-1 through NF-4) guarantees unique-read, unique-write, and no-read-after-write as structural invariants of the IR. The overlay's read-cache and read-your-writes behavior is therefore redundant for intra-tx correctness — a valid NF program will never trigger read-cache hits or re-read its own writes within one tx. The proof system exploits this: no intra-tx RAM-consistency argument is needed (see [proof-spec.md](./proof-spec.md) §10.6).

The overlay enforces three rules:

| Rule | Name | Behavior | Scope |
|------|------|----------|-------|
| A | Read-Your-Writes | If key `k` was written earlier in the batch, `READ(k)` returns the value from the overlay, **not** from committed state. | Inter-tx (prior txs' writes) |
| B | Read Deduplication | If key `k` was read from committed state before, reuse the cached value. One opening per unique key. | Inter-tx + intra-tx (convenience) |
| C | Write Coalescing | If key `k` is written multiple times across txs, only the **last** write appears in `WriteSet_batch_final`. Intermediate values stay local. | Inter-tx (across txs; intra-tx write uniqueness is guaranteed by NF-2) |

```
READ(k):
  1. Check write buffer (Rule A) → hit? return value
  2. Check read cache (Rule B)   → hit? return value
  3. Read from StateSnapshot     → cache in read_cache, return value

WRITE(k, v):
  1. Insert/overwrite in write buffer (Rule C)

Finalize:
  read_set_old    = read_cache contents
  write_set_final = write buffer contents
```

### 8.3 Per-Transaction Rollback

Failed transactions (ASSERT failure) must not pollute the batch state.

```
For each tx in batch:
  1. Checkpoint overlay state (O(1) — records undo-log position)
  2. Execute tx
  3. If ASSERT fails:
     a. Capture partial events (trace up to failure point)
     b. Rollback overlay (O(k) — replays undo-log in reverse)
     c. Record TxOutcome::Failed { reason, partial_events, failed_instruction }
     d. Continue to next tx
  4. If success:
     a. Discard checkpoint
     b. Record TxOutcome::Success
```

This matches Ethereum semantics: a failed tx is skipped, successful txs persist. One bad tx does not invalidate an entire batch.

### 8.4 Stage 2 Detail: Opening Plan

The `OpeningPlan` reorganizes `ReadSet_old` for efficient batched proofs:

```
ReadSet_old:
  [(Entity/0/hp, 100), (Entity/0/atk, 50), (Entity/1/hp, 80), ...]

  ↓ group by (tableId, colId)

OpeningPlan:
  Group 1: Entity.hp  → rows [0, 1, ...] with values [100, 80, ...]
  Group 2: Entity.atk → rows [0, ...]     with values [50, ...]

  → one BatchOpen call per group
  → proofs scale with #groups, not #READs
```

---

## 9. Abstraction Layers

### 9.1 Pluggable (trait-based, swappable implementations)

| Component | Trait | v1.0 Mock | Future |
|-----------|-------|-----------|--------|
| Hash function | `Hasher` | `Blake3Hasher` | Poseidon (STARK-friendly, in-circuit) |
| Vector commitment | `PCS` | `MockPCS` (hash-based, no ZK) | FRI/Plonky3 (STARK), KZG (SNARK) |
| Value encoding | `ValueCodec` | `MockValueCodec` (borsh bytes) | Schema-Typed Digest-Native: `w(Bool)=1, w(U64/I64)=3, w(Digest)=8` BabyBear FE |
| State storage | `StateSnapshot` | `InMemoryState` (BTreeMap) | RocksDB, SQLite |
| Signature verification | `SigVerifier` | `MockSigVerifier` (always true) | EdDSA-over-BabyBear, ECDSA |
| Replay protection | `NoncePolicy` | `SequentialNonce` | Epoch-based, bitfield |
| Program membership | `MembershipScheme` | `FlatHashMembership` | Merkle tree proofs |
| Batch hashing | `BatchDigester` | `SimpleBatchDigester` | Merkle tree of txs |
| Static tables | `StaticTableProvider` | `InMemoryStaticTables` | LogUp/Lasso-linked |
| Prover | `Prover` / `Verifier` | `MockProver` (accept-all) | STARK circuit prover |

### 9.2 Concrete (stable, versioned — changes require explicit spec update)

| Component | Rationale |
|-----------|-----------|
| DB-IR instruction set (`Instruction` enum) | Core language. Adding an opcode is a deliberate, versioned change. |
| `CellKey` / `Value` / `ValueType` types | Fundamental data types shared across all crates. |
| `ExecutionResult` / `ExecutionEvent` | Stage 1 → Stage 2 handoff format. Must be stable. |
| `OpeningPlan` / `ColumnOpenGroup` | Stage 2 internal structure. Encodes a specific optimization strategy. |
| `ApplyBatchStatement` | Public inputs for the proof. Must match the spec exactly. |
| `Digest = [u8; 32]` | 256-bit hash output. Standard across both SNARK and STARK. |

---

## 10. Tech Stack

### 10.1 Rust Crate Dependencies

| Concern | Crate | Version | Rationale |
|---------|-------|---------|-----------|
| Deterministic serialization | `borsh` | 1.x | Canonical binary encoding for commitment inputs. Guarantees byte-level determinism. |
| Human-readable serialization | `serde` | 1.x | JSON for configs, debugging, test fixtures. |
| Hashing (out-of-circuit) | `blake3` | 1.x | Fast, 256-bit, well-audited. Used by mock implementations. |
| Error handling (libraries) | `thiserror` | 2.x | Derive `Error` for typed error enums. |
| Error handling (binary) | `anyhow` | 1.x | Ergonomic top-level error reporting in CLI. |
| Structured logging | `tracing` | 0.1.x | Span-based structured logging. Execution trace inspection. |
| CLI argument parsing | `clap` | 4.x | Standard CLI framework. |
| Property-based testing | `proptest` | 1.x | Critical for overlay/consistency logic correctness. |
| Collections | `std` BTreeMap/BTreeSet | — | **Deterministic iteration order. No HashMap in executor.** |

**Phase 2+ (behind feature flags):**

| Concern | Crate | Rationale |
|---------|-------|-----------|
| STARK proof system | `p3-uni-stark`, `p3-fri` | **Confirmed**: Plonky3 — production-ready, audited, SP1-validated. |
| Field arithmetic | `p3-field`, `p3-baby-bear` | **Confirmed**: BabyBear (p = 2^31 − 2^27 + 1 = 2013265921). Fast native arithmetic. |
| Hash (in-circuit) | `p3-poseidon2` | Poseidon2 over BabyBear for SSMC sponge commitment, SMT internal nodes, and IR Hash instruction. |
| LogUp | `p3-air` (built-in) | Native LogUp support for memory consistency arguments. |
| SNARK wrapping | TBD (SP1 wrapper or custom Groth16) | Optional: STARK→SNARK for on-chain verification (~200 bytes). |

### 10.2 Dependencies by Crate

```
tabula-core:
  serde, borsh, thiserror
  (no blake3 — Hasher is a trait, not a concrete dependency)

tabula-executor:
  tabula-core, tracing
  (zero crypto dependencies — all crypto injected via traits)

tabula-lang:
  tabula-core, thiserror
  (zero parser dependencies — hand-rolled lexer + recursive descent)

tabula-commitment:
  tabula-core
  [feature = "stark"] p3-field, p3-baby-bear, p3-poseidon2, p3-symmetric

tabula-proof:
  tabula-core, tabula-commitment
  [feature = "stark"] p3-field, p3-baby-bear, p3-air, p3-matrix

tabula-cli:
  all of the above, clap, anyhow, tracing-subscriber
```

### 10.3 Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/tabula-core",
    "crates/tabula-executor",
    "crates/tabula-commitment",
    "crates/tabula-lang",
    "crates/tabula-proof",
    "crates/tabula-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
# Internal
tabula-core       = { path = "crates/tabula-core" }
tabula-executor   = { path = "crates/tabula-executor" }
tabula-commitment = { path = "crates/tabula-commitment" }
tabula-lang       = { path = "crates/tabula-lang" }
tabula-proof      = { path = "crates/tabula-proof" }

# Serialization
serde  = { version = "1", features = ["derive"] }
borsh  = { version = "1", features = ["derive"] }

# Crypto (mock implementations)
blake3 = "1"

# Error handling
thiserror = "2"
anyhow    = "1"

# Logging
tracing            = "0.1"
tracing-subscriber = "0.3"

# CLI
clap = { version = "4", features = ["derive"] }

# Testing
proptest = "1"
```

---

## 11. Design Decisions

### D1: Multi-crate workspace over single crate

**Choice**: Cargo workspace with 5 crates.

- **Pro**: Dependency boundaries enforced at compile time. `tabula-executor` cannot accidentally import crypto crates.
- **Pro**: Independent compilation — proof crate changes don't rebuild executor.
- **Pro**: Clear ownership boundaries for code review and testing.
- **Con**: More boilerplate (multiple `Cargo.toml`, `pub use` re-exports).

The system has clear enough boundaries that the overhead is justified.

### D2: BTreeMap everywhere in executor

**Choice**: `BTreeMap` / `BTreeSet` exclusively. No `HashMap` in `tabula-executor`.

- **Pro**: Deterministic iteration order. Non-negotiable for reproducible execution.
- **Con**: O(log n) vs O(1) lookups.
- **Why it's fine**: Touch set per batch is typically thousands of keys, not millions. The log factor is negligible compared to proving cost.

### D3: Concrete `Value` enum, no `Field` variant

**Choice**: `Value` is a concrete enum with application-level types only (`U64`, `I64`, `Bool`, `Bytes32`, `Null`). No `Field([u8; 32])` variant.

- **Pro**: No assumptions about field size leak into the executor. BabyBear (31-bit), Goldilocks (64-bit), and BN254 (254-bit) are all supported via `ValueCodec`.
- **Pro**: Debuggable, no generic parameter pollution.
- **Pro**: Clean separation — application types live in `Value`, field encoding lives in `ValueCodec`.
- **Con**: Conversion overhead at the PCS boundary.
- **Why it's fine**: Conversion cost is negligible compared to commitment/proof operations.

### D4: Slot-based flat IR with arithmetic (True SSA)

**Choice**: Flat `Vec<Instruction>` with `Slot` indices for data flow, plus `Add`/`Sub`/`Mul`/`DivMod` arithmetic instructions. **True SSA**: each destination slot is assigned at most once across the entire instruction body.

- **Pro**: 1 instruction = 1 operation = 1 constraint group. Direct mapping to proof system.
- **Pro**: Slot indices provide explicit data flow tracking.
- **Pro**: True SSA means slots are **wires** in the constraint system — no register-file propagation, no intra-tx memory argument needed for local variables.
- **Pro**: Arithmetic instructions enable state-mutating computations (e.g., `hp = hp - atk`).
- **Con**: No structured control flow (loops, branches) in v1.0.
- **Mitigation**: Transaction logic in v1.0 is straight-line code. For DAG-CFG, MLIR-style block parameters preserve SSA at join points.
- **Enforcement**: `Program::register()` validates the SSA invariant and rejects programs with duplicate destination slots.

### D5: Stage 1 / Stage 2 separation via `ExecutionResult`

**Choice**: `ExecutionResult` struct is the strict handoff point between execution and commitment.

- **Pro**: Stages are independently testable. Can test execution with mock state, and commitment with synthetic ReadSet/WriteSet.
- **Pro**: Clear ownership — executor owns Stage 1, commitment owns Stage 2.
- **Con**: Materializing the full `ExecutionResult` in memory. For very large batches, streaming may be needed.

### D6: Mock-first development

**Choice**: Every pluggable component gets a mock implementation before any real one.

- **Pro**: Full pipeline testable from day one without any crypto setup.
- **Pro**: Tests remain fast (no trusted setup, no field arithmetic).
- **Pro**: Separates "is the logic correct?" from "is the crypto correct?"

### D7: Crypto-agnostic via 10 traits

**Choice**: All cryptographic and policy decisions are abstracted behind traits. The executor and commitment layers are parameterized, not hardcoded.

10 traits cover all crypto touchpoints:

| Touchpoint | Trait |
|------------|-------|
| Hash function | `Hasher` |
| Vector commitment | `PCS` + `ColumnCommitment` |
| Value↔field encoding | `ValueCodec` |
| State storage | `StateSnapshot` |
| Signature verification | `SigVerifier` |
| Replay protection | `NoncePolicy` |
| Program membership | `MembershipScheme` |
| Batch digest | `BatchDigester` |
| Static table access | `StaticTableProvider` |

- **Pro**: STARK or SNARK backend can be swapped without touching executor or commitment logic.
- **Pro**: Phase 1 is fully functional with mock implementations.
- **Con**: More generic parameters on structs (e.g., `BatchExecutor<S, V, N>` where S: StateSnapshot, V: SigVerifier, N: NoncePolicy).
- **Mitigation**: Use type aliases or config structs to bundle trait implementations for ergonomics.

### D8: Shared overlay with per-tx rollback

**Choice**: Single overlay for the entire batch. Each tx sees prior txs' writes. Failed txs are rolled back.

- **Pro**: Matches Ethereum semantics (txs in a block see prior txs' state).
- **Pro**: Simplest model — no merge conflicts, no parallel execution complexity.
- **Pro**: Spec-aligned — "deterministic execution of B over S_old under a fixed ordering policy".
- **Pro**: Per-tx rollback prevents one bad tx from invalidating an entire batch.
- **Con**: Serial execution (but proving can still be parallelized via table/column sharding).

### D9: STARK (FRI) as proof backend — CONFIRMED

**Choice**: **Plonky3 over BabyBear** (p = 2^31 − 2^27 + 1 = 2013265921).

- **Pro**: Transparent setup (no trusted ceremony).
- **Pro**: Post-quantum secure.
- **Pro**: Faster prover — critical for Tabula's batch proving workload.
- **Pro**: Ecosystem alignment — SP1, RISC Zero, Stwo all use STARK. SP1 validates Plonky3 in production.
- **Pro**: Native LogUp support for memory consistency arguments.
- **Con**: Larger proofs (~tens of KB vs hundreds of bytes for SNARK).
- **Mitigation**: STARK→Groth16 recursive wrapping for on-chain verification (SP1 approach).
- **State commitment**: Hybrid SSMC + SMT (see [proof-spec.md](./proof-spec.md) v0.9 §10.1). FRI is the STARK backend; SMT/SSMC is the state VC — these are separate roles. Integer encoding for u64 keys/timestamps in BabyBear is specified in §4.2.R.

---

## 12. Implementation Phases

### Phase 1: Reference Interpreter (no cryptography) — COMPLETED

**Goal**: Execute batches deterministically, produce correct `ExecutionResult`. All crypto is mocked.

**Deliverables**:

1. `tabula-core` — all types, all 10 traits, error types
2. `tabula-executor` — Overlay (with checkpoint/rollback), Interpreter (with arithmetic), BatchExecutor, consistency checker
3. `tabula-commitment` — all mock implementations (MockPCS, MockValueCodec, MockHasher, MockSigVerifier, SequentialNonce, FlatHashMembership, SimpleBatchDigester, InMemoryStaticTables)
4. `tabula-lang` — DSL compiler (lex → parse → lower)
5. Integration tests with a toy program (e.g., token transfer with `balances` table)

**Exit criteria**: All met. 191 tests passing. Overlay, rollback, consistency, arithmetic, DSL all working.

### Phase 2: Proof Foundation (State Commitment + Single-Tx Proof)

> See [proof-spec.md](./proof-spec.md) for detailed design.

**Goal**: Build the cryptographic foundation and prove `oldRoot → newRoot` for one transaction.

**Deliverables**:

1. **Trait polish** (T1-T3): `SigVerifier`/`NoncePolicy` → `Result<()>`, `hash_many` default method, remove `column_len`
2. **Plonky3 integration**: add `p3-*` workspace dependencies behind feature flags, `BabyBearCodec` implementing `ValueCodec`
3. **Poseidon hasher**: `Hasher` trait impl using Poseidon over BabyBear
4. **Sparse Merkle Tree**: 64-level, Poseidon-based, with `Open`, `Verify`, `Update`
5. **SSMC**: Small Sparse Map Commitment — AIR trace sub-table + domain-separated Poseidon sponge commitment + LogUp membership/non-membership (with strict inequality gap witnesses) + 3-way merge update proof (with delete support). Empty columns handled via ColumnMeta only (`is_empty_old=1` → no GlobalSSMC rows, `is_touched=0` → no GlobalMerge rows)
6. **Hybrid VC layer**: per-column strategy selection (SSMC ≤ threshold, SMT > threshold; threshold TBD per §9 break-even analysis, estimated 100-300 rows), domain-tagged table roots, ColumnMeta table `(t, c, tag, Com_old, Com_new, is_empty_old, is_empty_new, is_touched)` wiring commitments to root inclusion proofs + meta-level SMT update proofs
7. **AIR trace layout**: instruction columns (with `is_access` flag + `clk` recurrence) + slot columns + access columns + GlobalSSMC / GlobalMerge / GlobalSortedMem tables with `is_real` prefix constraints on all global tables + init-row format constraints explicit in AIR + root inclusion path columns
8. **Layer B constraints** (proof-spec): instruction correctness, clock binding (`τ = clk_i + 1`, §8.7), SSA slot consistency, base opening verification (SSMC: LogUp + gap checks, SMT: Merkle path), root inclusion proofs, meta-level SMT update proof for root transition, state update verification (merge proof with delete, path update)
9. **End-to-end single-tx proof**: generate and verify

**Exit criteria**:

- [ ] Poseidon hasher passes correctness tests against known test vectors
- [ ] SMT produces valid Merkle proofs (open, verify, update)
- [ ] SSMC produces valid commitments with LogUp-based membership AND non-membership openings (strict inequality gap witnesses)
- [ ] SSMC merge update proof correctly enforces WriteSet-only changes (including delete via `WRITE(k, Null)`)
- [ ] SSMC empty column handled via ColumnMeta (`is_empty_old=1`, no GlobalSSMC rows; `is_touched=0` → no GlobalMerge rows)
- [ ] ColumnMeta table correctly wires `Com[t,c]` to root inclusion proofs and meta-level SMT update proofs
- [ ] Clock binding (`τ = clk_i + 1`) prevents timestamp manipulation
- [ ] Hybrid VC auto-selects strategy based on column size
- [ ] Single-tx proof generates and verifies for `Read → Add → Write`
- [ ] `root0 → root1 → root2` chaining via two proofs
- [ ] Benchmark: constraint count for SSMC vs SMT at various column sizes

### Phase 3: Batch Proof with Memory Argument

> See [proof-spec.md](./proof-spec.md) §7-8 for the memory consistency design.

**Goal**: Prove `oldRoot → newRoot` for a batch of N transactions in one proof.

**Deliverables**:

1. GlobalSortedMem construction from batch execution events — per-(t,c) segments with `is_real` prefix constraints on all global tables, `same_group` boundaries, segment-first init constraint, strict lexicographic ordering
2. LogUp argument (Plonky3 native) linking execution access log to GlobalSortedMem with namespaced fingerprints and explicit multiplicity columns
3. Sorting + transition constraints (range checks for ordering, read/write consistency), gated by `is_real ∧ same_group`. Init row uniqueness per `(t,c,r)`
4. Init row generation with hybrid opening proofs + meta-level SMT update proofs. Init-row format constraints explicit in AIR (τ=0, is_write=0). Clock binding: `τ = clk_i + 1` (§8.7)
5. Write coalescing via `is_last_for_key ∧ has_written` selectors, single root update via hybrid VC with ColumnMeta table + meta-level SMT update proofs for root transition
6. Failed tx exclusion from access log; `AppliedTxDigest` as public input
7. End-to-end batch proof generation and verification

**Exit criteria**:

- [ ] Batch proof with inter-tx read-after-write semantics generates and verifies
- [ ] Memory consistency proven per-(t,c) group via LogUp with namespaced fingerprints
- [ ] Clock binding (`τ = clk_i + 1`) prevents timestamp manipulation
- [ ] Write-set extraction via `is_last_for_key ∧ has_written` selectors works correctly
- [ ] Benchmark harness comparing against a zkVM baseline
- [ ] Proof size and verification time within acceptable bounds
