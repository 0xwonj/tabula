# AIR Chip Architecture — Code Organization Guide

## 1. Context and Goals

Tabula's proof system has 6-8 AIR chips, each with different complexity levels.
This document defines the canonical code organization for `tabula-proof/src/`,
informed by best practices from SP1, Valida, and OpenVM — adapted for Tabula's
domain (state commitments + memory consistency, not a general-purpose VM).

**Design principles:**
- Valida's strict separation (columns / constraints / trace)
- SP1's pragmatic flexibility (simple chip = one file, complex = directory)
- SP1's gadget pattern (columns + populate + eval bundled per gadget)
- No over-abstraction (no SubAir traits, no adapter/core split, no proc macros)

---

## 2. Planned Chips and Complexity

| Chip | Spec ref | Columns | Constraints | Complexity |
|------|----------|---------|-------------|------------|
| **ColumnMeta** | §4.2 | 25 | boolean, prefix, lex order, binding | Simple (~200L) |
| **GlobalSortedMem** | §8.2-8.8 | ~30+ | lex (t,c,r,τ), same_key, mem consistency, init, write-set | Complex (~600L) |
| **GlobalSSMC** | §4.2 SSMC | ~20 | sorted uniqueness, hash chain, boundary | Complex (~500L) |
| **GlobalMerge** | §4.2 merge | ~20 | source encoding, merge correctness, in_new, hash | Medium (~400L) |
| **Execution** | §6, §8.7 | ~20+ | SSA slots, clock, is_access, op dispatch | Complex (~500L) |
| **SmtPath** | §4.2 SMT | ~15 | 64-level Merkle path, leaf hash | Medium (~300L) |
| **RangeCheck** | §4.2.R | ~3 | lookup table, counter | Simple (~100L) |

**Threshold:** chips under ~300 lines → single file; chips over ~300 lines → directory.

---

## 3. Target Module Structure

```
tabula-proof/src/
├── lib.rs                      # Crate root, feature gates
├── statement.rs                # ApplyBatchStatement (public inputs)
├── trace.rs                    # Witness data types (InitRow, AccessRow, ColumnWitness, BatchWitness)
├── witness.rs                  # WitnessGenerator: ExecutionResult → BatchWitness
│
└── air/
    ├── mod.rs                  # Re-exports
    ├── columns.rs              # borrow_cols, borrow_cols_mut, num_cols
    ├── bus.rs                  # InteractionKind, InteractionDirection, InteractionDecl
    ├── debug.rs                # DebugConstraintBuilder, debug_check
    │
    ├── gadgets/                # Reusable constraint gadgets (SP1 Operations pattern)
    │   ├── mod.rs              # Re-exports
    │   ├── boolean.rs          # is_real_prefix, assert_bool (moved from gadgets.rs)
    │   ├── integer.rs          # U64Limbs: decompose, range-check, comparison, same_key
    │   └── lex_order.rs        # Lexicographic ordering with inverse witness helpers
    │
    └── chips/
        ├── mod.rs              # TabulaAir enum (dispatch all chips)
        ├── column_meta.rs      # Simple chip — single file
        ├── range_check.rs      # Simple chip — single file
        │
        ├── sorted_mem/         # Complex chip — directory
        │   ├── mod.rs          # Re-exports
        │   ├── columns.rs      # GlobalSortedMemCols<T>, WIDTH
        │   ├── air.rs          # Air<AB> impl (constraints from §8.3-8.8)
        │   └── trace.rs        # generate_sorted_mem_trace()
        │
        ├── ssmc/               # Complex chip — directory
        │   ├── mod.rs
        │   ├── columns.rs      # GlobalSSMCCols<T>
        │   ├── air.rs          # Air<AB> impl (sorted uniqueness, hash chain)
        │   └── trace.rs        # generate_ssmc_trace()
        │
        ├── merge/              # Medium chip — directory
        │   ├── mod.rs
        │   ├── columns.rs      # GlobalMergeCols<T>
        │   ├── air.rs          # Air<AB> impl (3-way merge)
        │   └── trace.rs        # generate_merge_trace()
        │
        ├── execution/          # Complex chip — directory
        │   ├── mod.rs
        │   ├── columns.rs      # ExecutionCols<T>
        │   ├── air.rs          # Air<AB> impl (SSA, clock, dispatch)
        │   └── trace.rs        # generate_execution_trace()
        │
        └── smt_path/           # Medium chip — directory
            ├── mod.rs
            ├── columns.rs      # SmtPathCols<T>
            ├── air.rs          # Air<AB> impl (Merkle verification)
            └── trace.rs        # generate_smt_path_trace()
```

