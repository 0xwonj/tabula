# Implementation Conventions

> Rules governing all code in the Tabula workspace.
> For architecture decisions, see [architecture.md](./design/architecture.md).

---

## 1. Language & Toolchain

- **Rust Edition 2024**, `rustc 1.93+`
- **No `unwrap()` / `expect()` in library crates** — return `Result<T, TabulaError>`
  - `expect()` allowed only for proven invariants, with a message stating the invariant: `expect("block 0 always exists")`
  - `unwrap()` / `expect()` allowed freely in test code
- **`panic!` / `unreachable!`** only for statically provable invariants, never for external input
- **`BTreeMap` / `BTreeSet` only** in `tabula-executor` — no `HashMap` (determinism)
- **`BTreeMap`** preferred everywhere unless performance profiling justifies `HashMap`

---

## 2. Lints & Diagnostics

Every library crate's `lib.rs` must have:

```rust
#![warn(missing_docs)]
#![deny(unused)]
```

- `cargo clippy --all-targets` must pass with **zero warnings**
- `cargo doc` must produce **zero warnings** (no broken links, no referencing private types)
- No `#![allow(unused)]` overrides — dead code must be removed, not suppressed

---

## 3. Feature Flags

| Flag | Crate | Purpose |
|------|-------|---------|
| `stark` | `tabula-proof`, `tabula-commitment` | Plonky3 deps (p3-air, p3-field, p3-baby-bear, p3-matrix) |
| `mock` | `tabula-core` | Blake3-based test doubles (`MockHasher`, etc.) |

```bash
# Run proof tests (needs stark feature):
cargo test -p tabula-proof --features stark

# Run all workspace tests (default features only):
cargo test --workspace
```

Gate modules behind features, not individual items:

```rust
#[cfg(feature = "stark")]
pub mod air;

#[cfg(any(feature = "mock", test))]
pub mod mock;
```

---

## 4. Naming

| Kind | Convention | Example |
|------|-----------|---------|
| Types | `PascalCase` | `CellKey`, `TableSchema` |
| Traits | `PascalCase`, adjective-like | `Hasher`, `StateSnapshot` |
| Modules | `snake_case`, singular | `overlay`, `gadgets` |
| Constants | `SCREAMING_SNAKE_CASE` | `SSMC_STANDARD_WIDTH` |
| Test functions | `test_<behavior>` or `valid_`/`invalid_` prefix | `test_read_your_writes`, `valid_single_entry` |
| Error variants | `PascalCase`, descriptive noun phrase | `ArithmeticOverflow`, not `Overflow` |
| Chips | `Global<Name>Chip<W>` | `GlobalSsmcChip<3>` |
| Column structs | `<Name>Cols<T, W>` | `GlobalSsmcCols<T, 3>` |

---

## 5. Type Safety

### 5.1 Newtype Identifiers

Domain identifiers are newtypes with a full derive stack:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TableId(pub u32);
```

Required derives on identifier types: `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash` + serialization.

### 5.2 Type Aliases

Use type aliases for semantic clarity, not abstraction:

```rust
pub type Slot = u16;
pub type Digest = [u8; 32];
pub type LogicalTime = u64;
```

### 5.3 API Ergonomics

- Prefer `&[T]` over `&Vec<T>` in function signatures
- Prefer `&str` over `&String`
- Derive standard traits on all data types: `Debug, Clone, PartialEq, Eq`
- Add `Borsh` + `Serde` derives only on types that cross serialization boundaries

---

## 6. Error Handling

### 6.1 Library Crates

One error enum per crate boundary, defined with `thiserror`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TabulaError {
    #[error("table not found: {0:?}")]
    TableNotFound(TableId),

    #[error("NF-1 unique-read: instructions {first} and {second} both read ...")]
    NfUniqueRead { first: usize, second: usize, table: TableId, col: ColId },
}
```

Rules:
- Variants carry enough context to diagnose (include `CellKey`, `TableId`, indices)
- NF violations prefixed `Nf*` with rule ID (`NfUniqueRead` for NF-1)
- No `anyhow` in library crates
- Propagate with `?` — avoid `map_err` unless changing error type

### 6.2 CLI / Binary Crate

- Use `anyhow::Result<T>` at command handler boundaries
- Add context with `.context("...")` or `.with_context(|| format!(...))`

