# M11 Design: State Root Binding + Gap Proofs

> Complete soundness: bind column commitments to the state root via SMT,
> prove non-membership via gap witnesses, connect public inputs to the AIR.

---

## 1. Goals and Scope

### 1.1 Goals

1. **Gap witness non-membership** — prove that a key is absent from committed SSMC state
   via `next_key` denormalization and strict inequality constraints
2. **SMT Merkle path verification** — new `SmtPathChip` proving bottom-up path
   traversal with Poseidon compress, binding leaf digests to public roots
3. **Leaf digest computation** — ColumnMeta computes `LeafDigest` for each `(t,c)`
   column, wired to SmtPathChip via LogUp bus
4. **Public input binding** — wire `ApplyBatchStatement` fields (6 public inputs)
   to AIR constraints via `AirBuilderWithPublicValues`
5. **StaticTableChip** — receiver side for `StaticTableLookup` bus (M10-B3)

### 1.2 Non-Goals (deferred to M12+)

- Trace assembly (executor → chip traces) — M12
- Plonky3 prover/verifier integration — M13
- `AppliedTxDigest` hash chain (tx outcome commitment) — M12 trace assembly
- `ProgramRoot` Merkle inclusion proof — M12 (reuses SmtPathChip)
- Multi-permutation sponge support in AIR — future optimization
- Layout B operand linkage — optimization

### 1.3 Architecture Decisions

#### D1. Leaf digest: compress-based (single permutation)

**Problem**: The current `compute_leaf` in `hybrid.rs` uses `hash_domain(0x10, [t, c, tag, Com[0..8]])`.
This produces a 12-FE sponge input requiring 2 Poseidon permutations (rate=8). Our AIR hash chain
pattern does not carry sponge capacity between permutations — it resets capacity to zero at each step.
A true sponge would require 8 additional capacity-carry columns per permutation row.

**Decision**: Change `compute_leaf` to use `compress` (single TruncatedPermutation):

```
LeafDigest(t, c) = compress(
    [DOMAIN_LEAF, t, c, tag, 0, 0, 0, 0],   // left: 8 FE (header + padding)
    Com[0..8]                                  // right: 8 FE (commitment)
)
= Poseidon2([0x10, t, c, tag, 0, 0, 0, 0, Com[0], ..., Com[7]])[0..8]
```

**Rationale**:
- Single permutation = reuses existing PoseidonPermutation bus with zero new infrastructure
- Domain separated: `DOMAIN_LEAF = 0x10` in position 0 distinguishes from SMT node compress
  (which has arbitrary child hashes, never starting with a small domain tag)
- Matches the ColumnMeta `Com_empty` pattern (M10-B4): single perm_input[16] + perm_output[8]
- Requires updating `hybrid.rs` `compute_leaf()` to use `hasher.compress()` instead of `hasher.hash_domain()`

**Change scope**: `crates/tabula-commitment/src/hybrid.rs` — `compute_leaf()` method only.

#### D2. SMT node hash: compress (no per-level domain tag)

The commitment crate's `SparseMerkleTree::compress(left, right)` uses `TruncatedPermutation`
with NO per-level domain tag. Domain separation comes from the `empty_hashes` chain seeded
from `hash_domain(domain_tag, &[])`. The AIR matches this exactly:

```
node_hash(left, right) = Poseidon2([left[0..8], right[0..8]])[0..8]
```

This is the same PoseidonPermutation bus format. No `level` or `domain_tag` column needed
for SMT node constraints. Different trees (`SMT_cols` domain=0x12 vs `SMT_tables` domain=0x11)
produce different empty subtree hashes, which propagate through the sibling values — the AIR
need not distinguish tree types.

#### D3. SMT path traversal: LSB-first, bottom-up

The commitment crate extracts key bits LSB-first: `bit_i = (key >> i) & 1`.
Bit 0 is at level 0 (leaf/shallowest). The path is verified bottom-up (leaf → root).
SmtPathChip rows are ordered by increasing level within each path instance.

#### D4. Dual-column update proof (old + new in one path)

For each touched column, the SMT update proof requires two root derivations sharing
the same sibling path but differing in the leaf value. Rather than two separate path
instances (doubling row count), each SmtPathChip row contains BOTH `old_node[8]` and
`new_node[8]`, with two PoseidonPermutation bus sends per row.

**Trade-off**: Wider columns (~60 FE) but half the row count per update. Same number
of Poseidon permutations total. Better for prover efficiency (fewer trace rows = faster FFT).

#### D5. Two-level SMT: two SmtPathChip instances

The state root uses a two-level SMT:
- **SMT_cols** (depth=16): per-table, key=ColId, leaf=`LeafDigest(t,c)` → `table_root[t]`
- **SMT_tables** (depth=32): global, key=TableId, leaf=`table_root[t]` → `state_root`

SmtPathChip is generic over `const DEPTH: usize`. Two instances are created:
- `SmtPathChip<16>` for column-level paths (one path per touched column)
- `SmtPathChip<32>` for table-level paths (one path per touched table)

Column-level roots bind to table-level leaves via a `SmtTableRoot` LogUp bus.

#### D6. Gap witness: SortedMem sends, SSMC receives

