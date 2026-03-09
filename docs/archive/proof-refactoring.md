# Proof Crate Refactoring Plan

> Reference document for post-M10 (or inter-milestone) refactoring.
> Not a feature design — purely structural improvements to existing code.

**Status**: Draft
**Scope**: `tabula-proof` crate only (~8,000 LOC src + ~5,300 LOC tests)

---

## Table of Contents

0. [Industry Patterns (SP1 / OpenVM)](#0-industry-patterns-sp1--openvm)
1. [The Operation Pattern](#1-the-operation-pattern)
2. [Column Sub-Structs](#2-column-sub-structs)
3. [Builder Trait Composition](#3-builder-trait-composition)
4. [ExecutionChip Decomposition](#4-executionchip-decomposition)
5. [Debug Checker Consolidation](#5-debug-checker-consolidation)
6. [Constraint Naming](#6-constraint-naming)
7. [Minor Cleanups](#7-minor-cleanups)
8. [Execution Order](#8-execution-order)

---

## 0. Industry Patterns (SP1 / OpenVM)

Both SP1 (Succinct) and OpenVM (Axiom/Scroll) are production zkVMs built on
Plonky3/BabyBear. They face the same problems we do — large column structs,
cross-chip bus wiring, duplicated constraint logic — and solve them with
converging patterns.

### Key patterns summary

| Pattern | SP1 | OpenVM | Tabula (current) | Applicable? |
|---------|-----|--------|-------------------|-------------|
| **Operation-as-sub-struct** | `AddOperation<T>` with `populate()+eval()` | `SubAir<AB>` trait with GAT context | Gadgets are standalone functions | **Yes — §1** |
| **Deep column nesting** | 2–4 levels (Operation embeds Word, carries, etc.) | 2–3 levels (Io + AuxCols) | 1 level (gadget structs only) | **Yes — §2** |
| **Builder trait composition** | `WordAirBuilder + MemoryAirBuilder + ...` | `InteractionBuilder` + bus bridges | Single `InteractionAirBuilder` | **Partial — §3** |
| **AlignedBorrow derive macro** | `#[derive(AlignedBorrow)]` | `#[derive(AlignedBorrow)]` | Manual `borrow_cols()` fn | Nice-to-have |
| **Adapter/Core split** | `VmAirWrapper<Adapter, Core>` | Same | N/A (not a VM) | No |
| **Chip-per-file** | One file per chip/opcode | Same | 3-file split (columns/air/trace) | See §4 |
| **Enum dispatch derive** | `#[derive(MachineAir)]` on enum | Manual | Manual `dispatch_tabula_air!` | Low priority |

### What we should adopt

1. **The Operation pattern** (§1) — the single highest-leverage change.
   Unifies column sub-structs, constraint logic, and trace generation into
   one cohesive unit. Directly eliminates the "gadgets are free functions
   operating on flat fields" problem.

2. **Deeper column nesting** (§2) — natural consequence of §1.
   Operations embed their own auxiliary columns, so chip column structs
   shrink from 20–30 flat fields to 8–12 operation fields.

3. **Builder trait composition** (§3) — domain-specific extension traits
   on `AirBuilder` for memory, range checks, etc. Replaces raw
   `builder.send(AirInteraction{...})` with `builder.send_memory(...)`.

### What we should NOT adopt

- **Adapter/Core split**: This is a VM-specific pattern (decode→execute→memory).
  Tabula's chips are state-machine chips, not instruction-execution chips.
- **AlignedBorrow derive macro**: Our `borrow_cols()` does the same thing in
  ~30 lines. A proc macro adds build complexity for marginal ergonomic gain.
  Revisit if the crate grows past 15+ chips.
- **Single-file chips**: SP1 puts columns+air+trace in one file. Our 3-file
  split is already well-established and matches the spec structure. Keep it.

---

## 1. The Operation Pattern

### Problem (root cause of "spaghetti")

Our current gadgets are **disembodied**: the column sub-struct (`IsZero<T>`,
`StrictIneq<T>`, `LimbHalves<T>`) lives in `gadgets/integer.rs`, the
constraint function (`constrain_is_zero`) lives next to it, but the trace
population (`populate()`) lives on `impl IsZero<BabyBear>`. These three
concerns are linked, but the call sites look like:

```rust
// In air.rs — caller manually passes fields
constrain_is_zero(builder, table_diff_expr, &local.table_diff_iz);
constrain_strict_ineq(builder, &local.key, &next_key, &local.key_ordering);
constrain_limb_halves(builder, local.key.limb0.clone().into(), &local.key_l0_halves);
// ... repeat for every limb half, every ineq diff, etc.

// In trace.rs — caller manually populates each field
local.table_diff_iz.populate(table_diff);
local.key_ordering.populate(key_a, key_b);
local.key_l0_halves.populate(key_limb0);
local.key_l1_halves.populate(key_limb1);
local.ordering_diff0_halves.populate(diff0);
local.ordering_diff1_halves.populate(diff1);
```

This creates three problems:
1. **Column structs are flat** — the chip has 22–30 fields because the gadget
   columns and their associated range-check columns are separate fields.
2. **Constraint code is verbose** — each gadget invocation requires 3–5 lines
   of plumbing.
3. **Trace code duplicates the same population sequence** across 3+ chips.

### Solution: Operation = Columns + Constraints + Trace

Following SP1's pattern, each reusable gadget becomes an **Operation**:
a `#[repr(C)]` struct that owns its auxiliary columns and provides both
`populate()` and `eval()` methods.

```rust
// gadgets/segment.rs

/// (t,c) segment change detection with its own auxiliary columns.
/// Embeddable in any global table column struct.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SegmentDetectionOp<T> {
    pub table_diff_iz: IsZero<T>,
    pub col_diff_iz: IsZero<T>,
    pub tc_changed: T,
}

impl SegmentDetectionOp<BabyBear> {
    /// Fill all witness columns.
    pub fn populate(&mut self, cur_table: u32, next_table: u32, cur_col: u16, next_col: u16) {
        let t_diff = BabyBear::new(next_table) - BabyBear::new(cur_table);
        let c_diff = BabyBear::new(next_col as u32) - BabyBear::new(cur_col as u32);
        self.table_diff_iz.populate(t_diff);
        self.col_diff_iz.populate(c_diff);
        let same = (t_diff == BabyBear::ZERO && c_diff == BabyBear::ZERO);
        self.tc_changed = if same { BabyBear::ZERO } else { BabyBear::ONE };
    }
}

impl SegmentDetectionOp<()> {
    /// Emit AIR constraints. Call site: one line.
    pub fn eval<AB: AirBuilder>(
        builder: &mut AB,
        local: &SegmentDetectionOp<AB::Var>,
        table_id: AB::Expr,
        next_table_id: AB::Expr,
        col_id: AB::Expr,
        next_col_id: AB::Expr,
    ) {
        let table_diff = next_table_id - table_id;
        let col_diff = next_col_id - col_id;
        constrain_is_zero(builder, table_diff, &local.table_diff_iz);
        constrain_is_zero(builder, col_diff, &local.col_diff_iz);
        let table_same: AB::Expr = local.table_diff_iz.is_zero.clone().into();
        let col_same: AB::Expr = local.col_diff_iz.is_zero.clone().into();
        builder.assert_eq(
            local.tc_changed.clone(),
            AB::Expr::ONE - table_same * col_same,
        );
    }
}
```

Usage in a chip:

```rust
// columns.rs — one field instead of five
pub struct GlobalSsmcCols<T, const W: usize> {
    pub is_real: T,
    pub table_id: T,
    pub col_id: T,
    pub key: U64Limbs<T>,
    pub value: [T; W],
    pub is_first: T,
    pub is_last: T,
    pub hash_chain: HashChainOp<T>,         // 24 cols → 1 field
    pub key_ordering: RangeCheckedIneqOp<T>, // 7 cols → 1 field
    pub segment: SegmentDetectionOp<T>,      // 5 cols → 1 field
    pub lex: LexDirectionOp<T>,              // 3 cols → 1 field
    pub mult_witness: T,
    pub segment_is_touched: T,
    pub key_rc: U64RangeCheckOp<T>,          // 4 cols → 1 field
}
// 14 named fields, down from 22

// air.rs — one call instead of ten
SegmentDetectionOp::eval(builder, &local.segment, table_id, next_table, col_id, next_col);
RangeCheckedIneqOp::eval(builder, &local.key_ordering, &local.key, &next_key, is_real);
HashChainOp::eval(builder, &local.hash_chain, is_first, ...);

// trace.rs — one call instead of six
local.segment.populate(cur_table, next_table, cur_col, next_col);
local.key_ordering.populate(key_a, key_b);
local.hash_chain.populate(is_first, domain_tag, table_id, col_id, &key, &value, &prev_acc);
```

### Why `impl Op<()>` for eval?

SP1 uses `impl Op<F>` with `pub fn eval<AB>(...)` as a static method.
OpenVM uses a `SubAir<AB>` trait with a GAT for context. Both work.

We use `impl Op<()>` (unit type) because:
- `eval` doesn't need `self` state — all data comes from columns
- Avoids orphan rule issues with `impl<AB> SubAir<AB> for Op`
- Consistent with our existing `impl IsZero<BabyBear>` for `populate()`

Alternatively, `eval` can be a free function. The important thing is
**co-location**: the constraint function lives in the same file as the
column struct and populate function.

### Proposed operations

| Operation | Columns | Used by | Replaces |
|-----------|---------|---------|----------|
| `SegmentDetectionOp<T>` | 5 | SSMC, Merge, SortedMem | `table_diff_iz + col_diff_iz + tc_changed` + `constrain_same_key_detection()` |
| `LexDirectionOp<T>` | 3 | All 4 global tables | `lex_diff_is_table + lex_table_diff + lex_col_diff` + inline constraints |
| `HashChainOp<T>` | 24 | SSMC, Merge | `hash_acc + perm_input` + `constrain_hash_chain_input()` |
| `RangeCheckedIneqOp<T>` | 7 | SSMC, Merge, SortedMem | `StrictIneq + 2×LimbHalves` + 3 constraint calls + 4 RC sends |
| `U64RangeCheckOp<T>` | 4 | SSMC, Merge, SortedMem, Exec | `2×LimbHalves` + decomposition constraint + 4 RC sends |
| `CmpOp<T>` | 17 | Execution | M10-B1 cmp_* fields + cmp constraint function |
| `MulCarryOp<T>` | 4 | Execution | M10-C1 mul_c0/c1 fields + mul carry constraints |
| `DivModOp<T>` | 13 | Execution | M10-C2 divmod_* fields + divmod constraints |
| `ComEmptyOp<T>` | 25 | ColumnMeta | M10-B4 empty_perm_* fields + com_empty constraints |

### File organization

```
gadgets/
├── mod.rs              # re-exports
├── boolean.rs          # constrain_is_real_prefix (unchanged)
├── integer.rs          # U64Limbs, IsZero, StrictIneq, LimbHalves (unchanged)
│                       # + U64RangeCheckOp, RangeCheckedIneqOp (NEW)
├── mem.rs              # constrain_mem_read/write, constrain_null_canon (unchanged)
├── segment.rs          # SegmentDetectionOp, LexDirectionOp (NEW)
└── hash_chain.rs       # HashChainOp (NEW)
```

Execution-specific operations (`CmpOp`, `MulCarryOp`, `DivModOp`) live
in `chips/execution/ops/` since they are not shared across chips.

---

## 2. Column Sub-Structs

This section is a direct consequence of §1. Once operations own their
columns, the chip column structs shrink naturally.

### Before vs After

**GlobalSsmcCols** (current: 22 named fields):
```
is_real, table_id, col_id, key, value[W], is_first, is_last,
hash_acc[8], key_ordering, table_diff_iz, col_diff_iz, tc_changed,
perm_input[16], mult_witness, segment_is_touched,
key_l0_halves, key_l1_halves, ordering_diff0_halves, ordering_diff1_halves,
lex_diff_is_table, lex_table_diff, lex_col_diff
```

**GlobalSsmcCols** (after: 14 named fields):
```
is_real, table_id, col_id, key, value[W], is_first, is_last,
hash_chain: HashChainOp,                    // absorbs 24 cols
key_ordering: RangeCheckedIneqOp,           // absorbs 7 cols
segment: SegmentDetectionOp,                // absorbs 5 cols
lex: LexDirectionOp,                        // absorbs 3 cols
mult_witness, segment_is_touched,
key_rc: U64RangeCheckOp                     // absorbs 4 cols
```

**ExecutionCols** (current: 26+ named fields, growing with M10):
```
is_real, tx_index, 12 opcode flags, 2 arith sub-selectors,
is_access, clk, tau, tau_limbs, access_*, src1_val, src2_val, cond_val,
src1_sel[16], src2_sel[16], cond_sel[16], src1_is_null,
carry0, carry1, slots[16][W], slot_is_null[16], slot_written[16],
cmp_* (17 fields), hash_* (24 fields), mul_* (4 fields), divmod_* (13 fields),
access_r_l0_halves, access_r_l1_halves, tau_l0_halves, tau_l1_halves
```

**ExecutionCols** (after: ~20 named fields):
```
is_real, tx_index, opcodes: OpcodeSelectors,
is_access, clk, tau, tau_limbs,
access: AccessLog,
src1_val, src2_val, cond_val,
operand_link: OperandLinkage,               // absorbs sel arrays + src1_is_null
carry0, carry1,
ssa: SsaSlots,                              // absorbs slots + null + written
cmp: CmpOp,                                 // absorbs 17 M10-B1 fields
hash: HashInputOp,                          // absorbs 24 M10-B2 fields
mul: MulCarryOp,                            // absorbs 4 M10-C1 fields
divmod: DivModOp,                           // absorbs 13 M10-C2 fields
access_rc: U64RangeCheckOp,
tau_rc: U64RangeCheckOp
```

### Impact table

| Chip | Current fields | After | Flat cols (unchanged) |
|------|---------------|-------|----------------------|
| SSMC | 22 | 14 | 56 |
| Merge | 25 | 16 | 63 |
| SortedMem | 30 | 20 | 49 |
| ColumnMeta | 14 | 10 | 56 |
| Execution | 26+ | ~20 | 238 |

Flat column count does NOT change — only the logical grouping.

---

## 3. Builder Trait Composition

### Problem

Every bus interaction is a raw `builder.send(AirInteraction { values: vec![...], ... })`.
This is verbose, error-prone (field order must match receiver), and obscures intent.

### Solution: domain-specific extension traits

Following SP1's `MemoryAirBuilder` / `WordAirBuilder` pattern:

```rust
// builder.rs

/// Memory bus convenience methods.
pub trait MemoryBusBuilder: InteractionAirBuilder {
    /// Send a memory access tuple to the Memory bus.
    fn send_memory(
        &mut self,
        t: Self::Expr, c: Self::Expr,
        r: &U64Limbs<Self::Var>,
        tau: &U64Limbs<Self::Var>,
        is_write: Self::Expr,
        val: &[Self::Var],
        val_is_null: Self::Expr,
        mult: Self::Expr,
    ) { ... }  // default impl constructs AirInteraction
}

/// Range check bus convenience methods.
pub trait RangeCheckBusBuilder: InteractionAirBuilder {
    /// Send a 15-bit half-limb to the RangeCheck bus.
    fn send_range_check(&mut self, val: Self::Expr, mult: Self::Expr) { ... }

    /// Send both halves of a LimbHalves.
    fn send_limb_halves(&mut self, halves: &LimbHalves<Self::Var>, mult: Self::Expr) {
        self.send_range_check(halves.lo.clone().into(), mult.clone());
        self.send_range_check(halves.hi.clone().into(), mult);
    }
}

// Blanket impls
impl<AB: InteractionAirBuilder> MemoryBusBuilder for AB {}
impl<AB: InteractionAirBuilder> RangeCheckBusBuilder for AB {}
```

Usage in chip code:

```rust
// Before (5 lines):
let values = vec![t, c, r0, r1, r2, tau0, tau1, tau2, is_write, v0, v1, v2, is_null];
let mult = local.is_access.clone().into();
builder.send(AirInteraction { values, multiplicity: mult, kind: InteractionKind::Memory });

// After (1 call):
builder.send_memory(t, c, &local.access_r, &local.tau_limbs, is_write, &local.access_val, is_null, mult);
```

### Scope

| Trait | Methods | Used by |
|-------|---------|---------|
| `MemoryBusBuilder` | `send_memory`, `receive_memory` | Execution, SortedMem |
| `RangeCheckBusBuilder` | `send_range_check`, `send_limb_halves` | All 5 chips |
| `PoseidonBusBuilder` | `send_poseidon_perm` | SSMC, Merge, Execution |
| `SsmcBusBuilder` | `send_ssmc_membership`, `receive_ssmc_membership` | SortedMem, SSMC |
| `CommitmentBusBuilder` | `send_commitment_verif` | SSMC, Merge, ColumnMeta |

**NOT proposed**: A "mega" `TabulaAirBuilder` supertrait. Each bus trait is
independent, and chips import only what they use.

---

## 4. ExecutionChip Decomposition

### Problem

`execution/air.rs` is 826+ lines and growing with M10. With M10 Cmp/Mul/DivMod/Hash,
it will approach 1,200 lines.

### Proposed structure

With the operation pattern from §1, the ExecutionChip naturally decomposes:

```
execution/
├── mod.rs              # re-exports (ExecutionChip, columns, Opcode, etc.)
├── columns.rs          # ExecutionCols struct — now ~60 lines thanks to ops
├── trace.rs            # trace generation
├── air.rs              # Air::eval() orchestration (~100 lines)
├── structural.rs       # is_real, one-hot, clock, access log, slot carry
├── linkage.rs          # operand-to-slot selectors, value/null matching
└── ops/                # execution-specific operations
    ├── mod.rs
    ├── cmp.rs          # CmpOp (columns + constraints + populate)
    ├── mul.rs          # MulCarryOp
    └── divmod.rs       # DivModOp
```

`air.rs` becomes pure orchestration:

```rust
impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for ExecutionChip<W> {
    fn eval(&self, builder: &mut AB) {
        let (local, next) = /* borrow_cols */;

        structural::constrain_booleans(builder, local);
        structural::constrain_opcode_one_hot(builder, local);
        structural::constrain_clock(builder, local, next);
        structural::constrain_slot_carry(builder, local, next);

        // Per-opcode constraints via operations
        CmpOp::eval(builder, &local.cmp, &local.src1_val, &local.src2_val, local.op_cmp.clone());
        MulCarryOp::eval(builder, &local.mul, &local.src1_val, &local.src2_val, ...);
        DivModOp::eval(builder, &local.divmod, ...);

        linkage::constrain_operand_selectors(builder, local);
        linkage::send_memory_bus(builder, local);
        linkage::send_range_checks(builder, local);
    }
}
```

Each file stays under 300 lines. The `ops/` subdirectory follows the SP1
pattern of per-opcode operation files.

---

## 5. Debug Checker Consolidation

### Problem

`debug.rs` (767 lines) has:

1. **Row-iteration loop duplicated 3 times**: `debug_check_with_preprocessed`,
   `debug_check_all`, and `evaluate_chip_with_preprocessed`.

2. **`debug_check_all` silently ignores preprocessed traces**: Hardcodes
   `empty_preprocessed()`. Poseidon chip will produce silently wrong results.

3. **LogUp accumulation partially duplicated** between two functions.

### Fix

#### 5.1 Extract row-iteration helper

```rust
fn eval_rows<F, A>(
    air: &A,
    trace: &RowMajorMatrix<F>,
    preprocessed: Option<&RowMajorMatrix<F>>,
    mut on_row: impl FnMut(usize, &mut DebugConstraintBuilder<'_, F>),
)
```

All three public functions become 5-line wrappers.

#### 5.2 Fix `debug_check_all` signature

Add `preprocessed: Option<&RowMajorMatrix<F>>` parameter.

#### 5.3 Extract LogUp accumulation

```rust
fn accumulate_fingerprints<F: Field>(
    interactions: &[RecordedInteraction<F>],
    alpha: F, beta: F,
    bus_filter: Option<InteractionKind>,
) -> F
```

### Impact

~200 lines removed. Silent preprocessed bug fixed.

---

## 6. Constraint Naming

### Problem

Debug output: `"constraint 42 failed on row 7: value = 123456789"`.
No name — must count constraints to find the failing one.

### Solution

```rust
pub struct DebugConstraintBuilder<'a, F: Field> {
    // ... existing fields
    current_group: &'static str,  // NEW
}

impl DebugConstraintBuilder<'_, F> {
    pub fn enter_group(&mut self, name: &'static str) {
        self.current_group = name;
    }
}
```

Output becomes: `"[constrain_key_ordering] constraint 3 failed on row 7"`.

For production `AirBuilder` impls, `enter_group` is a no-op (defined as
a default method on a `NamedAirBuilder` supertrait).

### Priority

Lower than §1–§5. Incrementally adoptable per chip.

---

## 7. Minor Cleanups

### 7.1 `ChipMeta` trait → method on `TabulaAir`

7 identical impls returning `&'static str`. Only consumer is debug error messages.
Replace with `impl TabulaAir { fn chip_name(&self) -> &'static str }`.

### 7.2 `BaseAir::width` dispatch sync

Add `// NOTE: Keep in sync with dispatch_tabula_air!` comment.

### 7.3 `Interaction<F>` / `VirtualPairCol<F>` — document as forward stubs

Currently defined but unused. Add `/// **Not yet wired** — forward stub for
M11 permutation trace generator.`

### 7.4 `constrain_is_real` wrapper removal

5 chips each have an identical 1-line wrapper. Call `constrain_is_real_prefix()`
directly.

---

## 8. Execution Order

### Phase R1: Operation pattern foundation

Introduce the Operation pattern for shared gadgets. This is the single
highest-leverage change — everything else builds on it.

| # | Item | Est. effort |
|---|------|-------------|
| R1.1 | `SegmentDetectionOp` (cols + populate + eval) | Small |
| R1.2 | `LexDirectionOp` (cols + populate + eval) | Small |
| R1.3 | `HashChainOp` (cols + populate + eval) | Medium |
| R1.4 | `RangeCheckedIneqOp` (cols + populate + eval + RC sends) | Small |
| R1.5 | `U64RangeCheckOp` (cols + populate + RC sends) | Small |
| R1.6 | Migrate SSMC/Merge/SortedMem/ColumnMeta columns to use ops | Medium |
| R1.7 | Remove `constrain_is_real` wrappers (5 chips) | Trivial |

**Validation**: `cargo test -p tabula-proof` + `cargo clippy` after each step.

### Phase R2: Builder traits + Execution decomposition

| # | Item | Est. effort |
|---|------|-------------|
| R2.1 | `RangeCheckBusBuilder` trait + blanket impl | Small |
| R2.2 | `MemoryBusBuilder` trait + blanket impl | Small |
| R2.3 | `PoseidonBusBuilder` + other bus traits | Small |
| R2.4 | Migrate chip `send_*` functions to builder traits | Medium |
| R2.5 | ExecutionChip file split (structural/linkage/ops) | Medium |
| R2.6 | `CmpOp`, `MulCarryOp`, `DivModOp` as execution-local ops | Medium |

### Phase R3: Debug + polish

| # | Item | Est. effort |
|---|------|-------------|
| R3.1 | Debug checker row-loop consolidation + preprocessed fix | Small |
| R3.2 | Constraint naming (`enter_group`) | Small |
| R3.3 | `ChipMeta` removal + minor cleanups (§7) | Trivial |

### Not planned (conscious decisions)

- **`AlignedBorrow` derive macro**: Our `borrow_cols()` works. Macro adds
  build-time proc-macro dependency for marginal ergonomic gain.
- **Adapter/Core split**: VM-specific pattern. Our chips don't decode instructions.
- **Single-file chips**: Our 3-file split is well-established. The Operation
  pattern already reduces per-file size enough.
- **`GlobalTableHeader<T>`** sub-struct for `(is_real, table_id, col_id)`:
  Only 3 fields; `local.is_real` is cleaner than `local.header.is_real`.
- **`define_cols!` macro**: Obscures `#[repr(C)]` layout, hurts IDE navigation.
- **OpenVM-style GAT context**: The associated-type gymnastics in `SubAir<AB>`
  (`type AirContext<'a> where Self: 'a, AB: 'a, AB::Var: 'a, AB::Expr: 'a`)
  add complexity without proportional benefit at our scale. SP1's simpler
  static-method pattern is a better fit.
