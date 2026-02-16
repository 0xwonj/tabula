# M6: AIR Foundation — Plonky3 Constraint Infrastructure

## Philosophy

### 1. Separation of Concerns

Three distinct responsibilities, three distinct layers:

```
Witness Generation (M5, done)  →  what values go in the trace
AIR Constraints (M6-M8)        →  what relationships must hold between columns
Prover Integration (M9)        →  how constraints become a STARK proof
```

M6 establishes the middle layer: the **constraint definition** infrastructure. No prover yet — just the ability to define and debug-check AIR constraints against concrete traces.

### 2. Design Principles

- **Chips are data types, not frameworks.** Each chip is a small struct that implements `BaseAir` + `Air`. No inheritance, no trait objects, no registry. A chip has width and constraints — nothing else.

- **Columns are typed views into flat slices.** A generic `Cols<T>` struct is used for both trace generation (`T = BabyBear`) and constraint definition (`T = AB::Var`). Zero-copy reinterpretation via `Borrow<Cols<T>>`.

- **Interactions (LogUp) are declared, not embedded.** Cross-chip relationships are expressed as named send/receive channels. The actual LogUp math is handled by Plonky3's permutation argument machinery, not by our code.

- **One file per chip.** Following SP1's pattern: each chip in its own module. Chips are small (100-200 lines) and self-contained.

- **Soundness by construction.** Every constraint must map to a specific section in proof-spec.md. No "I think this is right" — every constraint has a normative reference.

### 3. What M6 Does NOT Do