Non-membership is proven at the SortedMem init rows (where `val_is_null=1` and the key
has no SSMC entry). The SSMC chip receives the gap witness lookup and the constraint
verifies `key < r < next_key` via StrictIneq (reusing M10 infrastructure).

The gap witness columns live on SortedMem (send side), using a new Operation struct
`GapWitness<T>` that bundles the two StrictIneq gadgets + boundary flags.

#### D7. StaticTableChip: simple table with LogUp receive

StaticTableChip is a sorted table of `(t, c, r, val[W])` tuples from committed static data.
It receives on the `StaticTableLookup` bus. Root binding to `StaticTableRoot` (public input)
is deferred to M12 — M11 just provides the receiver that balances the LogUp bus.

---

## 2. Execution Order

```
Phase A: Gap Witness (non-membership)
  A1  SSMC next_key column + transition constraint
  A2  GapWitness gadget (SortedMem side)
  A3  SsmcGapWitness bus wiring (send/receive)

Phase B: Leaf Digest + Commitment Changes
  B1  Change compute_leaf to compress-based (commitment crate)
  B2  ColumnMeta leaf digest columns + Poseidon bus sends

Phase C: SmtPathChip (new chip)
  C1  Column layout + basic constraints (compress, continuity, path_bit)
  C2  Key reconstruction (path_bit → key running sum)
  C3  Leaf/root boundary constraints
  C4  SmtTableRoot bus (column paths → table paths)

Phase D: Public Input Binding
  D1  AirBuilderWithPublicValues integration
  D2  Root binding (SmtPathChip root → public oldRoot/newRoot)
  D3  Budget enforcement (trace dimension checks)
  D4  StaticTableRoot public input binding (stub)

Phase E: StaticTableChip
  E1  Column layout + LogUp receive
  E2  Basic tests with mock data
```

Phase A and B are independent and can be parallelized.
Phase C depends on B (leaf digest format).
Phase D depends on C (root binding).
Phase E is independent of A-D.

---

## 3. Detailed Specifications

### A1. SSMC `next_key` Column

**Problem**: Gap witness lookups need both bounding keys `(key, next_key)` from a single
SSMC row's LogUp fingerprint. LogUp references columns of a single row only — cannot join
adjacent rows.

**File**: `crates/tabula-proof/src/air/chips/ssmc/columns.rs`

Add after `key_ordering`:
```rust
/// Next entry's key (denormalized from the following row within same segment).
/// Required for gap-witness non-membership lookups.
/// Constrained: `next_key = NEXT.key` when `is_last = 0`.
pub next_key: KeyRangeChecked<T>,  // 7 cols (3 limbs + 2×LimbHalves)
```

**Width impact**: 66 → 73 (+7)

**Transition constraint** (in `ssmc/air.rs`):
```
// Within same segment, not-last row: next_key must equal the next row's key.
let not_last = local.is_real · (1 - local.is_last);
for j in 0..3:
    not_last · (local.next_key.limbs[j] - next.key.limbs[j]) = 0

// At is_last = 1: next_key is unconstrained (padding/boundary).
// Range check: next_key.send_range_checks(builder, is_real) — always checked when real.
```

**Trace generation** (in `ssmc/trace.rs`):
```
// For each real row i (not last in segment):
cols[i].next_key.populate(entries[i+1].key);
// For the last row in a segment:
cols[last].next_key.populate(0);  // unconstrained, zero-fill
```

### A2. GapWitness Gadget

**New file**: `gadgets/gap_witness.rs`

A composite Operation struct for non-membership gap proofs on SortedMem init rows.

```rust
/// Gap witness columns for non-membership proof.
///
/// Proves that row key `r` is absent from the SSMC committed state by showing
/// it falls in a gap between two adjacent entries: `key < r < next_key`.
/// Boundary cases handled via `is_first` / `is_last` flags.
///
/// Active when: `is_init = 1` AND `val_is_null = 1` AND `meta_is_empty_old = 0`
/// (a null init for a non-empty column implies the key is absent from SSMC).
#[repr(C)]
#[derive(Clone, Debug)]
pub struct GapWitness<T> {
    /// 1 if this row needs a gap witness proof.
    pub is_active: T,                    // 1 col

    // ── Bounding entry from SSMC (looked up via SsmcGapWitness bus) ──
    /// Lower bounding key from the SSMC gap row.
    pub bound_key: U64Limbs<T>,          // 3 cols
    /// Upper bounding key from the SSMC gap row.
    pub bound_next_key: U64Limbs<T>,     // 3 cols
    /// 1 if the bounding row is the first entry in its SSMC segment.
    pub bound_is_first: T,               // 1 col
    /// 1 if the bounding row is the last entry in its SSMC segment.
    pub bound_is_last: T,                // 1 col

    // ── Lower bound: key < r (StrictIneq + halves for range check) ──
    pub lower: OrderingRangeChecked<T>,  // 7 cols

    // ── Upper bound: r < next_key (StrictIneq + halves for range check) ──
    pub upper: OrderingRangeChecked<T>,  // 7 cols
}
// Total: 23 cols
```

**Constraint logic** (`GapWitness::eval()`):