---

## 4. Per-Chip File Convention

### 4.1 Simple chip (single file)

For chips with ≤ ~300 total lines where columns + constraints + trace gen
all fit comfortably together. Examples: ColumnMeta, RangeCheck.

```
chips/column_meta.rs
  ├── ColumnMetaCols<T>           #[repr(C)] column struct
  ├── COLUMN_META_WIDTH           const
  ├── ColumnMetaChip              struct (unit or with config)
  ├── impl BaseAir<F>             width()
  ├── impl Air<AB>                eval() — all constraints
  ├── generate_column_meta_trace  fn(witness) → RowMajorMatrix
  └── #[cfg(test)] mod tests      valid/invalid trace tests
```

### 4.2 Complex chip (directory, 3+1 files)

For chips with > ~300 total lines. Separation into columns / air / trace.
Tests stay in the same directory as a 4th file or inline in air.rs.

```
chips/sorted_mem/
  ├── mod.rs          Re-exports: pub use columns::*, air::*, trace::*
  ├── columns.rs      Column struct + WIDTH + gadget structs (if chip-specific)
  ├── air.rs          Chip struct + BaseAir + Air (constraints only)
  └── trace.rs        generate_*_trace() (witness → concrete BabyBear matrix)
```

**Why these three specifically?**

| File | Concern | Depends on | Changes when... |
|------|---------|------------|-----------------|
| `columns.rs` | Data shape | Nothing | Column layout changes |
| `air.rs` | Math correctness | `columns.rs`, `gadgets/` | Constraints change |
| `trace.rs` | Data transformation | `columns.rs`, `trace.rs` (witness types) | Witness format changes |

This is Valida's proven 3-file split. Each file has a single reason to change.

### 4.3 Naming conventions

- Column struct: `{ChipName}Cols<T>` (e.g., `GlobalSortedMemCols<T>`)
- Width const: `{CHIP_NAME}_WIDTH` (e.g., `GLOBAL_SORTED_MEM_WIDTH`)
- Chip struct: `{ChipName}Chip` (e.g., `GlobalSortedMemChip`)
- Trace fn: `generate_{chip_name}_trace()` (e.g., `generate_sorted_mem_trace()`)
- Tests: `#[cfg(test)] mod tests` within each file, or dedicated `tests.rs`

---

## 5. Gadget Pattern (SP1 Operations Style)

Gadgets are reusable constraint building blocks shared across chips.
Each gadget bundles three things:

### 5.1 Structure

```rust
// gadgets/integer.rs

/// 30+30+4 BabyBear limb decomposition of u64 (§4.2.R).
///
/// Embeddable in any chip's column struct.
#[repr(C)]
pub struct U64Limbs<T> {
    pub x0: T,  // [0, 2^30)
    pub x1: T,  // [0, 2^30)
    pub x2: T,  // [0, 16)
}

impl U64Limbs<BabyBear> {
    /// Populate witness from a u64 value.
    pub fn populate(&mut self, val: u64) {
        self.x0 = BabyBear::new((val & 0x3FFF_FFFF) as u32);
        self.x1 = BabyBear::new(((val >> 30) & 0x3FFF_FFFF) as u32);
        self.x2 = BabyBear::new((val >> 60) as u32);
    }
}

// Constraint functions are free functions generic over AB.
/// Constrain that limbs reconstruct to the expected value.
pub fn eval_u64_decomposition<AB: AirBuilder>(
    builder: &mut AB,
    limbs: &U64Limbs<AB::Var>,
    expected: AB::Expr,
) { ... }

/// Constrain strict inequality: a < b (via borrow-chain gadget).
pub fn eval_u64_strict_lt<AB: AirBuilder>(
    builder: &mut AB,
    a: &U64Limbs<AB::Var>,
    b: &U64Limbs<AB::Var>,
    borrows: &[AB::Var; 3],  // auxiliary witness
) { ... }
```

### 5.2 Usage in chips

```rust
// chips/sorted_mem/columns.rs
use crate::air::gadgets::integer::U64Limbs;

#[repr(C)]
pub struct GlobalSortedMemCols<T> {
    pub is_real: T,
    pub table_id: T,
    pub col_id: T,
    pub row_key: U64Limbs<T>,     // Embedded gadget struct
    pub timestamp: T,
    // ...
}
```

