# Phase 1: SDK-Ready Architecture Plan

> **Version**: 1.0
> **Date**: 2026-03-07
> **Status**: Draft
> **Depends on**: extensibility-architecture.md (v0.4), implementation-workplan.md (v3.0), master-roadmap.md (v1.0)
> **Scope**: Internal framework changes that make Tabula SDK-ready. No public SDK crate yet.

---

## 1. Executive Summary

Phase 1 makes Tabula's internals **extensible by design** so that a future `tabula-sdk` crate can
be extracted mechanically. No public SDK is published yet — Phase 1 focuses on ensuring every
extension point (custom chips, precompiles, commitment schemes, buses) works correctly when
exercised from an external crate position.

**Guiding principle**: Build it right internally first, dog-food it with internal precompiles in
Phase 2, then extract the public SDK in Phase 3. This avoids premature API stabilization.

### 1.1 What Phase 1 Delivers

| Capability | Current State | Phase 1 Target |
|------------|---------------|----------------|
| Custom chip registration | `with_chip()` works but AnyRap bounds require importing 7 p3 types | `prelude` module re-exports all needed types |
| Custom bus definition | `BusId(u16)` is open, but `define_bus!` docs are internal | Public `define_bus!` + bus ID allocation guide |
| Precompile execution | No `Call` instruction | `Instruction::Call` + `PrecompileHandler` trait + executor dispatch |
| Precompile proving | No mechanism | `PrecompileBus` + `op_precompile` selector in ExecutionChip |
| Plonky3 isolation | 7+ p3 crates in public API | `tabula-machine::prelude` re-exports all needed p3 types |
| ColumnState extension | Closed 2-variant enum | Stays closed (justified); documented extension path |
| ChipExtension packaging | Not implemented | `ChipExtension` trait for bundling chips + witness + buses |
| Integration test | No external-chip test | Test proving a custom chip registered via `with_chip()` |

### 1.2 What Phase 1 Does NOT Do

- Publish a `tabula-sdk` crate (Phase 3)
- Implement concrete precompiles like ECDSA/Keccak (Phase 2 dogfooding)
- Open `ColumnState` to external variants (deferred; justified in §7)
- Template chips / compiled execution (Phase 4+)

---

## 2. Feature Definitions

### F1. Plonky3 Re-Export Consolidation (`tabula-machine::prelude`)

**Problem**: An external chip author currently needs:

```rust
use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;
use p3_field::{Field, PrimeCharacteristicRing};
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder};
```

If Tabula bumps p3 from 0.4 → 0.5, every external chip breaks because they import `p3-air 0.4` directly.

**Solution**: A `prelude` module in `tabula-machine` that re-exports all types an external chip needs.

```rust
// crates/machine/src/prelude.rs
pub use p3_air::{Air, BaseAir, AirBuilder, PairBuilder};
pub use p3_baby_bear::BabyBear;
pub use p3_field::{Field, PrimeCharacteristicRing, AbstractField};
pub use p3_field::extension::BinomialExtensionField;
pub use p3_matrix::dense::RowMajorMatrix;
pub use p3_uni_stark::{
    ProverConstraintFolder, SymbolicAirBuilder, VerifierConstraintFolder,
};

// Tabula's own types needed for chip authoring
pub use tabula_stark::air::builder::InteractionAirBuilder;
pub use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
pub use tabula_stark::air::interaction::{BusId, core_buses};
pub use tabula_stark::chips::{ChipId, ChipIdAllocator, ChipSpec};
pub use tabula_stark::debug::DebugConstraintBuilder;
pub use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};

// Machine-level types
pub use crate::{AnyRap, MachineBuilder, TabulaMachine, TabulaStarkConfig};
pub use crate::config::EF4;
pub use crate::prove::RapProverFolder;
pub use crate::verify::RapVerifierFolder;
```

**External chip authors then write**:

```rust
use tabula_machine::prelude::*;  // single import, version-pinned to tabula
```

**Impact**: Tabula can upgrade p3 versions in a minor release without breaking downstream — only the re-exports need updating, not external crates.

**Files changed**:
- `crates/machine/src/prelude.rs` — NEW (~40 lines)
- `crates/machine/src/lib.rs` — add `pub mod prelude;`

---

### F2. Precompile System

The precompile system has four sub-components that span four crates.