```
gate = is_active   // only constrain when active

// Boolean constraints
assert_bool(is_active)
assert_bool(bound_is_first)
assert_bool(bound_is_last)

// Mutual exclusion: cannot be both first and last simultaneously
// (a segment with one entry has is_first=1, is_last=1 — but gap witness
//  for such segments is impossible since key < r < next_key has no room)
// Actually, is_first and is_last CAN both be 1 (single-entry segment).
// In that case, both lower AND upper bounds apply normally.

// Case 1: Interior gap (is_first=0, is_last=0)
//   Prove: bound_key < r AND r < bound_next_key
//   Both StrictIneq active.

// Case 2: Before first (is_first=1)
//   Prove: r < bound_key (the first key in the segment)
//   Only upper bound applies, but directionally: r < bound_key
//   Reinterpret: upper proves (r, bound_key) and lower is inactive

// Case 3: After last (is_last=1)
//   Prove: bound_key < r
//   Only lower bound applies.

// Unified constraint:
// Lower bound: active when NOT is_first (proves bound_key < r)
let lower_active = gate · (1 - bound_is_first);
constrain_ordering(lower, bound_key, r, lower_active);

// Upper bound: active when NOT is_last (proves r < bound_next_key)
let upper_active = gate · (1 - bound_is_last);
constrain_ordering(upper, r, bound_next_key, upper_active);

// Special case: is_first = 1 → prove r < bound_key
// This uses the upper ordering gadget but with swapped arguments
let before_first_active = gate · bound_is_first;
constrain_ordering(upper, r, bound_key, before_first_active);
```

Wait — the above has a conflict: when `is_first=1`, the upper ordering proves both
`r < bound_next_key` (if `is_last=0`) and `r < bound_key`. We need to handle the
boundary cases more carefully.

**Revised three-case design**:

| Case | Condition | Proves |
|------|-----------|--------|
| Before first | `is_first=1` | `r < bound_key` |
| Interior | `is_first=0, is_last=0` | `bound_key < r` AND `r < bound_next_key` |
| After last | `is_last=1` | `bound_key < r` |

Constraints:
```
// Lower bound: proves bound_key < r
// Active for: interior + after_last = (1 - is_first)
lower.constrain(bound_key, r, gate · (1 - bound_is_first));

// Upper bound: interpretation depends on is_first
// When is_first=0: proves r < bound_next_key
// When is_first=1: proves r < bound_key
// Active for: interior + before_first = (1 - is_last)
//
// Resolved: two separate OrderingRangeChecked (lower and upper) handle the
// non-boundary directions. For the before-first case, we need a THIRD
// ordering OR we reuse one of them.
```

**Simplified approach**: Only two `OrderingRangeChecked` are needed:

```
lower_ordering: proves A < B where:
  - When is_first = 0: A = bound_key, B = r       (bound_key < r)
  - When is_first = 1: A = r, B = bound_key       (r < bound_key)

upper_ordering: proves r < bound_next_key
  - Active when is_last = 0
```

But this requires the lower ordering to switch direction based on `is_first`.
The `OrderingRangeChecked` gadget computes `gap = B - A - 1` and range-checks it.
We can compose the inputs conditionally:

```
// Lower/boundary ordering:
// direction: if is_first: gap = bound_key - r - 1
//            else:        gap = r - bound_key - 1
// Active when: is_active (always — either bound_key < r or r < bound_key)
//
// This is a single StrictIneq where the operands are chosen by is_first.
// Populate: if is_first { (r, bound_key) } else { (bound_key, r) }

// Upper ordering: gap = bound_next_key - r - 1
// Active when: is_active AND NOT is_last
```

This works! The lower `OrderingRangeChecked` is always active when `is_active=1`.
Its direction flips based on `is_first`. The AIR constrains the gap value to match
the correct direction.

Actually, `OrderingRangeChecked` stores the gap decomposition as fixed columns. The prover
fills in the gap based on the direction. The constraint just verifies `gap = B - A - 1`
where A and B are determined by `is_first`.

Let me simplify the column struct:

```rust
#[repr(C)]
pub struct GapWitness<T> {
    pub is_active: T,                    // 1 col

    // SSMC bounding entry (from LogUp lookup)
    pub bound_key: U64Limbs<T>,          // 3 cols
    pub bound_next_key: U64Limbs<T>,     // 3 cols
    pub bound_is_first: T,               // 1 col
    pub bound_is_last: T,                // 1 col

    // Lower/boundary: proves (key < r) or (r < key) depending on is_first
    pub lower: OrderingRangeChecked<T>,  // 7 cols

    // Upper: proves r < next_key (gated by !is_last)
    pub upper: OrderingRangeChecked<T>,  // 7 cols
}
// Total: 23 cols
```

The constraint for lower ordering:
```
// When is_first=0 (interior/after-last): lower proves bound_key < r
//   gap_lower = r - bound_key - 1 (range-checked)
// When is_first=1 (before-first): lower proves r < bound_key
//   gap_lower = bound_key - r - 1 (range-checked)
//
// Constraint: is_active · (lower.gap[j] - expected_gap[j]) = 0
// where expected_gap depends on is_first direction.
```

This is correct. The populate function in trace.rs computes the gap in the right direction
based on `is_first`, and the AIR constraint verifies the gap matches.

### A3. SsmcGapWitness Bus

**New InteractionKind**: `SsmcGapWitness = 10`

**Tuple** (13 elements): `(t, c, r[3], bound_key[3], bound_next_key[3], is_first, is_last)`