```rust
// chips/sorted_mem/air.rs
use crate::air::gadgets::integer::eval_u64_decomposition;

impl<AB: AirBuilder> Air<AB> for GlobalSortedMemChip {
    fn eval(&self, builder: &mut AB) {
        // Use the gadget's constraint function
        eval_u64_decomposition(builder, &local.row_key, expected_key);
    }
}
```

```rust
// chips/sorted_mem/trace.rs
impl GlobalSortedMemCols<BabyBear> {
    // Use the gadget's populate method
    row.row_key.populate(access.key.row.0);
}
```

### 5.3 Gadget inventory (planned)

| Gadget | File | Columns | Used by |
|--------|------|---------|---------|
| `is_real_prefix` | `boolean.rs` | 0 | All chips |
| `U64Limbs` | `integer.rs` | 3 | SortedMem, SSMC, Merge |
| `u64_strict_lt` | `integer.rs` | 3 (borrows) | SSMC (sorted), SortedMem (lex) |
| `same_key` | `integer.rs` | 2 (diff, inv) | SortedMem (§8.6) |
| `lex_order` | `lex_order.rs` | 2 (diff_inv × 2) | ColumnMeta, SortedMem |

---

## 6. TabulaAir Dispatch Enum

All chips register in a single enum for multi-chip proving (M9):

```rust
// chips/mod.rs
pub enum TabulaAir {
    ColumnMeta(ColumnMetaChip),
    GlobalSortedMem(GlobalSortedMemChip),
    GlobalSsmc(GlobalSsmcChip),
    GlobalMerge(GlobalMergeChip),
    Execution(ExecutionChip),
    SmtPath(SmtPathChip),
    RangeCheck(RangeCheckChip),
}
```

`BaseAir` and `Air` delegate via match. New chip = new variant + two match arms.

---

## 7. Trace Generation Interface

Each chip provides a standalone `generate_*_trace()` function:

```rust
// Signature pattern (not a trait — each chip has different inputs)
pub fn generate_sorted_mem_trace(
    columns: &[ColumnWitness<impl FieldHasher>],
) -> RowMajorMatrix<BabyBear>;

pub fn generate_column_meta_trace(
    metas: &[ColumnMeta],
) -> RowMajorMatrix<BabyBear>;

pub fn generate_ssmc_trace(
    columns: &[ColumnWitness<impl FieldHasher>],
) -> RowMajorMatrix<BabyBear>;
```

**Why not a unified trait?** Each chip needs different slices of `BatchWitness`.
A trait would either force passing the entire witness (wasteful coupling) or
require type-erased inputs (complexity). Free functions are simpler and testable.

A thin orchestrator at proving time (M9) calls each function:

```rust
// Future M9 code (not implemented now)
pub fn generate_all_traces(witness: &BatchWitness<H>) -> Vec<RowMajorMatrix<BabyBear>> {
    vec![
        generate_column_meta_trace(&witness.column_metas),
        generate_sorted_mem_trace(&witness.columns),
        generate_ssmc_trace(&witness.columns),
        generate_merge_trace(&witness.columns),
        // ...
    ]
}
```

---

## 8. Cross-Chip Interactions (LogUp Bus)

### 8.1 Bus types (already defined, M6)

```rust
pub enum InteractionKind {
    Memory,           // Execution ↔ GlobalSortedMem
    SsmcMembership,   // GlobalSortedMem init ↔ GlobalSSMC
    MergeCompleteness,// GlobalMerge ↔ GlobalSSMC + WriteSet
    ColumnMetaJoin,   // Any chip ↔ ColumnMeta
    RangeCheck,       // Any chip → RangeCheck table
}
```

### 8.2 Declaration pattern (M9 wiring)

Each chip declares its bus participation as data. No runtime dispatch:

```rust
impl GlobalSortedMemChip {
    pub fn interactions() -> Vec<InteractionDecl> {
        vec![
            InteractionDecl {
                kind: InteractionKind::Memory,
                direction: InteractionDirection::Receive,
                column_indices: vec![/* t, c, r, τ, is_write, val... */],
                multiplicity_index: /* is_real × (1 - is_init) */,
            },
            InteractionDecl {
                kind: InteractionKind::ColumnMetaJoin,
                direction: InteractionDirection::Send,
                column_indices: vec![/* t, c */],
                multiplicity_index: /* is_first_in_group */,
            },
        ]
    }
}
```