#### F2a. IR: `Instruction::Call` variant

```rust
// In crates/ir/src/instruction.rs

/// Opaque precompile identifier. Core precompile IDs are 0–99;
/// app-defined IDs use 100+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ...)]
pub struct PrecompileId(pub u16);

/// A single Tabula IR instruction.
pub enum Instruction {
    // ... existing 13 variants ...

    /// Call an app-defined precompile.
    ///
    /// The executor dispatches to a `PrecompileHandler` at runtime.
    /// The ExecutionChip sends a tuple on PrecompileBus for proof verification.
    Call {
        /// Destination slot for the return value.
        dst: Slot,
        /// Which precompile to invoke.
        precompile_id: PrecompileId,
        /// Input arguments (slot references).
        args: Vec<ValueExpr>,
    },
}
```

**Why `Call` not `Precompile`**: Naming consistency with function-call semantics. The name
`Precompile` is an implementation detail of the proving system; `Call` is the user-facing concept.

**`map_slots` + `dst_slots` updates**: Trivial pattern-match additions (same as Select/Hash).

**Files changed**:
- `crates/ir/src/instruction.rs` — add `PrecompileId` type + `Call` variant + update `map_slots`/`dst_slots`
- `crates/ir/src/program.rs` — add `Call` to NF validation (NF-5: no Call to undefined precompile)

#### F2b. Executor: `PrecompileHandler` trait

```rust
// In crates/executor/src/precompile.rs (NEW)

use tabula_core::Value;
use tabula_core::error::TabulaError;
use tabula_ir::PrecompileId;

/// Application-defined precompile execution handler.
///
/// Registered in `ExecContext` and dispatched by the interpreter
/// when it encounters `Instruction::Call`.
pub trait PrecompileHandler: Send + Sync {
    /// Execute a precompile and return the result value.
    ///
    /// # Arguments
    /// - `id`: which precompile to run
    /// - `args`: resolved argument values
    ///
    /// # Errors
    /// Returns `TabulaError` if the precompile fails (e.g., invalid args).
    /// Failed precompile = failed transaction (same as Assert failure).
    fn execute(
        &self,
        id: PrecompileId,
        args: &[Value],
    ) -> Result<Value, TabulaError>;

    /// Whether this handler supports the given precompile ID.
    fn supports(&self, id: PrecompileId) -> bool;
}

/// No-op handler for programs that don't use precompiles.
pub struct NoPrecompiles;

impl PrecompileHandler for NoPrecompiles {
    fn execute(&self, id: PrecompileId, _args: &[Value]) -> Result<Value, TabulaError> {
        Err(TabulaError::ExecutionError {
            detail: format!("no handler for precompile {:?}", id),
        })
    }
    fn supports(&self, _id: PrecompileId) -> bool { false }
}
```

**Executor integration**:

```rust
// In ExecContext — add one field:
pub struct ExecContext<'a> {
    pub hasher: &'a dyn Hasher,
    pub static_tables: &'a dyn StaticTableProvider,
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
    pub precompiles: &'a dyn PrecompileHandler,  // NEW
}

// In execute() match arm:
Instruction::Call { dst, precompile_id, args } => {
    let resolved: Vec<Value> = args.iter()
        .map(|a| resolve_value_expr(a, &slots, params))
        .collect::<Result<_, _>>()?;
    let result = ctx.precompiles.execute(*precompile_id, &resolved)?;
    set_slot(&mut slots, *dst, result);
}
```

**Backward compatibility**: `ExecContext` gains a new required field. All existing callers
(batch.rs, tests) pass `&NoPrecompiles`. This is a minor breaking change within the workspace
but doesn't affect external consumers (tabula-executor is not a public API surface).

**Files changed**:
- `crates/executor/src/precompile.rs` — NEW (~50 lines)
- `crates/executor/src/lib.rs` — add `pub mod precompile;`
- `crates/executor/src/interpreter.rs` — add `precompiles` to ExecContext, add Call match arm
- `crates/executor/src/batch.rs` — pass `&NoPrecompiles` to ExecContext (or accept handler param)

#### F2c. AIR: PrecompileBus + ExecutionChip selector