**Send side** (GlobalSortedMem): Init rows where `is_init=1, val_is_null=1, meta_is_empty_old=0`.

```rust
// In sorted_mem/air.rs:
let gap_active = local.gap.is_active.clone();
builder.send(AirInteraction {
    values: vec![
        local.table_id, local.col_id,
        local.r.limbs[0..3],
        local.gap.bound_key[0..3],
        local.gap.bound_next_key[0..3],
        local.gap.bound_is_first,
        local.gap.bound_is_last,
    ],
    multiplicity: gap_active,
    kind: InteractionKind::SsmcGapWitness,
});
```

**Receive side** (GlobalSSMC): Any real row. A new multiplicity witness column
`gap_mult_witness` indicates which rows serve as gap witness providers.

```rust
// In ssmc/columns.rs:
pub gap_mult_witness: T,   // +1 col → SSMC 73 → 74

// In ssmc/air.rs:
builder.receive(AirInteraction {
    values: vec![
        local.table_id, local.col_id,
        // r[3] from SortedMem side — matches against the gap range
        // But wait: the SSMC row doesn't have `r` — it has `key` and `next_key`.
        // The bus tuple must be symmetric.
    ],
    ...
});
```

**Correction**: The bus tuple carries the SSMC row's data to the SortedMem side. The
SortedMem side picks which SSMC row to look up (prover freedom). The bus connects them.

**Revised bus design**: SSMC SENDS its entry data with gap witness multiplicity;
SortedMem RECEIVES and constrains the gap inequalities.

Actually, the standard LogUp pattern is: the side that "queries" sends, the "table" side
receives. For gap witnesses:
- SortedMem init rows query "is my key absent from SSMC?"
- SSMC provides the bounding entry

So SortedMem sends and SSMC receives. The tuple carries the SSMC row data that SortedMem
claims to reference. The prover fills in `bound_key`, `bound_next_key`, `is_first`, `is_last`
on the SortedMem side from the witness. LogUp ensures these values match an actual SSMC row.

**Final bus tuple** (13 elements):
```
(t, c, bound_key[3], bound_next_key[3], is_first, is_last)
```

Note: `r` (the queried key) is NOT in the bus tuple. It's local to SortedMem and constrained
via the StrictIneq gadget against the looked-up `bound_key`/`bound_next_key`.

**Send side** (SortedMem): `mult = gap.is_active`

**Receive side** (SSMC): `mult = gap_mult_witness` (prover fills 1 for rows serving as gap
witnesses, 0 otherwise). The bus fingerprint uses:
```
values: [table_id, col_id, key[3], next_key[3], is_first, is_last]
```

---

### B1. Commitment Crate: compress-based `compute_leaf`

**File**: `crates/tabula-commitment/src/hybrid.rs`

Change `compute_leaf` from `hash_domain` to `compress`:

```rust
fn compute_leaf(
    hasher: &H,
    table: TableId,
    col: ColId,
    strategy: &CommitmentStrategy,
    com: &NativeDigest,
) -> NativeDigest {
    let tag_val = match strategy {
        CommitmentStrategy::Ssmc => BabyBear::ZERO,
        CommitmentStrategy::Smt => BabyBear::ONE,
    };
    // Header: [DOMAIN_LEAF, t, c, tag, 0, 0, 0, 0]
    let header = NativeDigest([
        BabyBear::new(DOMAIN_LEAF),
        BabyBear::new(table.0),
        BabyBear::new(col.0 as u32),
        tag_val,
        BabyBear::ZERO, BabyBear::ZERO, BabyBear::ZERO, BabyBear::ZERO,
    ]);
    hasher.compress(&header, com)
}
```

**Tests**: Update existing `compute_leaf` tests. Verify that `compute_state_root` produces
valid roots with the new formula. All witness generator tests must pass.

### B2. ColumnMeta Leaf Digest Columns

**File**: `crates/tabula-proof/src/air/chips/column_meta/columns.rs`

Add after `has_empty_check`:

```rust
// ── Leaf digest (M11-B2) ──
/// Poseidon permutation input for leaf_old: [DOMAIN_LEAF, t, c, tag, 0,0,0,0, com_old[8]].
pub leaf_perm_input_old: [T; 16],    // 16 cols
/// Poseidon permutation output: LeafDigest_old (8 FE).
pub leaf_digest_old: [T; DIGEST_WIDTH],  // 8 cols
/// Poseidon permutation input for leaf_new: same header, com_new[8].
pub leaf_perm_input_new: [T; 16],    // 16 cols
/// Poseidon permutation output: LeafDigest_new (8 FE).
pub leaf_digest_new: [T; DIGEST_WIDTH],  // 8 cols
```

**Width impact**: 56 → 104 (+48)

**Note**: This is a significant width increase. However, the ColumnMeta trace has very few
rows (one per (t,c) pair in the batch — typically tens of rows, not thousands). The cost of
wide columns in a short trace is negligible compared to narrow columns in long traces.

**Alternative (deferred optimization)**: Compute `leaf_perm_input` inline as expressions from
existing `com_old`/`com_new` columns. This would save 32 columns but requires the bus to
accept `AB::Expr` compositions of existing columns (which it already does). If ColumnMeta
width becomes a concern, we can drop the `leaf_perm_input_*` columns and construct the
PoseidonPermutation send values inline.