### 6.3 Compiler Errors (`tabula-lang`)

Custom `CompileError` struct with `kind`, `span`, and `message` — supports source-aware display.

---

## 7. Import Ordering

Group imports in this order, separated by blank lines:

```rust
// 1. Standard library
use std::collections::BTreeMap;

// 2. External crates (alphabetical)
use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;

// 3. Workspace crates
use tabula_core::{CellKey, TableId, Value};
use tabula_ir::Instruction;

// 4. Current crate
use crate::overlay::Overlay;

// 5. Parent/sibling modules
use super::columns::GlobalSsmcCols;
```

Re-export public API at crate root in `lib.rs`:

```rust
pub use state::id::{CellKey, ColId, Digest, RowKey, TableId, TxTypeId};
pub use state::value::{Value, ValueType, zero_value};
```

---

## 8. Module Organization

### 8.1 Chip 3-File Pattern

Every AIR chip lives in its own directory with three files:

```
chips/ssmc/
├── mod.rs       # Module docs + re-exports
├── columns.rs   # #[repr(C)] column struct + width constants
├── air.rs       # Chip struct + BaseAir + Air impl (constraints)
└── trace.rs     # Witness → RowMajorMatrix + tests
```

`mod.rs` re-exports the public surface:

```rust
pub use air::GlobalSsmcChip;
pub use columns::{GlobalSsmcCols, SSMC_STANDARD_WIDTH};
pub use trace::{SsmcEntry, generate_ssmc_trace};
```

### 8.2 Trait Organization

Core traits live in `tabula-core/src/traits/`, one file per concern:

```
traits/
├── mod.rs       # Re-exports
├── crypto.rs    # Hasher, SigVerifier, MembershipScheme
├── state.rs     # StateSnapshot, StaticTableProvider
└── codec.rs     # ValueCodec
```

All traits: `pub`, always `Send + Sync`.

### 8.3 Gadget Organization

Reusable AIR constraint helpers in `air/gadgets/`:

```
gadgets/
├── mod.rs       # Re-exports
├── boolean.rs   # constrain_is_real_prefix()
├── integer.rs   # U64Limbs, IsZero, StrictIneq
└── mem.rs       # null canonicality, mem_read, mem_write
```

Gadget functions are `pub(crate)`.

---

## 9. Visibility

| Scope | Use for | Example |
|-------|---------|---------|
| `pub` | Crate public API, re-exported in `lib.rs` | `pub struct TableId` |
| `pub(crate)` | Internal helpers, test fixtures, gadgets | `pub(crate) fn cell(...)` |
| `pub(super)` | Rare — parent-module-only access | Avoid unless necessary |
| private | Module-local implementation | `fn constrain_booleans(...)` |

Test doubles and fixture functions use `pub(crate)`, never `pub`.

---

## 10. Const Generics & `repr(C)`

### 10.1 Value Width Parameter

Chips are generic over value width `W`:

```rust
pub struct GlobalSsmcChip<const W: usize>;

// Width classes: W=1 (Bool), W=3 (U64/I64), W=8 (Digest)
pub const SSMC_STANDARD_WIDTH: usize = ssmc_width::<3>();
```

Compile-time width calculation via const functions:

```rust
pub const fn ssmc_width<const W: usize>() -> usize {
    num_cols::<GlobalSsmcCols<u8, W>, u8>()
}
```

### 10.2 Column Structs

Column structs use `#[repr(C)]` for zero-copy slice borrowing:

```rust
#[repr(C)]
pub struct GlobalSsmcCols<T, const W: usize> {
    pub is_real: T,
    pub table_id: T,
    pub value: [T; W],
    // ...
}
```

Rules:
- All fields must be `T` or `[T; N]` — no padding allowed
- `num_cols::<C, T>()` asserts no padding at compile time
- Nested gadget structs (`U64Limbs<T>`, `IsZero<T>`) must also be `#[repr(C)]`

### 10.3 Unsafe

The only `unsafe` in the codebase is `borrow_cols()` / `borrow_cols_mut()` for zero-copy column access.

Rules for any `unsafe` block:
1. **Precondition asserts** before the block (length, alignment)
2. **`// SAFETY:` comment** explaining why it's sound
3. **Minimized scope** — the unsafe block contains only the pointer cast

