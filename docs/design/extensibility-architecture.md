# Tabula Extensibility Architecture

> **Status**: Draft v0.3
> **Date**: 2026-03-05
> **Depends on**: tabula-machine-architecture.md (v0.4), proof-spec.md, semantics-spec.md
> **Scope**: Framework-level extensibility for purpose-built ZK applications

## 1. Vision

Tabula is a **framework for building purpose-built verifiable state machines**. Rather than a fixed VM with a fixed instruction set, Tabula provides composable building blocks — instructions, chips, state strategies, and proof infrastructure — that application developers assemble into an optimized, application-specific proving system.

The design principle: **applications customize what they need, inherit everything else.**

### 1.1 The Zero-Modification Principle

An application MUST be able to define all customizations — chips, buses, state commitment strategies, precompile handlers — **purely in its own crate**. The Tabula codebase is consumed as an immutable Cargo dependency and is never modified by applications.

```
┌──────────────────────────────────────────────────────────────┐
│  Tabula (immutable Cargo dependency, never modified by apps) │
│  tabula-core, tabula-ir, tabula-executor, tabula-machine,   │
│  tabula-gadgets, tabula-lang, tabula-std                     │
└──────────────────────────┬───────────────────────────────────┘
                    Cargo dependency (read-only)
┌──────────────────────────▼───────────────────────────────────┐
│  App Crate (100% of customization lives here)                │
│                                                              │
│  Composition point (the only "wiring" code):                 │
│    define_chip_set! { include TabulaCoreAir; + app chips }   │
│    config.register_vc(app_vc)                                │
│    BatchEnv { precompile_handler: &app_handler, ... }        │
│                                                              │
│  Pure app definitions (trait impls, no Tabula code changes): │
│    impl ChipSpec + Air<AB> for AppChip                       │
│    impl TraceContributor for AppChip                         │
│    impl VectorCommitment for AppVc                           │
│    impl PrecompileHandler for AppPrecompiles                 │
│    define_bus! for app buses                                 │
│    .tab files (DSL tx types)                                 │
└──────────────────────────────────────────────────────────────┘
                    uses
┌──────────────────────────────────────────────────────────────┐
│  Plonky3 (re-exported through tabula-machine::prelude)       │
└──────────────────────────────────────────────────────────────┘
```

### 1.2 Design Goals

1. **Zero-modification**: Apps never fork or modify Tabula's codebase.
2. **Graduated complexity**: Simple apps use core IR unchanged. Complex apps go deeper.
3. **Composability**: Extensions compose via LogUp buses — no coupling between components.
4. **Near-optimal efficiency**: Custom chips approach purpose-built circuit performance.
5. **Type safety**: Compile-time verification of chip composition, bus signatures, instruction schemas.
6. **Minimal boilerplate**: Macros and traits eliminate repetitive wiring code.
7. **Upgrade resilience**: Apps survive Tabula minor version updates without code changes.

### 1.3 Non-Goals