**Constraints** (in `column_meta/air.rs`):

```
// Compose leaf perm input from existing columns:
is_real · (leaf_perm_input_old[0] - DOMAIN_LEAF) = 0
is_real · (leaf_perm_input_old[1] - table_id) = 0
is_real · (leaf_perm_input_old[2] - col_id) = 0
is_real · (leaf_perm_input_old[3] - tag) = 0
is_real · leaf_perm_input_old[4..8] = 0   // zero padding
for i in 0..8:
    is_real · (leaf_perm_input_old[8+i] - com_old[i]) = 0

// Same for new:
// (identical structure with com_new instead of com_old)

// Send to PoseidonPermutation bus:
send(PoseidonPermutation, [leaf_perm_input_old[16], leaf_digest_old[8]], mult = is_real)
send(PoseidonPermutation, [leaf_perm_input_new[16], leaf_digest_new[8]], mult = is_real)
```

---

### C1. SmtPathChip: Column Layout

**New directory**: `crates/tabula-proof/src/air/chips/smt_path/`

```
smt_path/
├── mod.rs       # re-exports
├── columns.rs   # SmtPathCols<T>
├── air.rs       # SmtPathChip<DEPTH> + constraints
└── trace.rs     # generate_smt_path_trace()
```

**Column layout**:

```rust
/// Column layout for the SmtPathChip AIR.
///
/// One row per Merkle tree level. Rows are grouped by `path_id`, ordered
/// by increasing `level` (0 = leaf, DEPTH-1 = root) within each path.
///
/// Each path proves a dual (old + new) root derivation sharing the same
/// sibling path but differing in the leaf value. This supports SMT update proofs.
#[repr(C)]
pub struct SmtPathCols<T> {
    // ── Control ──
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Path instance identifier (monotonically increasing, resets per chip).
    pub path_id: T,
    /// Current tree level (0 = leaf, DEPTH-1 = root).
    pub level: T,
    /// 1 if this is the leaf level (level = 0).
    pub is_leaf: T,
    /// 1 if this is the root level (level = DEPTH-1).
    pub is_root: T,

    // ── Path bit (shared between old and new) ──
    /// LSB-first key bit at this level: 0 = node is left child, 1 = right child.
    pub path_bit: T,

    // ── Sibling hash (shared between old and new) ──
    /// Sibling node digest at this level (8 FE, from witness).
    pub sibling: [T; 8],

    // ── Old-tree path ──
    /// Current node digest in the old tree (8 FE).
    /// At leaf level: LeafDigest_old from ColumnMeta.
    /// At higher levels: parent from the previous level.
    pub old_node: [T; 8],
    /// Compress output: parent digest in the old tree (8 FE).
    pub old_parent: [T; 8],

    // ── New-tree path ──
    /// Current node digest in the new tree (8 FE).
    pub new_node: [T; 8],
    /// Compress output: parent digest in the new tree (8 FE).
    pub new_parent: [T; 8],

    // ── Key reconstruction ──
    /// Running key accumulation: sum of path_bit_j * 2^j for j=0..level.
    pub key_acc: T,
    /// Power of 2 for the current level: 2^level.
    pub level_power: T,

    // ── Binding metadata (populated at leaf level) ──
    /// Table identifier (for ColumnMeta / public input binding).
    pub bind_table_id: T,
    /// Column identifier (for ColumnMeta binding; 0 for table-level paths).
    pub bind_col_id: T,
}
```

**Width**: 1 + 1 + 1 + 1 + 1 + 1 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 1 + 1 = **50 columns**

### C2. SmtPathChip: Constraints

```
// ── Booleans ──
assert_bool(is_real), assert_bool(is_leaf), assert_bool(is_root), assert_bool(path_bit)

// ── is_real prefix ──
constrain_is_real_prefix(builder)

// ── Level consistency ──
is_leaf · level = 0                         // leaf is level 0
is_root · (level - (DEPTH - 1)) = 0        // root is level DEPTH-1

// ── Level monotonicity (within same path) ──
// Transition: if both rows are real and same path_id:
let same_path = is_real · next.is_real · (same path_id detection)
same_path · (next.level - level - 1) = 0   // level increments by 1
same_path · (next.path_id - path_id) = 0   // same path

// ── Path boundary: level resets at new path_id ──
// New path starts at is_leaf=1
let new_path = is_real · next.is_real · (next.path_id - path_id)  // nonzero if path changes
// (Alternative: use is_leaf as the first-row-of-path indicator)

// ── Compress (old tree) ──
// left = path_bit ? sibling : old_node
// right = path_bit ? old_node : sibling
// Inline expression for PoseidonPermutation bus send:
for i in 0..8:
    left_old[i]  = (1 - path_bit) · old_node[i] + path_bit · sibling[i]
    right_old[i] = path_bit · old_node[i] + (1 - path_bit) · sibling[i]

send(PoseidonPermutation,
     values: [left_old[0..8], right_old[0..8], old_parent[0..8]],
     mult: is_real)

// ── Compress (new tree) ──
for i in 0..8:
    left_new[i]  = (1 - path_bit) · new_node[i] + path_bit · sibling[i]
    right_new[i] = path_bit · new_node[i] + (1 - path_bit) · sibling[i]

send(PoseidonPermutation,
     values: [left_new[0..8], right_new[0..8], new_parent[0..8]],
     mult: is_real)

// ── Continuity: parent → next node ──
// Within same path, the parent at level i becomes the node at level i+1.
same_path:
    for i in 0..8:
        next.old_node[i] = old_parent[i]
        next.new_node[i] = new_parent[i]

// ── Key reconstruction ──
is_leaf: key_acc = path_bit                               // level 0
is_leaf: level_power = 1
same_path: key_acc_next = key_acc + next.path_bit · next.level_power
same_path: level_power_next = level_power · 2

// ── Leaf binding (via SmtLeafDigest bus, C11) ──
// At leaf level, send (table_id, col_id, old_node, new_node) for ColumnMeta to receive.
send(SmtLeafDigest,
     values: [bind_table_id, bind_col_id, old_node[0..8], new_node[0..8]],
     mult: is_real · is_leaf)
```