---

## 9. Testing Strategy

### 9.1 Per-chip test pattern

Every chip has tests that:
1. Build a valid trace → `debug_check` passes
2. Build invalid traces (one constraint violated) → `debug_check` fails

```rust
#[cfg(test)]
mod tests {
    use crate::air::debug::debug_check;

    #[test]
    fn valid_basic_trace() {
        let trace = generate_sorted_mem_trace(&test_witness());
        debug_check(&GlobalSortedMemChip, &trace).expect("valid");
    }

    #[test]
    fn invalid_unsorted_keys() {
        let mut trace = generate_sorted_mem_trace(&test_witness());
        // Swap two rows to break sorting
        // ...
        debug_check(&GlobalSortedMemChip, &trace).expect_err("should fail");
    }
}
```

### 9.2 Test location

- **Simple chips**: `#[cfg(test)] mod tests` at bottom of single file
- **Complex chips**: `#[cfg(test)] mod tests` at bottom of `air.rs`
  (tests verify constraints, which is air.rs's concern)
- **Trace-only tests** (encode/decode roundtrips): in `trace.rs`

### 9.3 Gadget tests

Gadgets get their own tests with minimal synthetic traces:

```rust
// gadgets/integer.rs
#[cfg(test)]
mod tests {
    #[test]
    fn u64_limbs_roundtrip() { ... }

    #[test]
    fn u64_strict_lt_valid() { ... }

    #[test]
    fn u64_strict_lt_equal_fails() { ... }
}
```

---

## 10. Completed Refactoring (M6)

The following refactoring has been applied:

| Before | After | Rationale |
|--------|-------|-----------|
| `air/gadgets.rs` (single file) | `air/gadgets/` directory (`mod.rs` + `boolean.rs`) | Extensible for `integer.rs`, `lex_order.rs` |
| `air/chips/column_meta.rs` (single file) | `air/chips/column_meta/` directory (3-file split) | Reference pattern for all future chips |

ColumnMeta was split proactively (despite being under 300 lines) to serve as the
canonical working example of the 3-file pattern.

### Adding a new chip

1. Create `chips/{chip_name}/` directory
2. Add `columns.rs`, `air.rs`, `trace.rs`, `mod.rs`
3. Add variant to `TabulaAir` enum in `chips/mod.rs`
4. Add `pub mod {chip_name}` to `chips/mod.rs`
5. Add gadgets to `gadgets/` as needed

---

## 11. Rejected Alternatives

### 11.1 One crate per chip (Valida style)
- Tabula has 6-8 chips, not 20+. Crate-per-chip adds Cargo.toml overhead
  without meaningful isolation benefit. Module boundaries suffice.

### 11.2 SubAir trait (OpenVM style)
- Formal `Io`/`Aux` separation for gadgets. Valuable when 10+ chips share
  the same gadget with different I/O shapes. Tabula's gadget reuse is lower
  (3-4 chips per gadget). Free functions are simpler and equally composable.

### 11.3 Proc macro for AlignedBorrow
- SP1 uses `#[derive(AlignedBorrow)]`. Our manual `borrow_cols` is 15 lines
  and works identically. No proc macro dependency justified for 6-8 chips.

### 11.4 Unified `ChipTrace` trait
- Forces all chips to accept the same witness type signature. Each chip
  actually needs different data. Free functions with specific parameters
  are more honest and testable.

### 11.5 ~~All chips in directories from day one~~ (adopted)
- Originally rejected as premature. Adopted during M6 refactoring so
  ColumnMeta serves as the canonical reference pattern for all future chips.

---

## 12. Summary

```
Guiding rule:
  Columns define shape.
  Air defines truth.
  Trace fills values.
  Gadgets are shared tools.
  Tests verify everything.
```

- **Simple chips** → single file (columns + air + trace together)
- **Complex chips** → directory with `columns.rs` / `air.rs` / `trace.rs`
- **Gadgets** → `gadgets/` directory, SP1 pattern: struct + populate + eval
- **Bus** → `InteractionKind` enum + `InteractionDecl` data
- **Dispatch** → `TabulaAir` enum in `chips/mod.rs`
- **Tests** → `debug_check` against valid/invalid traces per chip