```rust
// In crates/stark/src/air/interaction.rs — add to core_buses:
/// Execution → PrecompileChip (app-defined precompile verification).
pub const PRECOMPILE: BusId = BusId(17);

// In crates/chips/src/execution/columns.rs — add:
pub op_precompile: T,         // boolean selector
pub precompile_id: T,         // field element (PrecompileId as u16)

// In crates/chips/src/execution/air.rs — add constraint block:
// When op_precompile is active:
//   send(PRECOMPILE, [precompile_id, args..., dst_val])
```

**Width impact**: +2 columns to ExecutionChip (op_precompile selector + precompile_id).
Current width 278 → 280.

**Files changed**:
- `crates/stark/src/air/interaction.rs` — add `PRECOMPILE` bus constant
- `crates/chips/src/execution/columns.rs` — add 2 columns
- `crates/chips/src/execution/air.rs` — add precompile constraint block
- `crates/chips/src/execution/trace.rs` — populate new columns from InstructionRecord

#### F2d. Witness: PrecompileRecord in execution trace

```rust
// In crates/witness/src/trace/lowering.rs or similar:
// When lowering Instruction::Call to InstructionRecord:
//   - Set op_precompile = true
//   - Set precompile_id = id.0 as BabyBear
//   - Pack args into src slots, result into dst slot
```

**Files changed**:
- `crates/witness/src/trace/lowering.rs` — add Call case in instruction lowering

---

### F3. ChipExtension Trait

Packages a group of related chips, buses, and witness logic as a distributable unit.

```rust
// In crates/machine/src/extension.rs (NEW)

use crate::registry::ChipRegistry;
use tabula_stark::trace::contributor::WitnessStore;

/// A distributable extension that bundles chips, buses, and witness logic.
///
/// Extensions are registered via `MachineBuilder::with_extension()`.
/// Each extension is self-contained — it knows which chips to register,
/// which buses to declare, and how to populate witness data.
pub trait ChipExtension: Send + Sync {
    /// Human-readable name for diagnostics.
    fn name(&self) -> &str;

    /// Register this extension's chips into the registry.
    fn register_chips(&self, registry: &mut ChipRegistry);

    /// Populate witness data for this extension's chips.
    ///
    /// Called after execution, before trace building. The extension
    /// reads execution-level data from the store and writes chip-specific
    /// witness data back.
    fn populate_witness(&self, store: &mut WitnessStore);
}
```

**MachineBuilder integration**:

```rust
// In crates/machine/src/machine.rs:
impl MachineBuilder {
    /// Register a chip extension (bundle of chips + witness logic).
    pub fn with_extension(mut self, ext: impl ChipExtension + 'static) -> Self {
        ext.register_chips(&mut self.registry);
        self.extensions.push(Box::new(ext));
        self
    }
}
```

**Files changed**:
- `crates/machine/src/extension.rs` — NEW (~40 lines)
- `crates/machine/src/machine.rs` — add `extensions: Vec<Box<dyn ChipExtension>>` to builder
- `crates/machine/src/lib.rs` — add `pub use extension::ChipExtension;`

---

### F4. Custom Chip Integration Test

A test that proves a trivial custom chip works through the full pipeline: register → keygen → prove → verify.

```rust
// In crates/machine/tests/custom_chip.rs (NEW)

/// A no-op chip that demonstrates external registration.
/// 2 columns: is_real (boolean) + value (field element).
/// No interactions — pure constraint check that is_real * (1 - is_real) = 0.
struct TrivialChip;

impl ChipSpec for TrivialChip {
    fn chip_id(&self) -> ChipId { ChipId(200) }
    fn has_interactions(&self) -> bool { false }
}

impl<F: Field> BaseAir<F> for TrivialChip {
    fn width(&self) -> usize { 2 }
}

impl<AB: AirBuilder> Air<AB> for TrivialChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.row_slice(0);
        let is_real = local[0].clone();
        // Boolean constraint
        builder.assert_zero(is_real.clone() * (AB::Expr::ONE - is_real));
    }
}

#[test]
fn custom_chip_prove_verify() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_chip(TrivialChip)
        .build()
        .expect("setup");

    // Build traces (TrivialChip gets a 4-row trace)
    let mut traces = /* build core traces + trivial chip trace */;
    let proof = machine.prove(&traces, statement).expect("prove");
    machine.verify(&proof).expect("verify");
}
```