### C3. Key Reconstruction Constraint

At the root level (last row of each path), `key_acc` must equal the expected key:
- For column-level paths (SmtPathChip<16>): `key_acc = bind_col_id`
- For table-level paths (SmtPathChip<32>): `key_acc = bind_table_id`

```
is_root · (key_acc - expected_key) = 0
```

Where `expected_key` is `bind_col_id` for column paths and `bind_table_id` for table paths.
This is a chip-level configuration, not a per-row witness.

### C4. SmtTableRoot Bus

Connects column-level paths (SmtPathChip<16>) to table-level paths (SmtPathChip<32>).

**New InteractionKind**: `SmtTableRoot = 12`

**Tuple** (18 elements): `(table_id, old_root[8], new_root[8])`

**Send side** (SmtPathChip<16>, root level):
At each column-level path's root row, the `old_parent` and `new_parent` are the per-table
roots `table_root_old[t]` and `table_root_new[t]`.

```
send(SmtTableRoot,
     values: [bind_table_id, old_parent[0..8], new_parent[0..8]],
     mult: is_real · is_root)
```

**Receive side** (SmtPathChip<32>, leaf level):
At each table-level path's leaf row, `old_node` and `new_node` are the per-table roots.

```
receive(SmtTableRoot,
     values: [bind_table_id, old_node[0..8], new_node[0..8]],
     mult: is_real · is_leaf)
```

**Note**: Multiple column-level paths for the same table all produce the same
`(table_id, table_root_old, table_root_new)`. LogUp handles duplicates naturally
(multiplicity > 1 on the receive side). However, we need to coalesce: the table-level
path's leaf row receives once per table, not once per column. This requires a multiplicity
witness on the receive side.

**Alternative approach**: Instead of a bus, bind via public input:
- Column-level paths publish their root digests as intermediate values
- Table-level paths consume those roots as leaf inputs
- The binding is through equality constraints on shared values

For now, the bus approach is cleaner and doesn't require a separate intermediate public
input mechanism.

---

### D1. AirBuilderWithPublicValues Integration

**File**: `crates/tabula-proof/src/air/builder.rs`

Extend `InteractionAirBuilder` to include public values:

```rust
pub trait InteractionAirBuilder: AirBuilder + PairBuilder {
    fn send(&mut self, interaction: AirInteraction<Self::Expr>);
    fn receive(&mut self, interaction: AirInteraction<Self::Expr>);
    fn interactions(&self) -> &[RecordedInteraction<Self::Var>];

    // NEW: public values access
    fn public_values(&self) -> &[Self::Var];
}
```

`DebugConstraintBuilder` is updated to store public values:

```rust
pub struct DebugConstraintBuilder<'a, F: Field> {
    // ... existing fields ...
    public_values: &'a [F],  // NEW
}
```

### D2. Root Binding

At SmtPathChip<32> root level, bind to public inputs:

```
// Old state root: old_parent at root level = oldRoot public input
is_root · (old_parent[i] - public_values[OLD_ROOT_OFFSET + i]) = 0   for i in 0..8

// New state root: new_parent at root level = newRoot public input
is_root · (new_parent[i] - public_values[NEW_ROOT_OFFSET + i]) = 0   for i in 0..8
```

Note: all table-level paths share the SAME root. The constraint fires on every root-level
row. This is correct: every path through `SMT_tables` must produce the same root.

### D3. Budget Enforcement

Budget fields from `ApplyBatchStatement`:
- `max_ops: u32` — max IR instructions
- `max_slots: u16` — max SSA slots
- `max_accesses: u32` — max state accesses

Enforcement:
- ExecutionChip trace height = `max_ops` (padded with `is_real=0`)
- Slot count: `MAX_SLOTS >= max_slots` (static compile-time check)
- Access count: final value of `clk` column ≤ `max_accesses`

```
// On the last real row (is_real=1, next.is_real=0):
last_real · (clk - public_values[MAX_ACCESSES_OFFSET]) ≤ 0
```

This can be proven via a range check: `max_accesses - clk ∈ [0, 2^32)`.

### D4. StaticTableRoot Stub

`StaticTableRoot` is bound as a public input but not yet verified against the StaticTableChip
content (deferred to M12 when trace assembly connects the chip data).

---

### E1. StaticTableChip

**New directory**: `crates/tabula-proof/src/air/chips/static_table/`

```
static_table/
├── mod.rs
├── columns.rs
├── air.rs
└── trace.rs
```

