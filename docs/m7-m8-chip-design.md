# M7-M8 AIR Chip Architecture

> Normative design reference for M7 (gadgets + memory layer) and M8 (execution + hashing).
> Builds on M6 foundation: ColumnMetaChip, debug checker, column/chip patterns.

## D1: Width-Class via Const Generics

Standard(3) only for M7-M8. Architecture uses `<const W: usize>` on chips/columns
so wider chips (Narrow=1, Wide=8) can be added without refactoring.

```rust
pub struct GlobalSortedMemCols<T, const W: usize> { ... }
pub struct GlobalSortedMemChip<const W: usize>;
// Instantiate: GlobalSortedMemChip::<3>
```

## D2: Gadget Library (Hybrid Pattern)

Gadgets combine **embeddable column structs** + **pure constraint functions**:

| Gadget | File | Columns | Pattern |
|--------|------|---------|---------|
| `IsZero<T>` | `integer.rs` | `inv`, `is_zero` (2) | struct + fn |
| `U64Limbs<T>` | `integer.rs` | `limb0..2` (3) | struct + fn |
| `StrictIneq<T>` | `integer.rs` | `diff0..2`, `borrow0..1` (5) | struct + fn |
| `constrain_is_real_prefix` | `boolean.rs` | none | pure fn (existing) |
| `constrain_null_canon` | `mem.rs` | none | pure fn |
| `constrain_mem_read` | `mem.rs` | none | pure fn |
| `constrain_mem_write` | `mem.rs` | none | pure fn |

## D3: Same-Key Detection

Boolean witness columns `tc_changed` and `r_changed`:

- `tc_changed_i = 1` iff `(t_{i+1}, c_{i+1}) != (t_i, c_i)`
- `r_changed_i = 1` iff `r_{i+1} != r_i`
- Verified by **direct limb equality**: for each limb, `changed * limb_diff = limb_diff`.
  If `changed=0`, each limb diff must be zero. Conversely, `1 - changed` must be constrained
  via IsZero or equivalent to prevent a prover from setting `changed=0` when diffs are nonzero.

For `tc_changed`: enforce `tc_changed = 1 - is_zero(table_diff) * is_zero(col_diff)`.
For `r_changed`: enforce `r_changed = 1 - is_zero(r_diff)` where `r_diff` is combined from limbs.

Actually, we use a simpler approach: inverse-witness detection.
- `diff_t_inv`: inverse of `(t_next - t_cur)` if nonzero, else 0
- `diff_c_inv`: inverse of `(c_next - c_cur)` if nonzero, else 0
- `tc_changed = 1 - (1 - (t_next - t_cur) * diff_t_inv) * (1 - (c_next - c_cur) * diff_c_inv)`

But this is insufficient alone for soundness without range checks on the difference values.
With the StrictIneq gadget providing range-checked ordering, the same-key detection
becomes redundant for ordering but remains useful for conditional branching in constraints.

**Decision**: Use boolean witness `tc_changed` / `r_changed` + constrain via field-diff inverse.
The StrictIneq borrow chain handles ordering soundness.

## D4: Shared Borrow Chain

Row ordering in GlobalSortedMem requires either `r` or `tau` to be strictly increasing
between adjacent real rows. These are **mutually exclusive**:

- Different `(t,c)` or different `r`: enforce `r_{i+1} > r_i` (or `(t,c)` lex order)
- Same `(t,c,r)`: enforce `tau_{i+1} > tau_i`

One `StrictIneq<T>` (5 columns) handles both cases via selector gating:
- When `r_changed=1` or `tc_changed=1`: StrictIneq constrains `a_next > a_cur` on `r` values
- When `r_changed=0` and `tc_changed=0`: StrictIneq constrains `tau_next > tau_cur`

The selector determines which pair of values feeds the borrow chain.

## D5: RangeCheckChip

Preprocessed lookup table `[0, 2^16)`:
- 2 columns: `value` (preprocessed), `multiplicity` (main trace)
- No AIR constraints (table is preprocessed)
- LogUp bus: `InteractionKind::RangeCheck`
- Each limb range check decomposes into two 16-bit lookups (or one 16-bit + one < 16 check)