- No Plonky3 prover/verifier invocation (that's M9)
- No multi-AIR batch proving setup
- No complex chips (SSMC hash chain, merge proof, GlobalSortedMem)
- No LogUp wiring between chips (declared but not activated)

## Scope

M6 delivers:
1. **Plonky3 workspace dependencies** for AIR definition
2. **Column struct pattern** — generic `<T>` with `Borrow` for zero-copy
3. **Chip trait abstraction** — `TabulaChip` wrapping Plonky3's `BaseAir` + `Air`
4. **Interaction bus types** — named channels for cross-chip LogUp
5. **`is_real` gadget** — reusable prefix constraint
6. **One concrete chip** — `ColumnMetaChip` as the simplest real chip
7. **Debug constraint checker** — verify constraints against concrete trace (no prover needed)
8. **Tests** — all constraints checked against hand-built traces

## Architecture

### Module Structure (in `tabula-proof/src/`)

```
air/
├── mod.rs              # re-exports
├── columns.rs          # column struct utilities (AlignedBorrow-style)
├── bus.rs              # interaction channel types (InteractionKind enum)
├── debug.rs            # debug constraint checker (no prover needed)
├── gadgets/
│   ├── mod.rs          # re-exports
│   └── boolean.rs      # is_real prefix constraint
└── chips/
    ├── mod.rs          # TabulaAir enum + ChipMeta trait
    └── column_meta/    # reference chip (3-file pattern)
        ├── mod.rs      # re-exports
        ├── columns.rs  # ColumnMetaCols<T>, COLUMN_META_WIDTH
        ├── air.rs      # BaseAir + Air constraints
        └── trace.rs    # generate_column_meta_trace() + tests
```

> See `docs/air-chip-architecture.md` for the full chip organization guide.

### Column Struct Pattern

```rust
/// Generic columns struct — T is BabyBear for trace gen, AB::Var for constraints.
#[repr(C)]
pub struct ColumnMetaCols<T> {
    pub is_real: T,
    pub table_id: T,
    pub col_id: T,
    pub tag: T,
    pub com_old: [T; 8],    // NativeDigest = 8 FE
    pub com_new: [T; 8],
    pub is_empty_old: T,
    pub is_empty_new: T,
    pub is_touched: T,
}
```

Access pattern:
```rust
// In eval():
let main = builder.main();
let local: &ColumnMetaCols<AB::Var> = main.row_slice(0).borrow();
let next: &ColumnMetaCols<AB::Var> = main.row_slice(1).borrow();
```

Safety: `#[repr(C)]` guarantees field layout matches the flat slice. `Borrow` impl derived via size check. This is the SP1/Valida `AlignedBorrow` pattern — we implement it manually (no proc macro dependency).

### Interaction Bus Design

```rust
/// Named interaction channels for cross-chip LogUp.
pub enum InteractionKind {
    /// Execution trace ↔ GlobalSortedMem
    Memory,
    /// GlobalSortedMem init rows ↔ GlobalSSMC (membership proof)
    SsmcMembership,
    /// GlobalMerge ↔ GlobalSSMC + WriteSet (completeness)
    MergeCompleteness,
    /// Any chip ↔ ColumnMeta (metadata join)
    ColumnMetaJoin,
    /// Execution trace → Range check table
    RangeCheck,
}
```

Each chip declares `fn interactions() -> Vec<Interaction>` with `(kind, direction, columns, multiplicity)`. M6 defines the types; M9 wires them into Plonky3's `PermutationAirBuilder`.

### Gadget Library

Reusable constraint helpers (pure functions on `AB::Expr`):

```rust
/// is_real prefix: is_real transitions 1→0 at most once
fn constrain_is_real_prefix<AB: AirBuilder>(builder: &mut AB, is_real: AB::Var, next_is_real: AB::Var);

/// Boolean: x ∈ {0, 1}
fn constrain_bool<AB: AirBuilder>(builder: &mut AB, x: AB::Var);

/// When is_real=1 and flag: conditional assertion
fn when_real_and<AB: AirBuilder>(builder: &mut AB, is_real: AB::Var, flag: AB::Expr) -> FilteredAirBuilder;

/// u64 limb range check (30+30+4): validates limb bounds
/// (declares range check interactions, not inline constraints)
fn constrain_u64_limbs<AB: AirBuilder>(builder: &mut AB, limbs: &[AB::Var; 3]);
```

### ColumnMetaChip — First Real Chip

Why ColumnMeta first:
- Simplest global table (few columns, no hash chain, no merge logic)
- Tests the full pattern: column struct, is_real, sorted ordering, boolean constraints
- Required by all other chips (GlobalSSMC, GlobalMerge look up ColumnMeta)

Constraints (from proof-spec §4.2 ColumnMeta):

1. **Boolean fields**: `is_real`, `tag`, `is_empty_old`, `is_empty_new`, `is_touched` ∈ {0,1}
2. **is_real prefix**: `is_real_{i+1} ≤ is_real_i`
3. **Strict sorted order**: When both rows real: `(t_i, c_i) <_lex (t_{i+1}, c_{i+1})`
4. **Untouched binding**: `is_touched=0 ⟹ com_new = com_old`
5. **Empty old binding**: `is_empty_old=1 ⟹ com_old = Com_empty(t,c)` (hash check — deferred to M8, just boolean for now)
6. **Empty new binding**: `is_empty_new=1 ⟹ com_new = Com_empty(t,c)` (deferred similarly)

Deferred constraints (need Poseidon chip):
- Com_empty = Poseidon(0x00 || t || c) verification
- Root inclusion proof binding (LeafDigest computation)

### Debug Constraint Checking

Instead of invoking the full STARK prover, M6 uses Plonky3's `check_constraints` utility (or a manual implementation):

```rust
/// Verify that all AIR constraints are satisfied on a concrete trace.
pub fn debug_check<F: Field, A: Air<DebugConstraintBuilder<F>>>(
    air: &A,
    trace: &RowMajorMatrix<F>,
) -> Result<(), ConstraintError>;
```

This lets us test constraints without FRI, without PCS, without any proving infrastructure. Fast iteration: change a constraint, run `cargo test`, see if traces satisfy it.

## Step-by-Step Implementation

### Step 1: Add Plonky3 AIR Dependencies

Add to workspace `Cargo.toml`:
```toml
p3-air    = "0.4"
p3-matrix = "0.4"
p3-util   = "0.4"
```

Add to `tabula-proof/Cargo.toml` stark feature:
```toml
stark = ["tabula-commitment/stark", "p3-field", "p3-baby-bear", "p3-air", "p3-matrix", "p3-util"]
```

Note: `p3-uni-stark` and `p3-fri` deferred to M9 (prover integration). M6 only needs trait definitions and matrix types.

### Step 2: Column Struct Utilities (`air/columns.rs`)

Implement the `Borrow<Cols<T>>` pattern for `[T]`:

```rust
/// Safely borrow a &[T] as &Cols<T>.
/// Panics if slice length != size_of::<Cols<T>>() / size_of::<T>().
pub fn borrow_cols<T, C>(slice: &[T]) -> &C
where
    C: ?Sized,
{
    let expected = std::mem::size_of::<C>() / std::mem::size_of::<T>();
    assert_eq!(slice.len(), expected);
    unsafe { &*(slice.as_ptr() as *const C) }
}
```

Plus a `num_cols::<C, T>() -> usize` helper.

### Step 3: Interaction Bus Types (`air/bus.rs`)

Define `InteractionKind` enum and `Interaction` struct. Pure data — no Plonky3 types yet. These are declarations that will be wired in M9.

### Step 4: Reusable Gadgets (`air/gadgets.rs`)

Implement `constrain_is_real_prefix`, `constrain_bool`, helper combinators. These are free functions generic over `AB: AirBuilder`.

### Step 5: ColumnMetaChip (`air/chips/column_meta.rs`)

- `ColumnMetaCols<T>` struct with `#[repr(C)]`
- `ColumnMetaChip` struct implementing `BaseAir<F>` + `Air<AB>`
- ~6 constraint groups in `eval()`
- Trace generation helper: `fn generate_column_meta_trace(metas: &[ColumnMeta]) -> RowMajorMatrix<BabyBear>`

### Step 6: Chip Enum (`air/chips/mod.rs`)

```rust
pub enum TabulaAir<F: Field> {
    ColumnMeta(ColumnMetaChip),
    // Future: GlobalSsmc, GlobalMerge, GlobalSortedMem, Execution, SmtPath
}
```

Delegate `BaseAir`/`Air` via match. Only one variant for now.

### Step 7: Debug Checker + Tests

- Hand-build valid ColumnMeta traces
- Verify constraints pass
- Build invalid traces (unsorted, bad booleans, wrong com_new for untouched)
- Verify constraints fail at the expected location

## Test Plan (~15 tests)

**Column struct (3)**:
- `borrow_cols` correct size
- `borrow_cols` panics on wrong size
- `num_cols` matches struct field count

**Gadgets (3)**:
- `is_real_prefix` accepts valid prefix
- `is_real_prefix` rejects 0→1 transition
- `constrain_bool` rejects non-boolean

**ColumnMetaChip (9)**:
- Valid trace: 3 real rows + padding → passes
- Boolean violation: `tag=2` → fails
- is_real prefix violation: 0→1 → fails
- Sorted order violation: `(t=1,c=0)` before `(t=0,c=1)` → fails
- Duplicate `(t,c)`: same pair twice → fails
- Untouched binding: `is_touched=0` but `com_new ≠ com_old` → fails
- Untouched binding: `is_touched=0` and `com_new = com_old` → passes
- All padding: zero real rows → passes
- Single real row: no transition constraints needed → passes

## Verification

```bash
cargo check -p tabula-proof --features stark        # Compiles
cargo test -p tabula-proof --features stark          # All tests pass
cargo clippy -p tabula-proof --features stark --all-targets  # Zero warnings
cargo test --workspace                               # No regressions
```

## Dependencies on proof-spec

| Constraint | Spec Reference | Status |
|-----------|---------------|--------|
| is_real prefix | §4.2.G | Implemented in M6 |
| ColumnMeta boolean fields | §4.2 ColumnMeta | Implemented in M6 |
| ColumnMeta strict sorted order | §4.2 ColumnMeta uniqueness | Implemented in M6 |
| Untouched com_new = com_old | §4.2 ColumnMeta constraints | Implemented in M6 |
| Com_empty = Poseidon(...) | §4.2 ColumnMeta constraints | Deferred to M8 (needs Poseidon chip) |
| Root binding (LeafDigest) | §4.2 Root binding | Deferred to M8 |
| ColumnMeta join lookups | §4.2 ColumnMeta join | Deferred to M9 (needs LogUp wiring) |

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| p3-air 0.4 API mismatch with research | Pin exact version; verify with `cargo check` before writing constraints |
| `Borrow<Cols<T>>` unsafety | Size assertion at runtime; `#[repr(C)]` enforced; alternative: index-based access as fallback |
| Debug constraint checker not available in p3-uni-stark 0.4 | Implement minimal version manually: eval constraints at each row pair, check all zero |
| Rust 2024 `gen` keyword / derive issues | Same as M5 — manual impls where needed |

## What This Enables

After M6:
- M7 can add `ExecutionChip` (instruction constraints) following the same pattern
- M8 can add `GlobalSsmcChip`, `GlobalMergeChip` (hash chain + merge constraints)
- M9 wires everything together with Plonky3 prover

The pattern is established once in M6; subsequent chips are just more `eval()` implementations.