---

## 11. Documentation

### 11.1 Module Docs (`//!`)

Every module file starts with `//!` docs explaining purpose:

```rust
//! Trace generation for the GlobalSSMC chip.
//!
//! Converts witness data (SSMC entries per column) into a
//! `RowMajorMatrix<BabyBear>` trace.
```

### 11.2 Public Item Docs (`///`)

Every public item gets `///` docs. Functions include arguments and errors:

```rust
/// Execute a transaction body against an overlay.
///
/// # Errors
/// Returns `InterpreterError` if any instruction fails.
pub fn execute<S: StateSnapshot>(...) -> Result<...> { ... }
```

### 11.3 Constraint References

AIR constraint functions reference the spec section they implement:

```rust
//! Constraints (proof-spec §4.2):
//! 1. Boolean fields (4): is_real, is_first, is_last, tc_changed
//! 2. `is_real` prefix: monotonic 1→0
```

### 11.4 Do NOT Document

- Private functions (unless complex)
- Test functions
- Obvious getter/setter methods

---

## 12. Testing

Canonical layer rules and reviewer guidance live in
[`testing-architecture.md`](./design/testing-architecture.md).

### 12.1 Organization

- **Inline tests** at file bottom: `#[cfg(test)] mod tests { ... }`
- **Crate-private white-box helpers** in `src/testing/`
- **Crate-local black-box helpers** in `tests/common/`
- **Cross-crate black-box infra** in `tabula-testing`
- **Property-based tests** in `proptest_tests.rs` (separate file)
- **Integration tests** in `tests/` directory (CLI crate)

### 12.2 Test Naming

Group tests by validity:

```rust
// ── Valid traces ──
#[test]
fn valid_single_entry() { ... }

#[test]
fn valid_multiple_segments() { ... }

// ── Invalid traces ──
#[test]
fn invalid_broken_is_real_prefix() { ... }
```

### 12.3 Test Doubles

Pattern: minimal structs implementing traits, scoped `pub(crate)`:

```rust
pub(crate) struct AlwaysValidSig;
impl SigVerifier for AlwaysValidSig {
    fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8]) -> Result<(), TabulaError> {
        Ok(())
    }
}
```

Bundle test environment setup in a helper:

```rust
pub(crate) fn test_env() -> BatchEnv<'static> { ... }
```

### 12.4 Debug Constraint Checker

AIR chip tests use `debug_check()` to verify constraints on concrete traces:

```rust
debug_check(&GlobalSsmcChip::<3>, &trace).expect("valid trace should pass");
debug_check(&GlobalSsmcChip::<3>, &bad_trace).expect_err("invalid should fail");
```

### 12.5 Property-Based Tests

Use `proptest` for overlay semantics and consistency properties. Keep in a dedicated file to avoid cluttering unit tests.

---

## 13. Architecture Rules

- **Executor has zero crypto deps** — all crypto via trait objects (`&dyn Hasher`, `&dyn SigVerifier`)
- **Immutable snapshot** — `StateSnapshot` is read-only; `Overlay` handles mutations
- **Stage 1 / Stage 2 boundary** — `ExecutionResult` is the handoff; no leaking internals
- **Crate dependency direction**: `core` ← `ir` ← `executor` ← `proof` (never reverse)
- **No IR dependency from proof** — `tabula-proof` depends on `tabula-core` only

---

## 14. Null / Absence Semantics

Null is **not** a value type. The `Value` enum has four variants: `U64`, `I64`, `Bool`, `Bytes32`. Absence is represented separately:

- **State layer:** `Option<Value>` — `None` = absent cell
- **IR Read:** `Read { dst_val, dst_is_null, table, col, row }` — produces two SSA slots
- **IR Write:** `Write { table, col, row, src_val, src_is_null }` — `src_is_null = true` is a **delete**
- **Canonical zero:** When `val_is_null = true`, the value slot MUST contain `zero_value(T)`

**Guard pattern:**
```
Assert(Eq(Slot(is_null_slot), Literal(Bool(false))))  // ensure key exists before use
```

**Rationale:** No SQL-style three-valued logic. In a ZK constraint system every boolean must resolve to 0 or 1; a separate `is_null` flag is cheaper than a tagged union.

---

## 15. Determinism & Canonical Ordering