For the 30+30+4 decomposition:
- limb0, limb1: each split into two 15-bit halves, each range-checked via 16-bit table
- limb2: range [0, 16) — single lookup suffices (value < 2^16)

Range check interactions are declared but wired in M9 (LogUp integration).

## D6: PoseidonChip (M8)

Dedicated Poseidon2 permutation AIR chip, shared via `InteractionKind::PoseidonPermutation`.
48-64 columns. Deferred to M8 detailed design.

## D7: Chip Column Layouts

### GlobalSortedMem (`<const W: usize>`, Standard W=3)

```
is_real             1       row gating
table_id            1       (t,c) segment identity
col_id              1
r: U64Limbs         3       row key
tau: U64Limbs       3       timestamp
is_init             1       init row flag
is_write            1       write vs read
val: [T; W]         3       value (Tier 2)
val_is_null         1       null flag
mem: [T; W]         3       running memory
mem_is_null         1       running null flag
is_last_for_key     1       write-set extraction
has_written         1       write-set extraction
tc_changed          1       same-key detection
r_changed           1       same-key detection
diff_t_inv          1       inverse helper
diff_c_inv          1       inverse helper
strict_ineq: StrictIneq  5  shared borrow chain (r or tau)
─────────────────────────
Total: 31 + W = 34 (W=3)
```

### ColumnMeta (existing, upgraded in M7)

Replaces inverse-based lex ordering with IsZero gadgets (see D10).

### Execution (M8), GlobalSSMC (M8), GlobalMerge (M8)

Detailed in M8 design phase.

## D8: Bus Architecture

| Bus | Sender | Receiver |
|-----|--------|----------|
| Memory | Execution | GlobalSortedMem |
| SsmcMembership | GlobalSortedMem (init) | GlobalSSMC |
| MergeCompleteness | GlobalSortedMem (write-set) + GlobalSSMC | GlobalMerge |
| ColumnMetaJoin | GlobalSSMC/Merge/SortedMem | ColumnMeta |
| RangeCheck | all chips needing range checks | RangeCheckChip |
| ReadOnlyOpening | Execution | VC opening proof |
| PoseidonPermutation | any chip needing Poseidon | PoseidonChip |

M7 declares all buses. M9 wires LogUp fingerprints.

## D9: File Organization

```
air/
  gadgets/
    mod.rs
    boolean.rs          (existing)
    integer.rs          (M7: U64Limbs, StrictIneq, IsZero)
    mem.rs              (M7: null canon, memory transitions)
  chips/
    mod.rs              (TabulaAir enum + dispatch)
    column_meta/        (existing, upgraded in M7)
    range_check.rs      (M7: preprocessed table)
    sorted_mem/         (M7: GlobalSortedMem)
      mod.rs
      columns.rs
      air.rs
      trace.rs
```

## D10: ColumnMeta Soundness Upgrade

Replace inverse-based lex ordering (M6 — uniqueness-only) with:

1. `IsZero<T>` for `table_id_diff` and `col_id_diff`
2. Range-checked positive differences via RangeCheck bus (declared, wired M9)

The IsZero gadget prevents the inverse-forgery attack where a prover can claim
`diff=0` by providing `inv=0` even when `diff != 0`.

## D11: Implementation Order

1. **M7a**: Integer gadgets (`integer.rs`) + memory helpers (`mem.rs`) + RangeCheckChip + ColumnMeta upgrade
2. **M7b**: GlobalSortedMemChip (columns, constraints, trace generation)
3. **M8**: ExecutionChip + PoseidonChip + GlobalSSMC + GlobalMerge (separate design phase)

## D12: Testing Strategy

- Each gadget: populate + constrain tests with known values, edge cases, soundness (corrupted witness must fail)
- Each chip: valid trace + multiple invalid traces via `debug_check()`
- Integration: witness-generated traces pass chip AIR constraints
- Target: ~30-40 new tests in M7