**Purpose**: Proves the AnyRap blanket impl, ChipRegistry, prove/verify all work for non-core chips. This is the Phase 3 success gate tested early.

**Files changed**:
- `crates/machine/tests/custom_chip.rs` — NEW (~100 lines)

---

### F5. Bus ID Allocation Documentation

Currently `BusId` is an open newtype with core buses at 5–17 and app buses recommended at 100+.
Phase 1 formalizes this:

```rust
// In crates/stark/src/air/interaction.rs — add:

/// Allocator for application-defined bus IDs.
///
/// Works identically to `ChipIdAllocator`. Core buses use 0–99;
/// app buses start at 100.
pub struct BusIdAllocator {
    next_id: u16,
}

impl BusIdAllocator {
    pub fn for_apps() -> Self { Self { next_id: 100 } }
    pub fn next(&mut self) -> BusId {
        let id = BusId(self.next_id);
        self.next_id += 1;
        id
    }
}
```

**Files changed**:
- `crates/stark/src/air/interaction.rs` — add `BusIdAllocator` (~20 lines)

---

### F6. ChipRegistry Validation Enhancement

Add validation that catches common extension errors at setup time:

```rust
// In crates/machine/src/registry.rs — enhance validate():

/// Extended validation checks:
/// 1. No duplicate ChipId
/// 2. All chips have power-of-two-compatible width (≥ 1)
/// 3. Chips with public values: at most one per registry
/// 4. NEW: Warn if chip IDs in 0–99 range are used by non-core chips
/// 5. NEW: Warn if bus IDs in 0–99 range are used by non-core buses
```

**Files changed**:
- `crates/machine/src/registry.rs` — add ID range warnings in `validate()`

---

## 3. Additional Feature Proposals

### F7. Debug Tooling for Extension Authors

Extension authors need to test their chips in isolation before integrating with the full machine.

**Already available**: `tabula_stark::debug::debug_check()` — evaluates a chip's constraints against a trace matrix and reports violations. This is already public and works for any chip implementing `Air<DebugConstraintBuilder>`.

**Proposed addition**: A `test_chip_standalone()` helper that wraps `debug_check` with common boilerplate:

```rust
// In crates/stark/src/debug/mod.rs — add:

/// Test a chip's constraints against a trace, with optional preprocessed trace.
/// Returns Ok(()) if all constraints pass, or Err with constraint violation details.
///
/// Convenience wrapper for extension chip testing.
pub fn test_chip(
    chip: &(impl BaseAir<BabyBear> + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>),
    main_trace: &RowMajorMatrix<BabyBear>,
) -> Result<(), String> {
    debug_check(chip, main_trace)
}
```

**Already effectively available** — `debug_check` is public. The proposal is mainly documentation:
add an example in the `prelude` module showing how to test a custom chip.

### F8. Preprocessed Trace Support for Custom Chips

Custom chips may need preprocessed traces (like `PoseidonChip` uses for round constants).
The current `TraceMap` supports `preprocessed: Option<RowMajorMatrix<BabyBear>>`, and
`ChipSpec::preprocessed_width()` signals the width. This already works for external chips.

**Proposed**: Document the preprocessed trace pattern in the prelude module with an example.

### F9. WitnessStore Label Namespacing Convention

To prevent label collisions between core chips and extensions:

```
Core labels:      "execution_records", "poseidon_inputs", etc. (no prefix)
Extension labels: "ext:<extension_name>:<label>"

Example: "ext:lighter-dex:ecdsa_events"
```

**Implementation**: Convention only (no code enforcement). Documented in `WitnessStore` rustdoc.

**Files changed**:
- `crates/stark/src/trace/contributor.rs` — add rustdoc convention note

---

## 4. Architectural Analysis

### 4.1 Current Extension Points (Already Working)

| Extension Point | Mechanism | Status |
|----------------|-----------|--------|
| Custom chip AIR | `impl ChipSpec + Air<AB>` → AnyRap blanket | Working |
| Custom chip registration | `MachineBuilder::with_chip()` | Working |
| Custom bus ID | `BusId(100+)` | Working |
| Custom chip ID | `ChipId(100+)` via `ChipIdAllocator` | Working |
| Phase-ordered trace generation | `TraceContributor` + `WitnessStore` | Working |
| Bus-driven dependent chips | `BusConsumer` trait | Working |
| Pluggable column commitment | `ColumnCommitment` trait | Working (Phase 2) |
| Debug constraint checking | `debug_check()` | Working |