### 15.1 Determinism Principle

Identical inputs MUST produce identical outputs at **every layer** — executor, trace generation, serialization, hashing. This is not optional; the proof system assumes deterministic re-execution.

Rules:
- **No `HashMap` / `HashSet`** — iteration order is non-deterministic. Use `BTreeMap` / `BTreeSet` everywhere, not just in the executor
- **No floating point** — IEEE 754 rounding varies across platforms
- **No system time, no randomness** — no `std::time`, no `rand` in library crates
- **Iteration order is semantic** — if two crates iterate the same collection, they must see the same order. This is why `BTreeMap` is mandatory
- **Serialization must be deterministic** — Borsh is deterministic by design; serde JSON with `BTreeMap` preserves key order

### 15.2 Canonical Ordering: `(table, col, row)`

`CellKey` field order determines BTreeMap iteration, Merkle paths, trace sorting, and serialization across the entire project:

```rust
pub struct CellKey {
    pub table: TableId,  // first
    pub col: ColId,      // second
    pub row: RowKey,     // third
}
```

- `CellKey` derives `Ord` — the derived implementation uses **field declaration order**. Changing field order is a **breaking change** that silently corrupts sorting, hashing, and proofs
- `GlobalSortedMem` extends this to `(table, col, row, timestamp)`
- Events within a tx are ordered by instruction index; across txs by tx index
- All test assertions that compare ordered output depend on this ordering

---

## 16. Spec-Code Traceability

### 16.1 Normative Specs

| Spec | Governs | Update when |
|------|---------|-------------|
| `spec/proof-spec.md` | AIR constraints, LogUp buses, trace layout, STARK integration | Any proof/chip change |
| `spec/semantics-spec.md` | IR contract, NF rules, execution model, state semantics | Any IR/executor change |

These specs are **authoritative**. If spec and code disagree, fix the code (unless the spec has a known bug — fix both and note the correction).

### 16.2 Reference Docs

| Doc | Governs | Update when |
|-----|---------|-------------|
| `design/architecture.md` | Crate structure, dependency DAG, data flow | Any structural change |
| `TODO.md` | Roadmap, milestone status | Milestone start/completion |

### 16.3 Code References

AIR constraint code references the spec section it implements:

```rust
//! Constraints (proof-spec §4.2):      // chip-level
fn constrain_ordering(...)  // proof-spec §4.2.3
```

IR and executor code references semantics-spec:

```rust
// semantics-spec §2.3 NF-1: unique read per (t, c, r)
if seen_reads.contains(&key) { return Err(NfUniqueRead { ... }); }
```

### 16.4 Spec-First Rule

New features that affect the proof system or IR semantics need a **spec section before implementation**. The workflow:

1. Draft the spec section (constraints, semantics, or both)
2. Review the spec for soundness
3. Implement to match the spec
4. Verify spec-code correspondence

---

## 17. Development Workflow

### 17.1 Design-Doc Lifecycle

```
Draft docs/<name>.md → Implement → Mark "Status: COMPLETE" → Move to docs/archive/
```

- New milestones get a design doc in `docs/` capturing intent and rationale
- Design docs are NOT specs — they capture the *plan* for a specific milestone
- Completed docs move to `docs/archive/` to preserve historical context

### 17.2 Cross-Crate Change Propagation

Changes in upstream crates ripple downstream:

```
tabula-core → tabula-ir → tabula-executor → tabula-proof
                        → tabula-lang
                                           → tabula-cli
```

A type change in `tabula-core` can cause:
- Compilation errors in all downstream crates
- Unused-import warnings (caught by `#![deny(unused)]`)
- Broken doc links across crate boundaries
- Test failures in crates you didn't touch

**Rule**: Always verify the full workspace after any change, not just the crate you modified.

### 17.3 Verification Checklist

Before considering any change complete:

```bash
cargo test --workspace                         # all default-feature tests
cargo test -p tabula-proof --features stark     # proof tests (behind feature flag)
cargo clippy --all-targets                      # zero warnings
cargo doc                                       # zero warnings
```

Additionally:
- [ ] Affected specs updated (proof-spec, semantics-spec, architecture)
- [ ] Cross-references in docs still valid (no links to deleted/moved files)
- [ ] No dead code introduced (`#![deny(unused)]` catches most, but check re-exports)