**Column layout**:

```rust
#[repr(C)]
pub struct StaticTableCols<T, const W: usize> {
    /// Row is real (1) or padding (0).
    pub is_real: T,
    /// Table identifier.
    pub table_id: T,
    /// Column identifier.
    pub col_id: T,
    /// Row key (u64 limbs).
    pub row_key: U64Limbs<T>,    // 3 cols
    /// Value field elements.
    pub value: [T; W],
}
```

**Width**: 1 + 1 + 1 + 3 + W = 6 + W (9 for W=3)

**Constraints**:
- `is_real` prefix (padding at end)
- Receive on `StaticTableLookup` bus:

```
receive(StaticTableLookup,
     values: [table_id, col_id, row_key[0..3], value[0..W]],
     mult: is_real)
```

This balances the send from ExecutionChip (M10-B3). Each static table row can be
looked up multiple times (LogUp handles multiplicity naturally).

**Root binding**: Deferred to M12. For M11, the chip provides the LogUp receiver only.

---

## 4. New InteractionKind Variants

| Variant | Tag | Send | Receive | New in M11 |
|---------|-----|------|---------|-----------|
| `SsmcGapWitness` | 10 | SortedMem (gap active) | SSMC (gap_mult) | Yes |
| `SmtLeafDigest` | 11 | SmtPathChip<16/32> (leaf) | ColumnMeta | Yes |
| `SmtTableRoot` | 12 | SmtPathChip<16> (root) | SmtPathChip<32> (leaf) | Yes |

Total: 12 variants (was 9 in M10).

---

## 5. New Gadgets

### `gadgets/gap_witness.rs` — `GapWitness<T>`

| Field | Type | Cols |
|-------|------|------|
| `is_active` | `T` | 1 |
| `bound_key` | `U64Limbs<T>` | 3 |
| `bound_next_key` | `U64Limbs<T>` | 3 |
| `bound_is_first` | `T` | 1 |
| `bound_is_last` | `T` | 1 |
| `lower` | `OrderingRangeChecked<T>` | 7 |
| `upper` | `OrderingRangeChecked<T>` | 7 |
| **Total** | | **23** |

Methods: `populate()`, `eval()`, `send_gap_witness_bus()`, `send_range_checks()`

---

## 6. Column Width Impact

| Chip | M10 Width | M11 Additions | M11 Width |
|------|-----------|---------------|-----------|
| ExecutionChip | 278 | — | **278** |
| GlobalSSMC | 66 | +7 (next_key) +1 (gap_mult) | **74** |
| GlobalMerge | 74 | — | **74** |
| GlobalSortedMem | 67 | +23 (GapWitness) | **90** |
| ColumnMeta | 56 | +48 (leaf digests) | **104** |
| PoseidonChip | 93 (+19 prep) | — | **93** |
| RangeCheckChip | 2 | — | **2** |
| **SmtPathChip** | — | **50 (new)** | **50** |
| **StaticTableChip** | — | **9 (new, W=3)** | **9** |

**Total main trace width**: 278 + 74 + 74 + 90 + 104 + 93 + 2 + 50 + 9 = **774**
(was 558 post-M10, +216)

**Note**: ColumnMeta +48 looks large, but ColumnMeta has very few rows (one per touched
column). The dominant traces (Execution, SortedMem, SSMC) are more width-sensitive.

---

## 7. Bus Builder Traits (additions)

Add to `bus.rs`:

```rust
pub trait SsmcGapWitnessAirBuilder: InteractionAirBuilder {
    fn send_gap_witness(&mut self, t, c, bound_key, bound_next_key,
                        is_first, is_last, mult);
    fn receive_gap_witness(&mut self, ...);
}

pub trait SmtLeafDigestAirBuilder: InteractionAirBuilder {
    fn send_leaf_digest(&mut self, table_id, col_id,
                        old_digest: &[Self::Var; 8],
                        new_digest: &[Self::Var; 8], mult);
    fn receive_leaf_digest(&mut self, ...);
}

pub trait SmtTableRootAirBuilder: InteractionAirBuilder {
    fn send_table_root(&mut self, table_id,
                       old_root: &[Self::Var; 8],
                       new_root: &[Self::Var; 8], mult);
    fn receive_table_root(&mut self, ...);
}
```

---

## 8. Test Plan

| Phase | New Tests | Updated | Description |
|-------|-----------|---------|-------------|
| A1 | 5 | 3 | SSMC next_key transition, boundary, width assertion |
| A2 | 6 | 0 | GapWitness gadget: interior, before-first, after-last, single-entry |
| A3 | 4 | 2 | SsmcGapWitness bus balance, gap ordering failure cases |
| B1 | 3 | 5 | compress-based compute_leaf, state root consistency |
| B2 | 4 | 2 | ColumnMeta leaf digest, Poseidon bus sends |
| C1 | 8 | 0 | SmtPathChip basic: compress, continuity, path_bit, key reconstruction |
| C2 | 4 | 0 | SmtPathChip boundary: leaf binding, root binding |
| C3 | 3 | 0 | SmtTableRoot bus balance, column→table binding |
| D1 | 2 | 3 | Public values in DebugConstraintBuilder |
| D2 | 3 | 0 | Root binding constraints, budget enforcement |
| E1 | 4 | 2 | StaticTableChip receive, bus balance with ExecutionChip |
| **Total** | **~46** | **~17** | |