### 4.2 Extension Points That Need Phase 1 Work

| Extension Point | Blocker | Phase 1 Fix |
|----------------|---------|-------------|
| External chip without p3 imports | 7 p3 crates in AnyRap bounds | F1 (prelude) |
| App-defined computation | No Call instruction | F2 (precompile system) |
| Extension packaging | No ChipExtension trait | F3 |
| Custom chip E2E proof test | No integration test exists | F4 |
| Bus ID allocation | Manual, collision-prone | F5 (BusIdAllocator) |

### 4.3 ColumnState: Why It Stays Closed

`ColumnState<H>` is a 2-variant enum (`Ssmc | Smt`) with 7 match sites in `tabula-commitment`.
Opening it to external variants would require:

1. Replacing the enum with `Box<dyn ColumnVc>` — requires `Any` downcasting everywhere
2. Changing `HybridVC` dispatch from pattern match to vtable — loses exhaustiveness checking
3. Making `SsmcList` and `SparseMerkleTree<H>` implement a common trait — significant redesign
4. All 7 match sites need fallback arms — error-prone

**Cost-benefit**: Opening `ColumnState` is high-cost, and the actual need is low. The
`ColumnCommitment` trait (already in `tabula-stark`) operates at the proof layer, which is where
external extensibility matters. The data layer (`ColumnState` in `tabula-commitment`) can remain
closed because:

- SSMC and SMT cover the known use cases (small vs. large columns)
- The `ColumnCommitment` trait is the real extension point — it controls chip registration,
  witness population, and trace building
- A new commitment scheme at the data layer would also need a new `HybridVC` variant —
  this is inherently a Tabula-internal change

**Decision**: Keep `ColumnState` closed. Document that adding a new data-layer commitment scheme
requires adding a variant (Tabula-internal contribution). The proof-layer `ColumnCommitment` trait
is the extension point for external developers.

---

## 5. Per-Crate Change List

### 5.1 `tabula-ir` (Instruction Set)

| File | Change | Lines |
|------|--------|-------|
| `src/instruction.rs` | Add `PrecompileId(pub u16)`, `Instruction::Call` variant, update `map_slots`/`dst_slots` | +30 |
| `src/program.rs` | Add Call to NF pass-through (no new NF rules for Call) | +5 |

### 5.2 `tabula-executor` (Runtime)

| File | Change | Lines |
|------|--------|-------|
| `src/precompile.rs` | NEW: `PrecompileHandler` trait, `NoPrecompiles` | +50 |
| `src/lib.rs` | Add `pub mod precompile;` | +1 |
| `src/interpreter.rs` | Add `precompiles` field to `ExecContext`, Call match arm | +15 |
| `src/batch.rs` | Thread `PrecompileHandler` through `BatchEnv` → `ExecContext` | +10 |

### 5.3 `tabula-stark` (Framework)

| File | Change | Lines |
|------|--------|-------|
| `src/air/interaction.rs` | Add `PRECOMPILE` bus (BusId(17)), `BusIdAllocator` | +25 |
| `src/trace/contributor.rs` | Add label namespacing convention in rustdoc | +10 |

### 5.4 `tabula-chips` (AIR Chips)

| File | Change | Lines |
|------|--------|-------|
| `src/execution/columns.rs` | Add `op_precompile`, `precompile_id` columns | +5 |
| `src/execution/air.rs` | Add precompile constraint block + bus send | +30 |
| `src/execution/trace.rs` | Populate precompile columns from record | +15 |

### 5.5 `tabula-witness` (Witness Pipeline)

| File | Change | Lines |
|------|--------|-------|
| `src/trace/lowering.rs` | Add Call → InstructionRecord lowering | +15 |

### 5.6 `tabula-machine` (Prover/Verifier)

| File | Change | Lines |
|------|--------|-------|
| `src/prelude.rs` | NEW: p3 + tabula type re-exports | +40 |
| `src/extension.rs` | NEW: `ChipExtension` trait | +40 |
| `src/machine.rs` | Add `with_extension()`, `extensions` field | +15 |
| `src/lib.rs` | Add `pub mod prelude;`, `pub use extension::*;` | +3 |
| `src/registry.rs` | Enhance `validate()` with ID range warnings | +15 |
| `tests/custom_chip.rs` | NEW: E2E test for custom chip prove/verify | +100 |