- Runtime-pluggable chips (p3's `Air<AB>` requires static dispatch)
- Changing the base field (BabyBear is fixed)
- General-purpose computation (Tabula is a state machine, not a zkVM)

### 1.4 Framework Prerequisites

The Zero-Modification Principle requires a set of **one-time framework changes** in Tabula. Once these are in place, all subsequent app development requires zero changes to Tabula's codebase.

| # | Change | Current State | Target State | Scope |
|---|--------|---------------|--------------|-------|
| F1 | `BusId` newtype | Closed `InteractionKind` enum (11 variants) | `BusId(u16)` newtype with reserved ranges (core: 0-99, app: 100+) | ~50 LOC |
| F2 | `define_chip_set!` `include` | No composition syntax | `include TabulaCoreAir;` copies all core variants into app enum | ~100 LOC macro |
| F3 | `TraceContributor` trait | Hardcoded per-chip wiring in `orchestration.rs` | Trait-based generic loop, chips self-register | ~200 LOC |
| F4 | `WitnessStore` | Implicit data passing via function arguments | Typed key-value store, chips declare dependencies | ~100 LOC |
| F5 | `VectorCommitment` trait | SSMC/SMT hardcoded | Trait with `commit()`, `prove_transition()`, `chip_name()` | ~100 LOC |
| F6 | `PropertyOpening` trait | No structural query mechanism | Trait for ordered/aggregate queries against committed state | ~100 LOC |
| F7 | `OpcodeSpec` trait | 13-variant closed `Instruction` enum | Trait-based dispatch for execute / constrain / witness | ~150 LOC |
| F8 | `define_instruction_set!` macro | Manual enum + `map_slots()` + typecheck | Generates enum, serialization, dispatch from declarative spec | ~300 LOC macro |
| F9 | `Precompile` IR variant | No generic computation dispatch | Single `Precompile { id, dst_slots, inputs }` variant | ~50 LOC |
| F10 | `PrecompileHandler` trait | N/A | Executor-side dispatch for precompile execution | ~50 LOC |
| F11 | `TemplateChip` trait | Only generic `ExecutionChip` | Trait for specialized tx-pattern chips + equivalence harness | ~200 LOC |
| F12 | `tabula-machine::prelude` | Apps import p3 crates directly | Stable re-export of p3 types through Tabula | ~50 LOC |

**Total**: ~1,450 LOC of framework-level changes (traits, macros, and refactoring). After these changes, every "Files Affected" section in this document (§3.3, §4.5, §5.5, etc.) applies only to the initial framework setup — not to individual applications.

The dependency between these prerequisites:

```
F1 (BusId) ──→ F2 (chip_set include) ──→ F3+F4 (TraceContributor) ──→ F5 (VC trait)
                                                                    ──→ F11 (Template)
F7 (OpcodeSpec) ──→ F8 (instruction_set macro) ──→ F9+F10 (Precompile)
F12 (prelude) — independent, can be done anytime
```

---

## 2. Extension Axes

Tabula's extensibility decomposes into seven orthogonal axes, each with a clear extension mechanism and a well-defined boundary of responsibility.

```
Axis 1: Instruction Set ────── what computations are available
Axis 2: Chip Composition ───── what AIR components prove correctness
Axis 3: Trace Pipeline ──────── how witnesses flow to chips
Axis 4: State Commitment ───── how column state is committed
Axis 5: State Opening ───────── how reads against committed state are proven
Axis 6: Execution Strategy ──── how tx bodies are proven (interpreter/template/compiled)
Axis 7: Proof Composition ───── how sub-proofs aggregate into batch proofs
```

Each axis is independently extensible. An extension on one axis composes with all existing strategies on other axes through the LogUp bus system (the "product composition" property from tabula-native-optimizations.md §5).

---

## 3. Axis 1: Instruction Set Extension

### 3.1 Problem

The current `Instruction` enum is a closed 13-variant type. Adding a new opcode requires coordinated changes across 12 files in 5 crates. At 30+ opcodes, this pattern becomes a maintenance burden.

### 3.2 Mechanism: Opcode Trait + Instruction Set Macro

#### 3.2.1 `OpcodeSpec` Trait

Each opcode is a self-contained unit implementing a trait:

```rust
/// Defines a single opcode's behavior across all layers.
trait OpcodeSpec: Send + Sync + 'static {
    /// Unique identifier.
    const OPCODE_ID: u16;
    /// Human-readable name.
    const NAME: &'static str;

    /// Operand schema for static type checking.
    fn operand_schema(&self) -> OperandSchema;

    /// Execute in the interpreter.
    fn execute(
        &self,
        operands: &ResolvedOperands,
        ctx: &ExecContext,
    ) -> Result<OpcodeResult, TabulaError>;

    /// Populate witness columns.
    fn populate_witness(
        &self,
        record: &InstructionRecord,
        cols: &mut [BabyBear],
        offset: usize,
    );

    /// Emit AIR constraints.
    fn constrain<AB: InteractionAirBuilder>(
        &self,
        builder: &mut AB,
        local: &[AB::Var],
        offset: usize,
        selector: AB::Var,
    );

    /// Number of witness columns this opcode requires.
    fn witness_width(&self) -> usize;
}
```

```rust
/// Describes an opcode's type-checking rules declaratively.
struct OperandSchema {
    /// Input operands with type constraints.
    inputs: Vec<OperandConstraint>,
    /// Output slots with result type rules.
    outputs: Vec<ResultTypeRule>,
    /// Whether this opcode accesses state (affects NF validation).
    accesses_state: bool,
}

enum OperandConstraint {
    /// Any value type (inferred from context).
    AnyValue,
    /// Must be a specific type.
    Typed(ValueType),
    /// Must match another operand's type.
    SameAs(usize),
    /// Row expression.
    RowExpr,
}

enum ResultTypeRule {
    /// Same type as input operand at index.
    SameAsInput(usize),
    /// Fixed type.
    Fixed(ValueType),
    /// Inferred from schema (for Read).
    FromSchema,
}
```

#### 3.2.2 `define_instruction_set!` Macro

Generates the `Instruction` enum, serialization, `map_slots()`, `dst_slots()`, and typecheck dispatch from a declarative specification:

```rust
define_instruction_set! {
    pub enum AppInstruction {
        // Include all Tabula core instructions
        include TabulaCore;

        // App-defined instructions
        Bitwise {
            dst: Slot,
            op: BitwiseOp,
            lhs: ValueExpr,
            rhs: ValueExpr,
        } => impl BitwiseOpcode;

        SigVerify {
            dst: Slot,
            scheme: SigScheme,
            pubkey: ValueExpr,
            message: ValueExpr,
            signature: ValueExpr,
        } => impl SigVerifyOpcode;

        WideMul {
            dst_hi: Slot,
            dst_lo: Slot,
            lhs: ValueExpr,
            rhs: ValueExpr,
        } => impl WideMulOpcode;
    }
}
```

The macro generates:
- The `AppInstruction` enum with all core + custom variants
- `map_slots()` and `dst_slots()` implementations (derived from field types)
- Borsh/Serde serialization
- A dispatch table mapping opcode ID → `&dyn OpcodeSpec`
- Typecheck dispatch using `OperandSchema` from each opcode's `operand_schema()`
- NF validation classification (`accesses_state` flag)

#### 3.2.3 What the App Developer Writes

For a pure-compute opcode like `Bitwise`:

```rust
struct BitwiseOpcode;

impl OpcodeSpec for BitwiseOpcode {
    const OPCODE_ID: u16 = 100;
    const NAME: &'static str = "bitwise";

    fn operand_schema(&self) -> OperandSchema {
        OperandSchema {
            inputs: vec![OperandConstraint::AnyValue, OperandConstraint::SameAs(0)],
            outputs: vec![ResultTypeRule::SameAsInput(0)],
            accesses_state: false,
        }
    }

    fn execute(&self, ops: &ResolvedOperands, _ctx: &ExecContext) -> Result<OpcodeResult> {
        let (lhs, rhs) = (ops.value(0)?, ops.value(1)?);
        let op: BitwiseOp = ops.metadata()?;
        let result = match (lhs, rhs, op) {
            (Value::U64(a), Value::U64(b), BitwiseOp::And) => Value::U64(a & b),
            (Value::U64(a), Value::U64(b), BitwiseOp::Or)  => Value::U64(a | b),
            (Value::U64(a), Value::U64(b), BitwiseOp::Xor) => Value::U64(a ^ b),
            (Value::U64(a), Value::U64(b), BitwiseOp::Shl) => Value::U64(a << (b as u32)),
            (Value::U64(a), Value::U64(b), BitwiseOp::Shr) => Value::U64(a >> (b as u32)),
            _ => return Err(TabulaError::TypeMismatch { .. }),
        };
        Ok(OpcodeResult::single(result))
    }

    fn witness_width(&self) -> usize { 64 + 64 + 64 } // bit decomposition

    fn populate_witness(&self, record: &InstructionRecord, cols: &mut [BabyBear], offset: usize) {
        // Decompose lhs, rhs, dst into individual bits
        // ...
    }

    fn constrain<AB: InteractionAirBuilder>(&self, builder: &mut AB, ...) {
        // For AND: dst_bit[i] = lhs_bit[i] * rhs_bit[i]
        // Reconstitution: sum(bit[i] * 2^i) = original value
        // ...
    }
}
```

#### 3.2.4 Standard Library Opcodes

Tabula provides ready-to-use opcode implementations:

| Opcode | Category | Status |
|--------|----------|--------|
| Read, Write, Lookup | State access | Core (built-in) |
| Arith (Add/Sub/Mul), DivMod | Arithmetic | Core (built-in) |
| Cmp (6 sub-ops) | Comparison | Core (built-in) |
| Not, And, Or | Logic | Core (built-in) |
| Assert, Select | Control | Core (built-in) |
| Hash | Crypto | Core (built-in) |
| Emit | Side-effect | Core (built-in) |
| Bitwise (And/Or/Xor/Shl/Shr) | Bitwise | Standard library |
| WideMul | Wide arithmetic | Standard library |
| SigVerify (Ed25519/Secp256k1) | Crypto | Standard library |
| Cast (U64↔I64, Bool→U64) | Type conversion | Standard library |

Core opcodes are always included. Standard library opcodes are opt-in via `include` in the instruction set definition.

### 3.3 Files Affected

All changes below are **one-time framework changes** (prerequisites F7, F8). After these, app-defined opcodes live entirely in the app crate.

| File | Change | Prerequisite |
|---|---|---|
| `crates/ir/src/instruction.rs` | Refactor into `OpcodeSpec` trait + macro | F7 |
| New: `crates/ir/src/opcode.rs` | `OpcodeSpec`, `OperandSchema`, dispatch table | F7 |
| New: `crates/ir/src/opcodes/` | Per-opcode implementations (core + standard library) | F7 |
| `crates/ir/src/pass/typecheck.rs` | Replace match block with `OperandSchema`-driven dispatch | F8 |
| `crates/ir/src/pass/validate.rs` | Use `accesses_state` flag instead of pattern matching | F8 |
| `crates/executor/src/interpreter.rs` | Replace match block with `OpcodeSpec::execute()` dispatch | F8 |
| `crates/proof/src/chips/execution/` | `ExecutionChip` becomes opcode-table-driven | F8 |
| `crates/proof/src/trace/lowering/` | Replace per-opcode files with `OpcodeSpec::populate_witness()` | F8 |

---

## 4. Axis 2: Chip Composition

### 4.1 Problem

The `TabulaAir` enum generated by `define_chip_set!` is a compile-time-closed set. Adding a chip requires modifying `chips/mod.rs`. There is no mechanism to compose a core chip set with app-defined chips.

### 4.2 Mechanism: Composable Chip Set Macro

#### 4.2.1 `include` Syntax

```rust
// Core chip set (provided by tabula-machine)
define_chip_set! {
    pub enum TabulaCoreAir {
        Execution(ExecutionChip<3>),
        InterTxOrder(InterTxOrderChip<3>),
        StateColumn(StateColumnChip<3>),
        ColumnMeta(ColumnMetaChip),
        Poseidon(PoseidonChip),
        RangeCheck(RangeCheckChip),
        StaticTable(StaticTableChip<3>),
        SmtColPath(SmtColPathChip),
        SmtTablePath(SmtTablePathChip),
    }
}

// App extends with custom chips
define_chip_set! {
    pub enum LighterAir {
        include TabulaCoreAir;
        EcdsaVerify(EcdsaVerifyChip),
        OrderbookTree(OrderbookTreeChip<24>),
        FillOrder(FillOrderTemplateChip),
    }
}
```

The `include` directive:
- Copies all variants from `TabulaCoreAir` into `LighterAir`
- Forwards `ChipSpec`, `BaseAir<F>`, `Air<AB>` implementations
- Merges `ChipSet::all_chips()`, `from_name()`, `chip_names()`
- Preserves exhaustive pattern matching

#### 4.2.2 Chip Definition Pattern

A new chip follows the existing 3-file pattern. The framework provides helper traits:

```rust
/// Minimal trait for registering a chip in a chip set.
/// ChipSpec + Default + Send + Sync is required by define_chip_set!.
pub trait ChipSpec: Default + Send + Sync {
    fn chip_name(&self) -> &'static str;
    fn num_public_values(&self) -> usize { 0 }
    fn preprocessed_width(&self) -> usize { 0 }
    fn has_interactions(&self) -> bool { true }
}
```

The developer defines:
1. **`columns.rs`**: `#[repr(C)]` column struct parameterized by `T`
2. **`air.rs`**: `impl Air<AB> for MyChip where AB: InteractionAirBuilder`
3. **`trace.rs`**: `impl TraceContributor for MyChip` (see Axis 3)

#### 4.2.3 Feature-Gated Chips

For optional or app-specific chips that should not inflate the binary:

```rust
define_chip_set! {
    pub enum AppAir {
        include TabulaCoreAir;

        #[cfg(feature = "ecdsa")]
        EcdsaVerify(EcdsaVerifyChip),

        #[cfg(feature = "keccak")]
        Keccak(KeccakChip),
    }
}
```

Inactive chips are excluded at compile time — zero overhead.

### 4.3 Prover/Verifier Genericity

The prover and verifier are already generic over `CS: StarkAir`:

```rust
pub fn prove<CS: StarkAir>(traces: TraceMap, ...) -> TabulaProof { ... }
pub fn verify<CS: StarkAir>(proof: &TabulaProof, ...) -> Result<()> { ... }
```

App code calls `prove::<LighterAir>(...)`. No prover/verifier changes needed.

### 4.4 Bus Extension

Apps can define new LogUp buses:

```rust
// In app code
define_bus! {
    pub EcdsaVerifyAirBuilder(
        InteractionKind::EcdsaVerify,
        send_ecdsa_verify,
        receive_ecdsa_verify
    ) {
        pubkey_x: var_arr<3>,
        pubkey_y: var_arr<3>,
        msg_hash: var_arr<8>,
        sig_valid: expr,
    }
}
```

Adding a new bus requires adding a variant to `InteractionKind`. This enum should be made extensible:

```rust
// Instead of a closed enum, use a newtype with reserved ranges
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BusId(pub u16);

impl BusId {
    // Core buses: 0-99
    pub const MEMORY: Self = Self(1);
    pub const POSEIDON_PERM: Self = Self(5);
    pub const RANGE_CHECK: Self = Self(8);
    // ...

    // App buses: 100+
    pub const fn app(id: u16) -> Self { Self(100 + id) }
}
```

### 4.5 Files Affected

All changes below are **one-time framework changes** (prerequisites F1, F2). After these, app-defined chips and buses live entirely in the app crate via `define_chip_set! { include ...; }` and `BusId::app(N)`.

| File | Change | Prerequisite |
|---|---|---|
| `crates/proof/src/air/chip_set.rs` | Add `include` support to `define_chip_set!` | F2 |
| `crates/proof/src/air/interaction.rs` | Replace `InteractionKind` enum with `BusId` newtype | F1 |
| `crates/proof/src/chips/mod.rs` | Split into `TabulaCoreAir` (lib) + app-specific set | F2 |

---

## 5. Axis 3: Trace Pipeline Extension

### 5.1 Problem

`trace/orchestration.rs` is the biggest friction point. Every chip is manually wired: constructed, evaluated, and inserted into `TraceMap` in hardcoded sequence. Adding a chip requires 3+ edits in this 148-line function.

### 5.2 Mechanism: `TraceContributor` Trait

```rust
/// Trait for chips that contribute witness traces.
/// Replaces hardcoded per-chip wiring in orchestration.rs.
pub trait TraceContributor: ChipSpec {
    /// Witness data this chip needs. Keyed by a string tag.
    fn required_witness_keys(&self) -> &[&str];

    /// Generate the trace from witness data.
    /// `witness_store` provides access to shared witness data by key.
    fn build_trace(&self, store: &WitnessStore) -> TraceEntry;

    /// Ordering priority (lower = built first). Default 100.
    /// Chips that produce shared witness data (e.g., Poseidon records)
    /// should have lower priority than chips that consume it.
    fn priority(&self) -> u32 { 100 }
}
```

```rust
/// Shared witness data store. Chips deposit and consume data by key.
pub struct WitnessStore {
    data: BTreeMap<String, Box<dyn Any + Send + Sync>>,
}

impl WitnessStore {
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> { ... }
    pub fn insert<T: 'static + Send + Sync>(&mut self, key: &str, value: T) { ... }
}
```

### 5.3 Orchestration Becomes Generic

```rust
fn build_all_traces<CS: ChipSet>(
    batch_witness: &BatchWitness,
    store: &mut WitnessStore,
) -> TraceMap
where
    CS: TraceContributable, // marker: all chips in CS implement TraceContributor
{
    // Populate store with batch-level witness data
    store.insert("instruction_records", batch_witness.instruction_records.clone());
    store.insert("memory_records", batch_witness.memory_records.clone());
    store.insert("smt_witnesses", batch_witness.smt_witnesses.clone());
    // ...

    // Build traces in priority order
    let mut chips: Vec<_> = CS::all_chips().collect();
    chips.sort_by_key(|c| c.priority());

    let mut map = TraceMap::new();
    for chip in chips {
        if chip.is_active(store) {
            let entry = chip.build_trace(store);
            map.insert(chip.chip_name().to_string(), entry);
        }
    }
    map
}
```

### 5.4 Example: Custom Chip's TraceContributor

```rust
impl TraceContributor for OrderbookTreeChip<24> {
    fn required_witness_keys(&self) -> &[&str] {
        &["orderbook_transitions"]
    }

    fn build_trace(&self, store: &WitnessStore) -> TraceEntry {
        let transitions: &[OrderbookTransition] = store.get("orderbook_transitions").unwrap();
        let num_rows = transitions.len() * self.rows_per_transition();
        let mut trace = RowMajorMatrix::new(vec![BabyBear::ZERO; num_rows * self.width()], self.width());
        // ... populate trace from transitions ...
        TraceEntry { main: trace, preprocessed: None, public_values: vec![] }
    }
}
```

### 5.5 Files Affected

All changes below are **one-time framework changes** (prerequisites F3, F4). After these, app chips participate in the trace pipeline by implementing `TraceContributor` in their own crate.

| File | Change | Prerequisite |
|---|---|---|
| New: `crates/proof/src/trace/contributor.rs` | `TraceContributor` trait, `WitnessStore` | F3, F4 |
| `crates/proof/src/trace/orchestration.rs` | Replace hardcoded wiring with generic loop | F3 |
| `crates/proof/src/chips/*/trace.rs` | Each core chip implements `TraceContributor` | F3 |

---

## 6. Axis 4: State Commitment Extension

### 6.1 Problem

State commitment is hardcoded to SSMC (sorted hash chain) and SMT (sparse Merkle tree). Applications with specialized data structures (e.g., sorted orderbook tree) cannot define custom commitment schemes.

### 6.2 Mechanism: `VectorCommitment` Trait

```rust
/// Trait for column-level state commitment strategies.
///
/// A VectorCommitment defines how a column's entries are committed
/// to a single digest, and how state transitions are proven.
///
/// The commitment digest MUST be a NativeDigest ([BabyBear; 8])
/// for compatibility with ColumnMeta and SMT root computation.
pub trait VectorCommitment: Send + Sync {
    /// Unique identifier for this VC strategy.
    fn vc_id(&self) -> VcId;

    /// Human-readable name.
    fn name(&self) -> &'static str;

    /// Compute commitment digest from a set of entries.
    ///
    /// `entries` is sorted by key. Each entry is (key, encoded_value, is_null).
    /// The encoding uses the column's schema type width.
    fn commit(&self, entries: &[(RowKey, &[BabyBear], bool)]) -> NativeDigest;

    /// Generate witness data for proving a state transition.
    ///
    /// The framework provides old entries, the writes applied, and the new entries.
    /// The VC returns opaque witness data consumed by its AIR chip.
    fn prove_transition(
        &self,
        old_entries: &[(RowKey, &[BabyBear], bool)],
        writes: &[(RowKey, &[BabyBear], bool)],
        new_entries: &[(RowKey, &[BabyBear], bool)],
    ) -> Box<dyn VcWitness>;

    /// Name of the AIR chip that proves this VC's transitions.
    /// Must match a chip registered in the chip set.
    fn chip_name(&self) -> &'static str;
}

/// Opaque witness data for a VC transition proof.
pub trait VcWitness: Send + Sync + Any {
    fn as_any(&self) -> &dyn Any;
}
```

### 6.3 Built-in Implementations

```rust
/// Sorted Sub-table Multiset Commitment (existing).
/// Optimal for small columns (< threshold entries).
pub struct SsmcCommitment { /* ... */ }

/// Sparse Merkle Tree (existing).
/// Optimal for large columns (> threshold entries).
pub struct SmtCommitment { depth: usize }
```

### 6.4 Bus Integration

Custom VC chips integrate through **existing buses only** — no new bus definitions needed:

| Bus | Direction | Purpose |
|-----|-----------|---------|
| CommitmentVerification (C6) | receive | Bind computed digest to ColumnMeta |
| BaseStateEntry (C13) | receive | Consume init rows (base state values) |
| CoalescedWrite (C14) | receive | Consume write operations |
| PoseidonPermutation (C5) | send | Internal hashing (if Poseidon-based) |
| RangeCheck (C8) | send | Range-check limbs (if needed) |

The LogUp bus balance guarantees soundness: if the custom VC chip computes a different digest than what ColumnMeta expects, the CommitmentVerification bus will be unbalanced.

### 6.5 Column Strategy Selection

The `ProofPlan` (from tabula-machine-architecture.md §4.2) determines which VC strategy each column uses:

```rust
pub struct ColumnPlan {
    pub table: TableId,
    pub col: ColId,
    pub schema_type: ValueType,
    pub strategy: ColumnStrategy,
}

pub enum ColumnStrategy {
    /// Column not touched in this batch. Zero cost.
    Untouched,
    /// Read-only access. Only Meta + SMT path needed.
    ReadOnly { vc: VcId },
    /// Small number of accesses. Lightweight proof.
    ShortRun { pattern: AccessPattern, vc: VcId },
    /// Full memory consistency proof.
    Full { vc: VcId },
}
```

Apps register custom VCs at program setup:

```rust
let mut config = ProofConfig::default();
config.register_vc(OrderbookTreeVc::new(24));

// Override VC for specific columns
config.set_column_vc(TableId(1), ColId(0), OrderbookTreeVc::VC_ID); // bids column
config.set_column_vc(TableId(1), ColId(1), OrderbookTreeVc::VC_ID); // asks column
```

### 6.6 Example: Orderbook Tree VC

```rust
pub struct OrderbookTreeVc {
    depth: usize,
}

impl VectorCommitment for OrderbookTreeVc {
    fn vc_id(&self) -> VcId { VcId(0x4F42) }
    fn name(&self) -> &'static str { "orderbook_tree" }

    fn commit(&self, entries: &[(RowKey, &[BabyBear], bool)]) -> NativeDigest {
        // Build balanced binary tree from sorted entries
        // Internal nodes aggregate: (total_ask_qty, total_bid_qty, best_ask, best_bid)
        // Root hash = Poseidon(domain_tag || aggregated_data || children_hashes)
        build_tree_root(entries, self.depth)
    }

    fn prove_transition(&self, old: &[...], writes: &[...], new: &[...]) -> Box<dyn VcWitness> {
        // Generate Merkle authentication paths for modified leaves
        // Generate old root → new root transition witness
        Box::new(OrderbookTreeWitness {
            old_paths: compute_auth_paths(old, self.depth),
            new_paths: compute_auth_paths(new, self.depth),
            internal_updates: compute_aggregation_updates(old, writes, new),
        })
    }

    fn chip_name(&self) -> &'static str { "orderbook_tree" }
}
```

### 6.7 Files Affected

All changes below are **one-time framework changes** (prerequisite F5). After these, custom VC strategies are defined entirely in the app crate via `impl VectorCommitment`.

| File | Change | Prerequisite |
|---|---|---|
| New: `crates/proof/src/state/vc.rs` | `VectorCommitment` trait, `VcId`, `VcWitness` | F5 |
| New: `crates/proof/src/state/ssmc.rs` | Extract existing SSMC into `VectorCommitment` impl | F5 |
| New: `crates/proof/src/state/smt.rs` | Extract existing SMT into `VectorCommitment` impl | F5 |
| `crates/proof/src/witness/` | Use `VcId` from `ColumnPlan` to dispatch witness generation | F5 |
| `crates/proof/src/trace/orchestration.rs` | Route VC witness data to appropriate chip via `WitnessStore` | F3 |

---

## 7. Axis 5: State Opening Extension

### 7.1 Problem

Tabula's state model is a key-value store: `Read(t, c, r)` returns the value at key `r` in column `(t, c)`. This is sufficient for simple lookups, but applications like order-matching DEXs need to prove **structural properties**: "this is the minimum key," "no key exists in range [a, b]," "this is the successor of key k."

Without opening extensions, the prover could maliciously skip better-priced orders and the circuit could not detect it.

### 7.2 Mechanism: `PropertyOpening` Trait

```rust
/// Proves structural properties of committed state.
///
/// This extends beyond simple key-value opening (which Tabula handles
/// via init rows + memory consistency) to ordered/aggregate queries.
pub trait PropertyOpening: Send + Sync {
    /// The VC strategy this opening is compatible with.
    fn compatible_vc(&self) -> VcId;

    /// Supported query types.
    fn supported_queries(&self) -> &[PropertyQueryKind];

    /// Generate witness for a property query against committed state.
    fn prove_property(
        &self,
        commitment: &NativeDigest,
        query: &PropertyQuery,
        state: &[(RowKey, &[BabyBear], bool)],
    ) -> Box<dyn PropertyWitness>;

    /// Name of the AIR chip that verifies property proofs.
    fn chip_name(&self) -> &'static str;
}

/// Structural queries against committed state.
pub enum PropertyQuery {
    /// The entry with the minimum key.
    Minimum,
    /// The entry with the maximum key.
    Maximum,
    /// The entry immediately after key `k` (or proof of non-existence).
    Successor(RowKey),
    /// The entry immediately before key `k`.
    Predecessor(RowKey),
    /// Proof that no entry exists with key in `[lo, hi]`.
    NonExistenceRange(RowKey, RowKey),
    /// Aggregate value over all entries (e.g., sum of quantities).
    Aggregate(AggregateKind),
}

pub enum AggregateKind {
    Sum,
    Count,
    // App-defined aggregates via extension
    Custom(u32),
}
```

### 7.3 IR Integration

Property openings surface as a new instruction variant:

```rust
Instruction::PropertyRead {
    dst_val: Slot,
    dst_is_null: Slot,
    table: TableId,
    col: ColId,
    query: PropertyQuery,
}
```

This instruction queries committed state for a structural property. The prover provides witness data; the AIR chip verifies the property holds against the column's commitment.

### 7.4 Example: "Best Ask" Query for Lighter DEX

```
// In tabula-lang DSL
let best_ask_price = property_read orders.asks.minimum();
assert(fill_price >= best_ask_price);  // Ensure no better price was skipped
```

The `OrderbookTreeVc`'s property opening chip verifies:
1. The claimed minimum leaf exists in the committed tree
2. All leaves to its left are empty (proving minimality)
3. The path from leaf to root matches the commitment digest

### 7.5 Files Affected

Framework changes (F6) provide the trait; `PropertyRead` is added once via the instruction set framework (F8). App-defined openings live entirely in the app crate.

| File | Change | Prerequisite |
|---|---|---|
| New: `crates/proof/src/state/property.rs` | `PropertyOpening` trait, `PropertyQuery` enum | F6 |
| `crates/ir/src/instruction.rs` | Add `PropertyRead` variant (or via `define_instruction_set!`) | F8 |
| App code | Implement `PropertyOpening` for custom VCs | — (app-side) |

---

## 8. Axis 6: Execution Strategy Extension

### 8.1 Problem

The `ExecutionChip` is a monolithic chip (278 columns at W=3) that proves every instruction type. For applications where 90% of transactions follow a small number of patterns (e.g., `fill_order`), this is wasteful — most columns are unused per instruction.

### 8.2 Mechanism: Template Chips

Template chips are execution chips specialized for a specific transaction pattern. They prove the same bus interactions as the generic `ExecutionChip` but with fewer columns and tighter constraints.

```rust
/// A template chip replaces ExecutionChip for specific tx patterns.
///
/// SOUNDNESS INVARIANT: A template chip MUST emit identical LogUp bus
/// messages as the generic ExecutionChip would for the same transaction.
/// Bus balance enforces this — mismatched fingerprints cause verification failure.
pub trait TemplateChip: ChipSpec {
    /// Unique template identifier.
    fn template_id(&self) -> TemplateId;

    /// Check if a tx type definition matches this template.
    fn matches(&self, def: &TxTypeDef, info: &BodyTypeInfo) -> bool;

    /// Maximum number of instructions this template handles per tx.
    fn max_instructions(&self) -> usize;
}
```

### 8.3 Template Selection

At proof planning time, the framework checks each `TxTypeDef` against registered templates:

```rust
fn select_execution_strategy(
    def: &TxTypeDef,
    info: &BodyTypeInfo,
    templates: &[Box<dyn TemplateChip>],
) -> ExecutionVariant {
    for template in templates {
        if template.matches(def, info) {
            return ExecutionVariant::Template(template.template_id());
        }
    }
    ExecutionVariant::Interpreter
}
```

### 8.4 Bus Compatibility Testing

The framework provides a test harness to verify template correctness:

```rust
/// Verify that a template chip emits identical bus messages
/// as the generic ExecutionChip for the same transactions.
pub fn verify_template_equivalence<T: TemplateChip>(
    template: &T,
    test_txs: &[Transaction],
    program: &Program,
) -> Result<(), TemplateError> {
    for tx in test_txs {
        let interpreter_messages = collect_bus_messages_interpreter(tx, program);
        let template_messages = collect_bus_messages_template(template, tx, program);
        assert_eq!(interpreter_messages, template_messages,
            "Template {} emits different bus messages than interpreter for tx {:?}",
            T::chip_name(), tx);
    }
    Ok(())
}
```

### 8.5 Execution Strategy Composition

Template chips compose freely with all other axes:

```
Template ──[ReadAccess bus]──────→ InterTxOrder → StateColumn (or Custom VC)
Template ──[WriteAccess bus]─────→ InterTxOrder → StateColumn (or Custom VC)
Template ──[PoseidonPerm bus]────→ PoseidonChip
Template ──[PrecompileBus]───────→ Precompile Chips
Template ──[RangeCheck bus]──────→ RangeCheckChip
```

The template chip does not know or care which state commitment strategy each column uses. The bus is the only interface.

### 8.6 Files Affected

All changes below are **one-time framework changes** (prerequisite F11). After these, template chips are defined entirely in the app crate via `impl TemplateChip` + `impl Air<AB>`.

| File | Change | Prerequisite |
|---|---|---|
| New: `crates/proof/src/chips/template/mod.rs` | `TemplateChip` trait, `TemplateId` | F11 |
| New: `crates/proof/src/chips/template/test_harness.rs` | Equivalence testing | F11 |
| `crates/proof/src/witness/program_info.rs` | Template selection logic | F11 |
| `crates/proof/src/trace/orchestration.rs` | Route txs to template or interpreter chip | F3 |

---

## 9. Axis 7: Proof Composition Extension

### 9.1 Problem

Tabula currently produces a single flat proof per batch. For high-throughput applications, this creates a linear relationship between batch size and proving time. Lighter DEX compresses Block → Segment → Batch proofs via recursive aggregation.

### 9.2 Mechanism: `ProofAggregator` Trait

```rust
/// Aggregates multiple sub-proofs into a single proof.
pub trait ProofAggregator: Send + Sync {
    /// Aggregate N sub-proofs into one.
    fn aggregate(&self, proofs: &[TabulaProof]) -> AggregatedProof;

    /// Verify an aggregated proof.
    fn verify(&self, proof: &AggregatedProof) -> Result<(), VerificationError>;

    /// Maximum number of sub-proofs per aggregation step.
    fn fan_in(&self) -> usize;
}
```

### 9.3 Aggregation Strategies

```rust
/// Layered STARK aggregation (non-recursive).
/// Each layer merges N proofs by re-proving the verification.
pub struct LayeredStarkAggregator { fan_in: usize }

/// Recursive SNARK wrapper.
/// Wraps STARK proofs in a Groth16/FFLONK proof for L1 verification.
pub struct RecursiveSnarkWrapper { /* ... */ }

/// IVC (Incrementally Verifiable Computation).
/// Each batch proof includes verification of the previous batch.
pub struct IvcAggregator { /* ... */ }
```

### 9.4 Status

Proof composition is the most complex extension axis and depends on the v0.4 machine layer (shared PCS, two-round protocol). It is designed here for architectural completeness but deferred to a later implementation phase.

---

## 10. Precompile System

Orthogonal to the 7 axes, the precompile system provides a streamlined path for adding computation units that are too specialized for the core IR but reusable across applications.

### 10.1 Design

A single generic IR instruction handles all precompiles:

```rust
Instruction::Precompile {
    id: PrecompileId,
    dst_slots: Vec<Slot>,
    inputs: Vec<ValueExpr>,
}
```

Each precompile defines:

```rust
pub struct PrecompileDef {
    pub id: PrecompileId,
    pub name: &'static str,
    pub input_types: Vec<ValueType>,
    pub output_types: Vec<ValueType>,
}
```

### 10.2 Precompile Bus

All precompiles share a single bus with `precompile_id` discrimination:

```rust
define_bus! {
    pub PrecompileAirBuilder(BusId::PRECOMPILE, ...) {
        precompile_id: expr,    // Prevents cross-precompile collisions
        nonce: expr,            // Unique per invocation
        inputs: var_slice,      // Encoded input field elements
        outputs: var_slice,     // Encoded output field elements
    }
}
```

The ExecutionChip sends on this bus; each precompile chip receives and proves its specific computation.

### 10.3 Standard Precompiles

| ID | Name | Input | Output | Use Case |
|----|------|-------|--------|----------|
| 0x0001 | ecdsa_secp256k1_verify | (pubkey, msg_hash, sig) | Bool | User authentication |
| 0x0002 | ed25519_verify | (pubkey, msg, sig) | Bool | Oracle attestation |
| 0x0003 | keccak256 | (data...) | Bytes32 | EVM compatibility |
| 0x0004 | poseidon_hash | (data...) | Bytes32 | ZK-native hashing |
| 0x0005 | sha256 | (data...) | Bytes32 | Bitcoin compatibility |

### 10.4 App-Defined Precompiles

Apps define precompiles in app-id range (0x10000+):

```rust
// lighter-dex/src/precompiles/mod.rs
pub const ORDERBOOK_VERIFY: PrecompileId = PrecompileId(0x10001);

pub struct OrderbookVerifyPrecompile;
impl PrecompileChip for OrderbookVerifyPrecompile {
    fn precompile_id(&self) -> PrecompileId { ORDERBOOK_VERIFY }
    // AIR constraints verify orderbook tree operations
}
```

### 10.5 DSL Syntax

```
// Built-in precompile syntax
let valid = @ecdsa_verify(pubkey, msg_hash, signature);
let hash = @keccak256(data);

// App-defined precompile
let result = @orderbook_verify(tree_root, operation, proof);
```

### 10.6 Files Affected

All changes below are **one-time framework changes** (prerequisites F9, F10). After these, app-defined precompiles are implemented entirely in the app crate via `impl PrecompileHandler` (executor) + precompile AIR chip.

| File | Change | Prerequisite |
|---|---|---|
| `crates/ir/src/instruction.rs` | Add `Precompile` variant | F9 |
| New: `crates/ir/src/precompile.rs` | `PrecompileId`, `PrecompileDef`, `PrecompileRegistry` | F9 |
| New: `crates/executor/src/precompile.rs` | `PrecompileHandler` trait | F10 |
| `crates/executor/src/interpreter.rs` | Dispatch `Precompile` to handler | F10 |
| New: `crates/proof/src/chips/precompile/` | `PrecompileChip` trait, standard impls | F10 |

---

## 11. Case Study: Lighter DEX on Tabula

This section maps Lighter DEX's architecture onto the Tabula extensibility framework to validate completeness.

### 11.1 Lighter's Requirements

| Component | Requirement |
|-----------|-------------|
| Sequencer | Off-chain tx ordering → Batch production |
| Signature verification | ECDSA secp256k1 per order |
| Orderbook tree | Sorted binary tree with aggregated internal nodes |
| Price-time priority | Deterministic index = f(price, nonce) |
| Order matching | Fill best price first, variable N fills per market order |
| Risk/margin checks | Position value, PnL, funding rate calculations |
| State root | Merkle commitment over all state |
| Block commitment | Hash chain over block execution |
| Proof aggregation | Block → Segment → Batch compression |
| Data availability | Blob posting to Ethereum L1 |
| Escape hatch | L1 data enables state reconstruction |

### 11.2 Mapping to Tabula Axes

| Lighter Component | Tabula Axis | Mechanism |
|---|---|---|
| ECDSA verification | Precompile | `EcdsaVerifyChip` (precompile 0x0001) |
| Index calculation (`price << O \| nonce`) | Axis 1 | `BitwiseOp` (standard library opcode) |
| Wide multiplication (price × qty) | Axis 1 | `WideMul` (standard library opcode) |
| Orderbook tree state | Axis 4 | `OrderbookTreeVc` (custom VC) |
| Best price query | Axis 5 | `PropertyQuery::Minimum` (custom opening) |
| Fill order execution | Axis 6 | `FillOrderTemplate` (template chip) |
| Multi-fill decomposition | Core | Batch of N fill txs (existing batch model) |
| Risk/margin checks | Core IR | `Arith`, `Cmp`, `Assert` (existing) |
| Funding rate | Core IR | Fixed-point arithmetic with `DivMod` |
| State root | Core | SMT root proof (existing infrastructure) |
| Oracle price feed | Precompile | `EcdsaVerifyChip` on oracle signature |
| Proof aggregation | Axis 7 | `LayeredStarkAggregator` (future) |
| Data availability | External | `tabula-da` crate (future) |

### 11.3 Lighter's Tx Types

```
// place_order.tab
tx place_order(
    sig: Bytes32,           // ECDSA signature
    pubkey: Bytes32,        // trader's public key
    market_id: U64,
    side: U64,              // 0 = bid, 1 = ask
    price: U64,
    quantity: U64,
    nonce: U64,
) {
    // 1. Verify signature
    let order_hash = hash(market_id, side, price, quantity, nonce);
    let valid = @ecdsa_verify(pubkey, order_hash, sig);
    assert(valid);

    // 2. Calculate tree leaf index
    let nonce_space = 1u64 << 20;
    let index = select(
        side == 0,
        price << 20 | (nonce_space - 1 - nonce),  // bid: high price first
        price << 20 | nonce,                         // ask: low price first
    );

    // 3. Check margin
    let balance = read accounts[pubkey].balance;
    let required_margin = price * quantity / PRECISION;
    assert(balance >= required_margin);

    // 4. Write order to orderbook
    write orders[market_id].prices[index] = price;
    write orders[market_id].quantities[index] = quantity;
    write orders[market_id].owners[index] = pubkey;

    // 5. Lock margin
    write accounts[pubkey].balance = balance - required_margin;
    write accounts[pubkey].locked = read accounts[pubkey].locked + required_margin;

    emit("order_placed", market_id, side, price, quantity);
}
```

```
// fill_order.tab
tx fill_order(
    taker: Bytes32,
    maker: Bytes32,
    market_id: U64,
    maker_index: U64,
    fill_qty: U64,
    fill_price: U64,
) {
    // 1. Verify maker order exists
    let maker_qty = read orders[market_id].quantities[maker_index];
    let maker_price = read orders[market_id].prices[maker_index];
    assert(maker_qty >= fill_qty);
    assert(maker_price == fill_price);

    // 2. Verify best price (custom property opening)
    let best_price = property_read orders[market_id].prices.minimum();
    assert(fill_price <= best_price);  // No better price was skipped

    // 3. Update positions and PnL
    // ... (arithmetic on accounts)

    // 4. Update orderbook
    let remaining = maker_qty - fill_qty;
    write orders[market_id].quantities[maker_index] = remaining;

    emit("fill", market_id, taker, maker, fill_qty, fill_price);
}
```

### 11.4 Efficiency Analysis

| Component | Purpose-Built (Lighter) | Tabula Framework | Overhead |
|---|---|---|---|
| ECDSA chip | Custom circuit | Precompile chip | ~0% (same AIR) |
| Orderbook tree | Custom Merkle circuit | Custom VC chip | ~5% (bus fingerprints) |
| Fill execution | Monolithic circuit | Template chip (~60 cols) vs Interpreter (278 cols) | ~10% (bus overhead) |
| State root | Custom SMT | Built-in SMT | ~0% (same) |
| Proof aggregation | Custom recursion | Framework aggregator | TBD |
| **Overall** | **Baseline** | | **~5-10% overhead** |

The 5-10% overhead is the composability tax: LogUp bus fingerprint computation that enables modular composition. In exchange, development time drops from months to weeks.

---

## 12. Implementation Roadmap

### Phase 1: Foundation (framework prerequisites F1-F4, F12)

These are the one-time framework changes from §1.4 that enable the Zero-Modification Principle.

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `BusId` newtype replacing `InteractionKind` enum | F1 | Axis 2, ~50 LOC | High |
| `define_chip_set!` `include` support | F2 | Axis 2, ~100 LOC macro | High |
| `TraceContributor` trait + `WitnessStore` | F3, F4 | Axis 3, ~300 LOC | High |
| Refactor `trace/orchestration.rs` to generic loop | F3 | Axis 3, ~100 LOC | High |
| `tabula-machine::prelude` re-exports | F12 | Stable API, ~50 LOC | High |

### Phase 2: Instruction Extensibility (F7, F8)

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `OpcodeSpec` trait | F7 | Axis 1, ~150 LOC | High |
| `define_instruction_set!` macro (basic) | F8 | Axis 1, ~300 LOC | High |
| Standard library: `BitwiseOp` | — (app-side pattern) | Axis 1, ~200 LOC | High |
| Standard library: `WideMul` | — (app-side pattern) | Axis 1, ~150 LOC | High |

### Phase 3: Precompile System (F9, F10)

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `Precompile` instruction + `PrecompileRegistry` | F9 | §10, ~200 LOC | High |
| `PrecompileBus` definition | F1 | §10, ~50 LOC | High |
| `PrecompileHandler` trait (executor) | F10 | §10, ~50 LOC | High |
| Standard precompile: ECDSA secp256k1 | — (app-side pattern) | §10, ~500 LOC (chip) | High |

### Phase 4: State Extensibility (F5, F6)

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `VectorCommitment` trait | F5 | Axis 4, ~100 LOC | Medium |
| Extract SSMC/SMT into trait impls | F5 | Axis 4, ~refactor | Medium |
| `PropertyOpening` trait | F6 | Axis 5, ~100 LOC | Medium |
| `PropertyRead` instruction | F8 | Axis 5, ~150 LOC | Medium |

### Phase 5: Execution Optimization (F11)

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `TemplateChip` trait | F11 | Axis 6, ~100 LOC | Medium |
| Equivalence test harness | F11 | Axis 6, ~200 LOC | Medium |
| Built-in template: Transfer | — (app-side pattern) | Axis 6, ~300 LOC | Low |

### Phase 6: Proof Composition

| Item | Prerequisite | Scope | Priority |
|---|---|---|---|
| `ProofAggregator` trait | — | Axis 7, ~100 LOC | Low |
| Layered STARK aggregation | — | Axis 7, ~1000 LOC | Low |
| Recursive SNARK wrapper | — | Axis 7, ~TBD | Future |

---

## 13. Developer Experience Summary

| Extension | Who | Effort | What They Write |
|---|---|---|---|
| Use core IR | App developer | **Trivial** | `.tab` files (DSL) |
| Use standard library opcodes | App developer | **Trivial** | `include StdLib;` in instruction set |
| Use standard precompiles | App developer | **Trivial** | `@ecdsa_verify(...)` in DSL |
| Define custom opcode | Framework contributor | **Low** | `OpcodeSpec` impl (~100 LOC) |
| Define custom precompile | App developer (ZK) | **Medium** | `PrecompileChip` impl (~300-500 LOC) |
| Define template chip | Framework contributor | **Medium** | `TemplateChip` impl (~300 LOC) + equivalence tests |
| Define custom VC | App developer (ZK) | **High** | `VectorCommitment` + AIR chip (~500-1000 LOC) |
| Define property opening | App developer (ZK) | **High** | `PropertyOpening` + AIR chip (~500 LOC) |

The graduated complexity ensures that 80% of applications need only DSL-level knowledge, while the remaining 20% (like Lighter DEX) can go as deep as custom AIR chips while still benefiting from the framework's composition infrastructure.

---

## 14. Completeness Checklist

Requirements for supporting arbitrary ZK applications:

| Requirement | Axis | Mechanism | Status |
|---|---|---|---|
| Custom computations | 1 | `OpcodeSpec` trait | Designed |
| Bitwise operations | 1 | Standard library opcode | Designed |
| Wide arithmetic (U128) | 1 | `WideMul` opcode | Designed |
| Signature verification | Precompile | `EcdsaVerifyChip` | Designed |
| Custom hash functions | Precompile | `KeccakChip`, etc. | Designed |
| App-defined chips | 2 | `define_chip_set!` + `include` | Designed |
| App-defined buses | 2 | `BusId` newtype + `define_bus!` | Designed |
| Automatic trace routing | 3 | `TraceContributor` trait | Designed |
| Custom state commitment | 4 | `VectorCommitment` trait | Designed |
| Ordered data queries | 5 | `PropertyOpening` trait | Designed |
| Optimized tx execution | 6 | `TemplateChip` trait | Designed |
| Proof aggregation | 7 | `ProofAggregator` trait | Designed |
| Cross-tx invariants | Core | Continuation token pattern | Already possible |
| Oracle integration | Precompile | SigVerify on oracle data | Already possible |
| Timestamp/clock | Core | Batch parameter | Already possible |
| Data availability | External | `tabula-da` crate | Future |
| L1 bridge / escape hatch | External | `tabula-bridge` crate | Future |

---

## 15. API Stability and Upgrade Compatibility

### 15.1 Stability Tiers

Every public API in Tabula is classified into one of three stability tiers. Apps can gauge their upgrade risk based on which tiers they depend on.

| Tier | Guarantee | What's Included |
|------|-----------|-----------------|
| **S (Stable)** | Breaking changes only on major versions. Deprecation warnings for 2 minor versions before removal. | `Value`, `ValueType`, `CellKey`, `TableId`, `ColId`, `RowKey`, `Transaction`, `Batch`, `Program`, `TxTypeDef`, `ProgramBudgets`, `TabulaError`, `Hasher`, `SigVerifier`, `StateSnapshot`, `BatchResult`, `ExecutionResult` |
| **A (Extension)** | May evolve across minor versions, but with migration path documented. Additive changes (new trait methods with defaults) are non-breaking. | `ChipSpec`, `TraceContributor`, `VectorCommitment`, `PropertyOpening`, `OpcodeSpec`, `PrecompileHandler`, `TemplateChip`, `ProofAggregator`, `define_chip_set!`, `define_bus!`, `define_instruction_set!`, `BusId`, `WitnessStore`, `PrecompileId`, `VcId` |
| **I (Internal)** | No stability guarantee. May change between any release. | Individual chip implementations (`ExecutionChip`, `PoseidonChip`, etc.), column struct layouts, gadget internals, `trace/orchestration.rs`, constraint details, `stark/prover.rs` internals |

**Rule**: An app that depends only on Tier S + Tier A APIs survives all minor version upgrades without code changes. Apps that import Tier I types (e.g., to reuse a gadget inside a custom chip) accept the risk of breakage.

### 15.2 Plonky3 Re-export Strategy

Apps building custom chips need p3 types (`BabyBear`, `AB::Expr`, `RowMajorMatrix`, etc.). Rather than requiring apps to depend on specific p3 crate versions (which creates diamond dependency conflicts), Tabula re-exports everything through a stable prelude:

```rust
// tabula-machine/src/prelude.rs (Tier A)
pub use p3_air::{Air, AirBuilder, BaseAir};
pub use p3_baby_bear::BabyBear;
pub use p3_field::{Field, PrimeField32, PrimeCharacteristicRing};
pub use p3_matrix::dense::RowMajorMatrix;

// Tabula-specific re-exports
pub use crate::air::{InteractionAirBuilder, ChipSpec, BusId};
pub use crate::trace::{TraceContributor, WitnessStore, TraceEntry};
pub use crate::state::{VectorCommitment, VcId, PropertyOpening};
```

When Tabula upgrades p3 (e.g., 0.4 → 0.5), the prelude adapts internally. Apps using the prelude see no breakage as long as their code doesn't depend on p3-internal details.

### 15.3 Bus Signature Versioning

Bus signatures (the field layout of LogUp fingerprints) are the primary interoperability contract between chips. If a bus signature changes, all chips on that bus must update simultaneously.

**Policy**:
- Core bus signatures (BusId 0-99) are **Tier A** — stable within minor versions.
- App bus signatures (BusId 100+) are app-controlled — no Tabula stability guarantee needed.
- Bus signature changes are documented in release notes with migration instructions.

### 15.4 Upgrade Scenarios

| Scenario | App Impact | Why |
|----------|-----------|-----|
| Tabula adds new core opcode | **None** | App's `define_instruction_set!` inherits new opcodes via `include TabulaCore` |
| Tabula adds new core chip | **None** | App's `define_chip_set!` inherits new chips via `include TabulaCoreAir` |
| Tabula improves SSMC/SMT internals | **None** | Internal chip changes are Tier I; VC trait interface is unchanged |
| Tabula changes bus signature | **Recompile** | App's custom chips on affected bus need constraint updates |
| Tabula changes `VectorCommitment` trait | **Minor update** | Tier A — new methods have defaults; app may need to implement new optional methods |
| Tabula upgrades Plonky3 version | **None** (if using prelude) | Prelude absorbs p3 version changes |
| App adds new precompile | **None to Tabula** | Purely additive in app crate |
| App adds new custom VC | **None to Tabula** | Purely additive in app crate |

### 15.5 Versioning Contract

```
tabula v0.X.Y
         │ │
         │ └── Patch: bug fixes only. Zero app impact.
         │
         └──── Minor: Tier S unchanged. Tier A additive only. Tier I may change.

tabula v1.0.0+
         │
         └──── Major: Tier S may break (with deprecation cycle). Tier A may break.
```

---

## 16. Summary

Tabula's extensibility architecture provides **seven orthogonal extension axes** connected by **LogUp buses** as the universal composition interface. The **Zero-Modification Principle** ensures that applications never fork or modify Tabula's codebase — all customization is purely additive in the app's own crate.

The framework requires ~1,450 LOC of one-time prerequisites (§1.4) to enable this model. After that investment, the graduated complexity model allows:

- **80% of apps**: DSL-only (`.tab` files) — zero Rust code, zero ZK knowledge needed
- **15% of apps**: Standard library + precompiles — import and configure, minimal Rust
- **5% of apps** (like Lighter DEX): Custom chips, VCs, and templates — full AIR development, but still composing with the framework rather than forking it

The ~5-10% proving overhead (compared to a fully purpose-built circuit) is the composability tax that buys development velocity, upgrade safety, and ecosystem interoperability.