Expected test count after M11: 359 + 46 = **~405**

---

## 9. Success Criteria

- [ ] SSMC `next_key` correctly denormalized from adjacent row, range-checked
- [ ] Gap witness proves non-membership for all 3 cases (interior, before-first, after-last)
- [ ] `compute_leaf` changed to compress-based, all witness generator tests pass
- [ ] ColumnMeta computes LeafDigest for old and new commitments via Poseidon bus
- [ ] SmtPathChip<16> verifies column-level Merkle paths (leaf → table_root)
- [ ] SmtPathChip<32> verifies table-level Merkle paths (table_root → state_root)
- [ ] Column-level and table-level paths connected via SmtTableRoot bus
- [ ] Public inputs (oldRoot, newRoot) bound to SmtPathChip root constraints
- [ ] StaticTableChip receives on StaticTableLookup bus, balancing ExecutionChip sends
- [ ] ~405 tests pass, zero clippy warnings
- [ ] `cargo test --workspace` passes

---

## 10. Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| ColumnMeta +48 cols for leaf digests | Medium | Optimization: drop perm_input columns, compose inline from existing com_old/com_new. Saves 32 cols |
| SmtPathChip variable-depth paths in fixed-width chip | Medium | Use `const DEPTH: usize` generic. Two chip instances (depth=16, depth=32) |
| SmtTableRoot bus multiplicity (multiple columns per table) | Medium | Receive-side multiplicity witness on SmtPathChip<32> leaf rows |
| `AirBuilderWithPublicValues` compatibility with InteractionAirBuilder | Medium | Extend trait hierarchy. DebugConstraintBuilder provides mock public values |
| Gap witness for single-entry SSMC segments (is_first=1, is_last=1) | Low | Both boundary cases active: upper proves `r < bound_key`, lower proves `bound_key < r` — contradiction means single-entry gap is impossible (correct) |
| Compress-based leaf changes commitment format | Low | Only `compute_leaf` changes; all downstream (SMT insert, root computation) unchanged |
| `level_power` overflow for depth=32 | Low | 2^31 < BabyBear p=2013265921. For depth=32, 2^31 = 2147483648 > p. Use two-limb decomposition or constrain max depth to 30 |

### Risk: `level_power` overflow at depth=32

`2^31 = 2147483648 > p = 2013265921`. This means `level_power` would overflow BabyBear
at level 31. Two mitigations:

**Option A**: Limit `SMT_tables` depth to 30 (supports 2^30 = ~1 billion table IDs). This is
sufficient for any realistic deployment.

**Option B**: Use a 2-limb `level_power` representation: `level_power_lo + level_power_hi * 2^15`.
This adds 1 column but handles any depth.

**Recommendation**: Option A (depth ≤ 30). Change `TABLE_STATE_SMT_DEPTH` from 32 to 30 in
`hybrid.rs`. The constraint `level_power_next = level_power * 2` stays in single-field arithmetic.

---

## 11. File Change Summary

| File | Changes |
|------|---------|
| `air/interaction.rs` | Add `SsmcGapWitness=10`, `SmtLeafDigest=11`, `SmtTableRoot=12` |
| `air/bus.rs` | Add 3 builder traits |
| `air/builder.rs` | Add `public_values()` to `InteractionAirBuilder` |
| `air/debug.rs` | Store and provide public values in `DebugConstraintBuilder` |
| `air/gadgets/gap_witness.rs` | **New**: `GapWitness<T>` operation |
| `air/gadgets/mod.rs` | Add `gap_witness` module + re-exports |
| `air/chips/ssmc/columns.rs` | +7 (next_key) +1 (gap_mult) |
| `air/chips/ssmc/air.rs` | next_key transition, gap witness receive |
| `air/chips/ssmc/trace.rs` | Populate next_key, gap_mult |
| `air/chips/sorted_mem/columns.rs` | +23 (GapWitness) |
| `air/chips/sorted_mem/air.rs` | Gap witness send, StrictIneq constraints |
| `air/chips/sorted_mem/trace.rs` | Populate gap witness from init rows |
| `air/chips/column_meta/columns.rs` | +48 (leaf digests) |
| `air/chips/column_meta/air.rs` | Leaf digest Poseidon sends, SmtLeafDigest receive |
| `air/chips/column_meta/trace.rs` | Compute leaf digests |
| `air/chips/smt_path/` | **New chip** (4 files, ~600 lines) |
| `air/chips/static_table/` | **New chip** (4 files, ~200 lines) |
| `air/mod.rs` | Export new chips + builder traits |
| `commitment/hybrid.rs` | Change `compute_leaf` to compress-based |
| `tests/chips/ssmc.rs` | Gap witness + next_key tests |
| `tests/chips/sorted_mem.rs` | Gap witness tests |
| `tests/chips/column_meta.rs` | Leaf digest tests |
| `tests/chips/smt_path.rs` | **New**: SmtPathChip tests |
| `tests/chips/static_table.rs` | **New**: StaticTableChip tests |
| `tests/gadgets/gap_witness.rs` | **New**: GapWitness gadget tests |
| `tests/infra/bus.rs` | Update bus balance tests for 3 new buses |
| `tests/common/builders.rs` | Update debug builder for public values |