### Total: ~425 lines of new/changed code

---

## 6. Implementation Order

Tasks are ordered by dependency chain. Independent tasks can run in parallel.

```
Phase 1.1: Foundation (no dependencies)
  ├── F1: prelude module                        [independent]
  ├── F5: BusIdAllocator                        [independent]
  └── F6: registry validation enhancement       [independent]

Phase 1.2: IR + Executor (depends on nothing)
  ├── F2a: Instruction::Call in tabula-ir       [independent]
  └── F2b: PrecompileHandler in tabula-executor [depends on F2a]

Phase 1.3: AIR + Witness (depends on F2a)
  ├── F2c: PrecompileBus + ExecutionChip cols    [depends on F2a, F5]
  └── F2d: Witness lowering for Call             [depends on F2a]

Phase 1.4: Machine Integration (depends on F1, F2)
  ├── F3: ChipExtension trait                    [depends on F1]
  └── F4: Custom chip E2E test                   [depends on F1, F3]
```

**Estimated total**: ~425 LOC across 6 crates. No architectural rewrites — additive changes only.

---

## 7. Verification Plan

### 7.1 Per-Feature Tests

| Feature | Test |
|---------|------|
| F1 (prelude) | Compile test: custom chip using only `tabula_machine::prelude::*` imports |
| F2a (Call IR) | Unit test: `map_slots` + `dst_slots` for Call variant |
| F2b (PrecompileHandler) | Unit test: NoPrecompiles returns error; mock handler returns value |
| F2c (PrecompileBus) | Chip test: ExecutionChip with op_precompile active sends on bus |
| F2d (witness lowering) | Unit test: Call instruction produces correct InstructionRecord |
| F3 (ChipExtension) | Unit test: extension registers chips into MachineBuilder |
| F4 (custom chip E2E) | Integration test: prove + verify with custom chip |
| F5 (BusIdAllocator) | Unit test: sequential allocation starting at 100 |
| F6 (registry validation) | Unit test: warning on ChipId(50) from non-core chip |

### 7.2 Regression Gates

```bash
cargo check --workspace
cargo test --workspace                    # all existing tests pass
cargo test -p tabula-machine              # including new custom_chip test
cargo clippy --workspace --all-targets    # zero warnings
```

### 7.3 Success Criteria

Phase 1 is complete when:

1. A custom chip can be defined using only `tabula_machine::prelude::*` imports
2. The custom chip can be registered via `with_chip()` and proven/verified E2E
3. `Instruction::Call` dispatches to a `PrecompileHandler` in the executor
4. `ExecutionChip` sends on `PrecompileBus` when `op_precompile` is active
5. All existing 36+ machine tests pass unchanged
6. A `ChipExtension` can bundle multiple chips and register them as a unit

---

## 8. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| AnyRap bounds change on p3 update | All external chips break | F1 prelude absorbs the change |
| PrecompileBus fingerprint too narrow | Can't express complex precompile inputs | Use variable-width bus (Vec values) |
| WitnessStore label collision | Extension overwrites core data | F9 naming convention |
| ChipExtension too thin | Doesn't capture all extension needs | Phase 2 dogfooding will reveal gaps |
| ExecutionChip width growth | +2 cols per precompile selector | Only 1 `op_precompile` selector total; precompile_id distinguishes |

---

## 9. Relationship to Other Phases

```
Phase 1 (THIS)          Phase 2 (Dogfooding)       Phase 3 (SDK Extraction)
├── prelude             ├── ECDSA precompile        ├── tabula-sdk crate
├── Call instruction    ├── Keccak precompile       ├── Documentation site
├── PrecompileHandler   ├── Template chips          ├── Version stability policy
├── PrecompileBus       ├── ColumnCommitment impls  ├── Semver guarantees
├── ChipExtension       └── Performance benchmarks  └── Example app crate
├── Custom chip test
└── BusIdAllocator
```

Phase 1 is **framework plumbing** — it creates the hooks that Phase 2 exercises
and Phase 3 exposes publicly. The prelude module is the semver shield; the
ChipExtension trait is the composition unit; the precompile system is the
computation extension mechanism.
