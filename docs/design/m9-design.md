# M9 Design: LogUp Bus Wiring & Cross-Chip Integration

## 1. Overview

M8 delivered 7 AIR chips with **local constraints only**. Every chip carries witness
columns for inter-chip communication (hash accumulators, operand values, access logs)
but does NOT constrain them against other chips. M9 wires these via **LogUp arguments**
— the standard Plonky3-ecosystem technique for proving multiset equality across
separate trace tables.

### 1.1 Goals

1. Build a reusable LogUp framework (interaction declaration, permutation trace, constraints)
2. Wire all cross-chip buses (memory consistency, SSMC membership, merge completeness, etc.)
3. Complete local constraints deferred from M8 (operand linkage, Poseidon RC, missing opcodes)
4. Extend `debug_check()` to verify LogUp balance across multiple chips

### 1.2 Non-Goals (deferred to M10+)

- SSMC non-membership gap proofs (requires gap witness columns in SSMC)
- Comprehensive range checking of ALL u64 limbs (infrastructure built, full wiring deferred)
- Mul / DivMod / Hash / Lookup opcode semantics
- Layout B operand linkage (LogUp def-use chain)
- SmtPathChip (SMT opening verification)
- Plonky3 prover/verifier integration (M11)

---

## 2. Architecture Decisions

### D1. LogUp approach: SP1-style with shared challenges

Plonky3 0.4 provides `AirBuilder` but has **no native LogUp API**. We build our own
interaction layer following SP1's proven pattern:

- Each chip declares `send()` / `receive()` calls during `eval()`
- A shared random challenge pair `(α, β)` generates RLC fingerprints
- `InteractionKind` tag separates buses within the same RLC space
- Per-chip permutation trace columns carry running sums
- Cross-chip balance: `Σ_chips cumulative_sum_chip = 0`

### D2. Extension field: BabyBear degree-4

LogUp operates over `EF = BabyBear^4` (quartic extension). This provides ~124-bit
security against fingerprint collisions. Plonky3 provides `BinomialExtensionField<BabyBear, 4>`.

### D3. Operand-to-slot linkage: one-hot selectors (Layout A)

Each source operand needs a 16-element one-hot selector to index into the SSA slot array.
Cost: +48 columns to ExecutionChip (selectors only; total with other additions: 118 → 170).
Layout B (LogUp def-use) is a future optimization that eliminates these columns.

### D4. Poseidon output: carry columns

PoseidonChip adds 8 `perm_output` columns carrying the permutation digest across all 21
rows of a permutation. Set by prover at first-round row, verified at last-round row.
Enables the PoseidonPermutation bus interaction at first-round rows.

### D5. Commitment verification: combined bus

SSMC hash → `Com_old` and Merge hash → `Com_new` use a single `CommitmentVerification`
bus with a `comm_type` discriminator (0=old, 1=new). ColumnMeta receives on both sides.

### D6. Range checks: infrastructure now, full wiring later

M9 builds the RangeCheck bus infrastructure and wires critical range checks
(SortedMem key/timestamp limbs). Comprehensive range checking of all u64 values
across all chips is deferred to M10 to limit column overhead.

### D7. SortedMem metadata: dedicated bus

SortedMem needs `is_empty_old` from ColumnMeta to handle empty-column init rows.
A lightweight 3-element `SortedMemMeta` bus provides this with minimal overhead (+1 column).

---

## 3. Bus Inventory

Eight LogUp buses, each identified by an `InteractionKind` tag:

| # | Bus | Width | Send | Receive |
|---|-----|-------|------|---------|
| 1 | **Memory** | 13 | Execution (`is_access`) | SortedMem (`is_real·(1−is_init)`) |
| 2 | **SsmcMembership** | 8 | SortedMem init (`is_init·(1−val_is_null)·(1−meta_empty)`) | SSMC (`is_real·mult_accessed`) |
| 3 | **MergeOldList** | 8 | SSMC (`is_real·segment_is_touched`) | Merge (`is_real·has_old`) |
| 4 | **MergeWriteSet** | 9 | SortedMem (`is_last_for_key·has_written`) | Merge (`is_real·has_write`) |
| 5 | **PoseidonPerm** | 24 | SSMC/Merge hash chains | Poseidon (`is_first_round`) |
| 6 | **CommitmentVerif** | 12 | SSMC/Merge last-of-segment | ColumnMeta (`tag=0` rows) |
| 7 | **SortedMemMeta** | 3 | SortedMem first-of-segment | ColumnMeta (`is_real·has_sorted_mem`) |
| 8 | **RangeCheck** | 1 | All chips (limb halves) | RangeCheck (preprocessed) |

### 3.1 Fingerprint Formula

All buses share challenges `(α, β)` derived via Fiat-Shamir. Per-interaction RLC:

```
f = α + β^0 · kind_tag + β^1 · values[0] + β^2 · values[1] + ... + β^{n} · values[n-1]
```

The `kind_tag` (integer 1..8) prevents cross-bus collisions.

### 3.2 InteractionKind Enum (updated)

```rust
pub enum InteractionKind {
    Memory = 1,
    SsmcMembership = 2,
    MergeOldList = 3,
    MergeWriteSet = 4,
    PoseidonPermutation = 5,
    CommitmentVerification = 6,
    SortedMemMeta = 7,
    RangeCheck = 8,
}
```

Removes `ReadOnlyOpening` (Phase 2 optimization) and `ColumnMetaJoin` (split into
`CommitmentVerification` + `SortedMemMeta`). Adds `MergeOldList`, `MergeWriteSet`,
`CommitmentVerification`, `SortedMemMeta`.

---

## 4. Bus Specifications

### 4.1 Memory Bus

Links execution access log to sorted memory events (proof-spec §8.4).

**Tuple** (13 elements, W=3):
```
(t, c, r_l0, r_l1, r_l2, tau_l0, tau_l1, tau_l2, is_write, val[0], val[1], val[2], val_is_null)
 ↑  ↑  ────── r[3] ──────  ───── tau[3] ──────           ────── val[W] ──────
```

Width: t(1) + c(1) + r(3) + tau(3) + is_write(1) + val(3) + val_is_null(1) = 13.
Using u64 limbs for both `r` and `tau` in the fingerprint ensures soundness without
requiring range checks to be wired first (different limb values → different fingerprints
via Schwartz-Zippel).

**Send**: ExecutionChip, each `is_access=1` row.
- Columns: `(access_t, access_c, access_r[3], tau_l0/l1/l2, access_is_write, access_val[W], access_is_null)`
- Multiplicity: `is_real · is_access`
- Note: `tau` in execution uses a single FE (not u64 limbs). Must decompose to 3 limbs
  for fingerprint matching. Add `tau_limbs: U64Limbs<T>` to ExecutionCols (+3 columns)
  with constraint `tau = tau_limbs.limb0 + tau_limbs.limb1 · 2^30 + tau_limbs.limb2 · 2^60`.

**Receive**: GlobalSortedMemChip, each non-init real row.
- Columns: `(table_id, col_id, r[3], tau[3], is_write, val[W], val_is_null)`
- Multiplicity: `is_real · (1 − is_init)`

### 4.2 SsmcMembership Bus

Proves init row values exist in the SSMC table (proof-spec §4.2.A membership).

**Tuple** (8 elements, W=3):
```
(t, c, key_l0, key_l1, key_l2, val[0], val[1], val[2])
```

**Send**: GlobalSortedMemChip, init rows for non-empty columns with non-null values.
- Columns: `(table_id, col_id, r[3], val[W])`
- Multiplicity: `is_real · is_init · (1 − val_is_null) · (1 − meta_is_empty_old)`
- Note: empty columns and null-valued init rows skip SSMC lookup

**Receive**: GlobalSsmcChip, real rows that are accessed by init rows.
- Columns: `(table_id, col_id, key[3], value[W])`
- Multiplicity: `is_real · mult_witness` where `mult_witness ∈ {0, 1}` is a new witness column
- Note: `mult_witness=1` for entries looked up by init rows, 0 for others.
  LogUp enforces consistency between send and receive multiplicities.

**New column**: `mult_witness: T` in GlobalSsmcCols (+1 column, boolean-constrained).

### 4.3 MergeOldList Bus

Proves every OldList entry **for touched columns** appears in the merge trace (proof-spec §4.2.A completeness).

**Important**: SSMC has rows for all non-empty SSMC columns (both touched and untouched —
untouched columns still need rows for SsmcMembership lookups). But GlobalMerge only exists
for touched columns (`is_touched=1`). Therefore, the MergeOldList send must be gated to
only include SSMC rows from touched-column segments.

**Tuple** (8 elements, W=3):
```
(t, c, key_l0, key_l1, key_l2, old_val[0], old_val[1], old_val[2])
```

**Send**: GlobalSsmcChip, real rows in **touched** segments only.
- Columns: `(table_id, col_id, key[3], value[W])`
- Multiplicity: `is_real · segment_is_touched`
- Note: `segment_is_touched` is a new per-segment boolean witness column in SSMC.
  Constrained to be constant within a segment (transition: same segment ⟹ unchanged).
  Verified against ColumnMeta's `is_touched` via the CommitmentVerification bus (§4.6).

**Receive**: GlobalMergeChip, rows from OldList (old_only, both, or delete source).
- Columns: `(table_id, col_id, key[3], old_val[W])`
- Multiplicity: `is_real · (is_old_only + is_both + is_delete)`
  where derived selectors: `is_old_only = (1−s1)(1−s0)`, `is_both = s1·(1−s0)`,
  `is_delete = s1·s0`.

**New column**: `segment_is_touched: T` in GlobalSsmcCols (+1 column, boolean-constrained,
constant within segment).

### 4.4 MergeWriteSet Bus

Proves every batch write-set entry appears in the merge trace.

**Tuple** (9 elements, W=3):
```
(t, c, key_l0, key_l1, key_l2, write_val[0], write_val[1], write_val[2], val_is_null)
```

Note: `val_is_null` is essential to distinguish a write of zero from a delete (write of Null).
Without it, `Write(k, 0)` and `Write(k, Null)` produce identical fingerprints, allowing a
malicious prover to swap deletes for zero-writes or vice versa.

**Send**: GlobalSortedMemChip, write-set extraction rows.
- Columns: `(table_id, col_id, r[3], mem[W], mem_is_null)`
- Multiplicity: `is_real · is_last_for_key · has_written`
- Note: `mem` (running memory value) is the final value for the key;
  `mem_is_null` is the null flag for the final memory state.
  (At `is_last_for_key`, `mem_is_null = val_is_null` is guaranteed by
  the read/write transition constraints, but we use `mem_is_null` for
  semantic consistency with the `mem` value being sent.)

**Receive**: GlobalMergeChip, rows from WriteSet (write_only, both, or delete source).
- Columns: `(table_id, col_id, key[3], write_val[W], is_delete)`
- Multiplicity: `is_real · (is_write_only + is_both + is_delete)`
  where `is_write_only = (1−s1)·s0`.
- Note: `is_delete = s1·s0` serves as the `val_is_null` flag on the receive side,
  since delete is the only write-set source where the value is Null.

### 4.5 PoseidonPermutation Bus

Links hash chain invocations to verified Poseidon permutations (proof-spec §4.2 hash).

**Tuple** (24 elements):
```
(input_state[0..16], output_digest[0..8])
```

**Send**: GlobalSsmcChip and GlobalMergeChip, each real row that contributes to
the hash chain (see §6 Hash Chain Constraints for input composition).
- Multiplicity: see §6

**Receive**: PoseidonChip, at `is_first_round=1` rows.
- Columns: `(state[0..16], perm_output[0..8])`
- Multiplicity: `is_real · is_first_round`

**New columns**: `perm_output: [T; 8]` in PoseidonCols (+8 columns).
Carry constraint: constant within a permutation (21 rows).
Verified at last round: `perm_output = external_linear_layer(sbox_out)[0..8]`.

### 4.6 CommitmentVerification Bus

Proves SSMC/Merge hash chains match ColumnMeta commitments, and verifies
SSMC's `segment_is_touched` flag against ColumnMeta's `is_touched`.

**Tuple** (12 elements):
```
(t, c, comm_type, is_touched, digest[0..8])
```

Where `comm_type = 0` for Com_old (SSMC), `comm_type = 1` for Com_new (Merge).
The `is_touched` field binds SSMC's `segment_is_touched` to ColumnMeta's `is_touched`.

**Send**: GlobalSsmcChip, at last-of-segment rows.
- Columns: `(table_id, col_id, 0, segment_is_touched, hash_acc[0..8])`
- Multiplicity: `is_real · is_last`

**Send**: GlobalMergeChip, at last-of-segment rows.
- Columns: `(table_id, col_id, 1, 1, hash_acc[0..8])`
- Multiplicity: `is_real · is_last_segment`
  Note: Merge always sends `is_touched=1` (merge rows only exist for touched columns).
  Merge needs an `is_last_segment` flag. Currently `tc_changed` detects segment
  boundaries via the NEXT row. For the last real row, we need `is_last_segment = is_real ·
  (tc_changed OR real_to_padding)`. This is already derivable from existing columns.

**Receive**: ColumnMetaChip.
- For Com_old: `(table_id, col_id, 0, is_touched, com_old[0..8])`
  Multiplicity: `is_real · (1 − tag) · (1 − is_empty_old)`
- For Com_new: `(table_id, col_id, 1, is_touched, com_new[0..8])`
  Multiplicity: `is_real · (1 − tag) · is_touched`

Note: ColumnMeta participates in TWO receive interactions per eligible row — one for
Com_old and one for Com_new. This is handled naturally by LogUp (separate fingerprints
due to different `comm_type` values). The `is_touched` field in Com_old receive verifies
that SSMC's `segment_is_touched` matches ColumnMeta's ground truth.

### 4.7 SortedMemMeta Bus

Provides `is_empty_old` metadata from ColumnMeta to SortedMem init rows.

**Tuple** (3 elements):
```
(t, c, is_empty_old)
```

**Send**: GlobalSortedMemChip, first row of each (t,c) segment.
- Columns: `(table_id, col_id, meta_is_empty_old)`
- Multiplicity: first-of-segment selector (derivable from `tc_changed` of previous row)

**Receive**: ColumnMetaChip, rows that have a corresponding SortedMem segment.
- Columns: `(table_id, col_id, is_empty_old)`
- Multiplicity: `is_real · has_sorted_mem`
- Note: ColumnMeta has rows for ALL columns (touched + untouched), but SortedMem only
  has segments for columns accessed in the batch. `has_sorted_mem` is a prover witness
  boolean gating which ColumnMeta rows participate. LogUp enforces consistency: if the
  prover sets it wrong for any row, the multiset equality fails.

**New columns**:
- `meta_is_empty_old: T` in GlobalSortedMemCols (+1 column, boolean)
- `has_sorted_mem: T` in ColumnMetaCols (+1 column, boolean)

### 4.8 RangeCheck Bus

Proves u64 limb sub-components are in [0, 2^16).

**Tuple** (1 element):
```
(value)
```

**Send**: Any chip needing a range proof. Each 30-bit limb is split into two 15-bit
halves (`lo`, `hi`) with constraint `limb = lo + hi · 2^15`. Both halves are sent
as separate interactions.

**Receive**: RangeCheckChip preprocessed table [0, 2^16).
- Multiplicity: `mult_witness` (prover-computed count of lookups per entry)

**M9 scope**: Wire range checks for SortedMem `r` and `tau` limbs only.
Requires +4 columns per U64Limbs instance (limb0_lo, limb0_hi, limb1_lo, limb1_hi).
Full range checking of all chips deferred to M10.

---

## 5. Phase A: Local Constraint Completion

These constraints are WITHIN a single chip (no LogUp needed).

### 5.1 Operand-to-Slot Linkage (ExecutionChip)

**Problem**: `src1_val[W]`, `src2_val[W]`, `cond_val` are unconstrained witnesses.
A malicious prover can set them to arbitrary values, making all opcode semantics
constraints meaningless.

**Solution**: One-hot slot selectors.

**New columns** (+48):
```rust
// In ExecutionCols:
pub src1_sel: [T; MAX_SLOTS],  // +16: one-hot selector for src1's slot index
pub src2_sel: [T; MAX_SLOTS],  // +16: one-hot selector for src2's slot index
pub cond_sel: [T; MAX_SLOTS],  // +16: one-hot selector for cond's slot index
```

**Constraints**:
```
// For each operand selector (src1_sel shown):
1. Boolean: src1_sel[s] ∈ {0,1} for all s
2. Exactly-one (when operand is used):
   is_real · needs_src1 · (Σ_s src1_sel[s] - 1) = 0
3. Value linkage:
   src1_sel[s] · (src1_val[i] - slots[s][i]) = 0   for all s, i∈[0,W)
4. Null linkage (for src1):
   src1_sel[s] · (src1_is_null - slot_is_null[s]) = 0   for all s

// needs_src1 = op_arith + op_cmp + op_not + op_and + op_or + op_assert + op_select + op_write
// needs_src2 = op_arith + op_cmp + op_and + op_or + op_select
// needs_cond = op_select
```

**New column**: `src1_is_null: T` (+1) — null flag for src1 operand.

**Write operand linkage** (C2):
`op_write` is included in `needs_src1`. When `op_write=1`, the value being written to
state comes from `src1_val`:
```
// op_write ⟹ access_val = src1_val, access_is_null = src1_is_null
is_real · op_write · (access_val[i] - src1_val[i]) = 0   for all i∈[0,W)
is_real · op_write · (access_is_null - src1_is_null) = 0
```

**Read destination linkage** (C3):
When `op_read=1`, the value from the access log must be written to the destination slot:
```
// op_read ⟹ written slot gets access_val
is_real · op_read · slot_written[s] · (slots[s][i] - access_val[i]) = 0   for all s, i∈[0,W)
is_real · op_read · slot_written[s] · (slot_is_null[s] - access_is_null) = 0   for all s
```
Note: Read does NOT use `needs_src1` (it has no source operand — it produces a value).

**Notes**:
- When an opcode doesn't need an operand, the selector can be all-zeros (not exactly-one).
  Gate the exactly-one constraint by the `needs_srcX` selector.
- `cond_val` is a single FE from `slots[s][0]`. Linkage: `cond_sel[s] · (cond_val - slots[s][0]) = 0`.
- Total new columns: 48 (selectors) + 1 (src1_is_null) = 49.

### 5.2 Poseidon Round Constant Verification

**Problem**: `rc[16]` columns are unconstrained witnesses.

**Solution**: Hard-code the Poseidon2 BabyBear round constants and verify via equality.

**Approach**: At each round, constrain `rc[i] = RC_TABLE[round_ctr][i]` where `RC_TABLE`
is a 21×16 matrix of known constants extracted from `p3_poseidon2`.

**Implementation**: Use preprocessed columns. PoseidonChip gets a preprocessed trace
with the correct RC values. Constraint: `rc[i] = preprocessed.rc[i]` at each real row.

This requires extending `BaseAir::preprocessed_trace()` for PoseidonChip and updating
the debug checker to support preprocessed columns.

**Alternative** (simpler for M9): Encode round constants as a lookup. Each (round_ctr, i, rc_value)
triple is verified against a preprocessed table. This can use the existing RangeCheck-style
preprocessed pattern.

**Decision**: Use preprocessed columns (cleaner, follows Plonky3 convention).

### 5.3 Poseidon is_full_round Consistency

**Problem**: `is_full_round` is unconstrained — prover could mark partial rounds as full.

**Constraint**: `is_full_round` must be consistent with `round_ctr`:
- Rounds 0-3: `is_full_round = 1` (initial full rounds)
- Rounds 4-16: `is_full_round = 0` (partial rounds)
- Rounds 17-20: `is_full_round = 1` (final full rounds)

**Implementation**: With preprocessed RC columns, `is_full_round` can also be preprocessed.
Otherwise, constrain via:
```
is_real · (round_ctr - 4) · (round_ctr - 5) · ... · (round_ctr - 16) · (1 - is_full_round) = 0
```
This is degree 14 — too high. Use preprocessed approach instead.

### 5.4 Transaction Index Monotonicity (ExecutionChip)

**Problem**: `tx_index` is unconstrained — a malicious prover could interleave instructions
from different transactions.

**Constraint**: `tx_index` must be non-decreasing across consecutive real rows:
```
both_real ⟹ next.tx_index − local.tx_index ∈ {0, 1}
```

Equivalently:
```
both_real · (next.tx_index − local.tx_index) · (next.tx_index − local.tx_index − 1) = 0
```

### 5.5 Boolean Opcode Constraints

Add constraints for Not, And, Or (simple boolean operations):

```
// Not: dst = 1 - src1 (src1 must be boolean)
op_not · slot_written[s] · (slots[s][0] - (1 - src1_val[0])) = 0

// And: dst = src1 · src2
op_and · slot_written[s] · (slots[s][0] - src1_val[0] · src2_val[0]) = 0

// Or: dst = src1 + src2 - src1 · src2
op_or · slot_written[s] · (slots[s][0] - src1_val[0] - src2_val[0] + src1_val[0] · src2_val[0]) = 0
```

For W=3: higher limbs of boolean results must be zero:
```
op_{not|and|or} · slot_written[s] · slots[s][i] = 0   for i ∈ {1, 2}
```

### 5.6 Cmp Opcode Constraints

Comparison operations (Eq, Neq, Lt, Lte, Gt, Gte) produce a boolean result.

**New columns** needed:
- `cmp_sub_sel: [T; 6]` — one-hot for comparison type (Eq=0, Neq=1, Lt=2, Lte=3, Gt=4, Gte=5)
- `cmp_lt_witness: T` — auxiliary: 1 if src1 < src2 (for ordering comparisons)
- `cmp_eq_witness: T` — auxiliary: 1 if src1 = src2

**Deferred to M10**: Full Cmp constraint requires u64 comparison via borrow chain
(similar complexity to Sub). Defer along with Mul/DivMod.

---

## 6. Hash Chain Constraints

### 6.1 SSMC Hash Chain

Each SSMC row invokes one Poseidon permutation for the iterative hash chain.

**Input composition** (16 elements):

For `is_first=1` (first entry in segment):
```
input = [0x00, table_id, col_id, key.l0, key.l1, key.l2, value[0], value[1], value[2], 0, 0, 0, 0, 0, 0, 0]
```

For `is_first=0` (continuation):
```
input = [prev_hash_acc[0..8], key.l0, key.l1, key.l2, value[0], value[1], value[2], 0, 0]
```
Where `prev_hash_acc` = hash_acc of the previous row (same segment).

**Local constraints** (in SSMC air.rs):
```
// Compose perm_input[16] from local columns:
when is_real:
  if is_first:
    perm_input[0] = 0x00 (domain tag)
    perm_input[1] = table_id
    perm_input[2] = col_id
    perm_input[3..6] = key[0..3]
    perm_input[6..6+W] = value[0..W]
    perm_input[6+W..16] = 0
  else:
    perm_input[0..8] = prev_row.hash_acc[0..8]  (transition constraint)
    perm_input[8..11] = key[0..3]
    perm_input[11..11+W] = value[0..W]
    perm_input[11+W..16] = 0
```

Note: the `prev_row.hash_acc` reference is a transition constraint using the PREVIOUS row's
hash_acc. In the AIR, we access `local` and `next` rows. The constraint is written from
the perspective of the NEXT row:
```
when both_real AND NOT next.is_first:
  next.perm_input[0..8] = local.hash_acc[0..8]
```

**New columns** in GlobalSsmcCols: `perm_input: [T; 16]` (+16 columns).
These carry the composed Poseidon input for the PoseidonPermutation bus.

**PoseidonPermutation interaction** (per real SSMC row):
- Send: `(perm_input[0..16], hash_acc[0..8])`
- Multiplicity: `is_real`
- This proves: `Poseidon(perm_input) = hash_acc` (digest)

### 6.2 Merge Hash Chain

Similar to SSMC, but only rows with `in_new=1` contribute to the NewList hash.

**Input composition** (16 elements):

For `is_first_in_new=1` (first NewList entry in segment):
```
input = [0x00, table_id, col_id, key.l0, key.l1, key.l2, new_val[0..W], 0, ..., 0]
```

For continuation (`is_first_in_new=0`, `in_new=1`):
```
input = [prev_hash_acc[0..8], key.l0, key.l1, key.l2, new_val[0..W], 0, 0]
```

Rows with `in_new=0` (delete) do NOT participate in the hash chain. The hash_acc
carries forward unchanged. This requires a **local transition constraint**:

```
// Within same segment, when current row has in_new=0:
// next row's hash_acc must equal current row's hash_acc.
when both_real AND NOT tc_changed AND (1 − in_new):
  next.hash_acc[0..8] = local.hash_acc[0..8]
```

**New columns** in GlobalMergeCols:
- `perm_input: [T; 16]` (+16 columns) — composed Poseidon input
- `is_first_in_new: T` (+1 column) — first `in_new=1` row in segment

**PoseidonPermutation interaction** (per real Merge row with `in_new=1`):
- Send: `(perm_input[0..16], hash_acc[0..8])`
- Multiplicity: `is_real · in_new`

---

## 7. LogUp Framework Design

### 7.1 Interaction Types

```rust
/// Constraint-time interaction (emitted during Air::eval).
pub struct AirInteraction<E> {
    pub values: Vec<E>,           // Fingerprint tuple elements (as expressions)
    pub multiplicity: E,          // Selector expression (0 or 1 typically)
    pub kind: InteractionKind,    // Bus tag
}
```

### 7.2 Builder Trait

```rust
/// Extension to AirBuilder for LogUp interaction declarations.
pub trait InteractionAirBuilder: AirBuilder {
    /// Declare a send interaction (this chip contributes to the multiset).
    fn send(&mut self, interaction: AirInteraction<Self::Expr>);

    /// Declare a receive interaction (this chip consumes from the multiset).
    fn receive(&mut self, interaction: AirInteraction<Self::Expr>);
}
```

Chips call `builder.send(...)` / `builder.receive(...)` inside their `eval()` method.
The concrete builder implementation collects these for permutation trace generation.

### 7.3 Interaction Collection

```rust
/// Collects interactions during a symbolic evaluation pass.
pub struct InteractionCollector<F: Field> {
    sends: Vec<Interaction<F>>,      // Collected send interactions
    receives: Vec<Interaction<F>>,   // Collected receive interactions
}

/// Static interaction descriptor (column indices + weights).
pub struct Interaction<F: Field> {
    pub values: Vec<VirtualPairCol<F>>,
    pub multiplicity: VirtualPairCol<F>,
    pub kind: InteractionKind,
}
```

The collector runs a symbolic evaluation of `Air::eval()` to extract `VirtualPairCol`
descriptors from symbolic expressions. This is done once per chip at setup time.

### 7.4 Permutation Trace Generation

Per chip, the prover generates a permutation trace:

```rust
pub fn generate_permutation_trace<F: PrimeField32, EF: ExtensionField<F>>(
    sends: &[Interaction<F>],
    receives: &[Interaction<F>],
    main_trace: &RowMajorMatrix<F>,
    challenges: &[EF; 2],        // (alpha, beta)
    batch_size: usize,
) -> (RowMajorMatrix<EF>, EF)    // (perm_trace, cumulative_sum)
```

**Per-row computation**:
1. For each interaction i, compute fingerprint:
   `f_i = α + β^0 · kind + β^1 · values[0] + ... + β^n · values[n-1]`
2. Compute signed contribution: `c_i = m_i / f_i` (positive for send, negative for receive)
3. Batch contributions into permutation columns (batch_size interactions per column)
4. Last column = running cumulative sum

**Permutation trace width**: `ceil(num_interactions / batch_size) + 1`

### 7.5 Permutation Constraints

The AIR constraints for the permutation trace enforce:

1. **Batch entry correctness**: Each batch column equals `Σ_i m_i / f_i` for its batch
   of interactions. Verified via the "cross-multiply" technique:
   `entry · Π(f_i) = Σ_i (m_i · Π_{j≠i}(f_j))`

2. **Running sum**: `cumsum[0] = Σ batch_entries[0]`;
   `cumsum[i+1] = cumsum[i] + Σ batch_entries[i+1]`

3. **Final sum**: `cumsum[last_row] = chip_cumulative_sum` (public value per chip)

4. **Cross-chip balance**: `Σ_chips chip_cumulative_sum = 0` (verified outside the AIR)

### 7.6 Debug Checker Extension

Extend `debug_check()` to verify LogUp balance across multiple chips.

```rust
/// Verify AIR constraints AND LogUp balance for a multi-chip trace.
pub fn debug_check_multi<F: Field>(
    chips: &[(impl Air<...>, RowMajorMatrix<F>)],
    challenges: &[F; 2],
) -> Result<(), MultiChipError>
```

**Algorithm**:
1. For each chip: run existing `debug_check()` for local/transition constraints
2. For each chip: compute per-row `Σ m_i / f_i` for all interactions
3. Sum across all rows and chips: verify total = 0

This avoids generating actual permutation traces — it directly checks the multiset
equality using concrete field arithmetic.

---

## 8. Column Impact Summary

### 8.1 ExecutionChip (118 → 170)

| Change | Columns | Reason |
|--------|---------|--------|
| `src1_sel[16]` | +16 | Operand-to-slot linkage |
| `src2_sel[16]` | +16 | Operand-to-slot linkage |
| `cond_sel[16]` | +16 | Operand-to-slot linkage |
| `src1_is_null` | +1 | Null flag for src1 |
| `tau_limbs: U64Limbs` | +3 | Memory bus tau decomposition |
| **Subtotal** | **+52** | **118 → 170** |

### 8.2 PoseidonChip (69 → 77)

| Change | Columns | Reason |
|--------|---------|--------|
| `perm_output[8]` | +8 | Permutation digest carry |
| **Subtotal** | **+8** | **69 → 77** |

### 8.3 GlobalSsmcChip (27 → 45)

| Change | Columns | Reason |
|--------|---------|--------|
| `perm_input[16]` | +16 | Hash chain Poseidon input |
| `mult_witness` | +1 | SsmcMembership multiplicity |
| `segment_is_touched` | +1 | MergeOldList gating (§4.3) |
| **Subtotal** | **+18** | **27 → 45** |

### 8.4 GlobalMergeChip (34 → 51)

| Change | Columns | Reason |
|--------|---------|--------|
| `perm_input[16]` | +16 | Hash chain Poseidon input |
| `is_first_in_new` | +1 | First NewList entry flag |
| **Subtotal** | **+17** | **34 → 51** |

### 8.5 GlobalSortedMemChip (32 → 41)

| Change | Columns | Reason |
|--------|---------|--------|
| `meta_is_empty_old` | +1 | SortedMemMeta bus metadata |
| `r` half-decomposition | +4 | RangeCheck bus (limb0_lo/hi, limb1_lo/hi) |
| `tau` half-decomposition | +4 | RangeCheck bus (limb0_lo/hi, limb1_lo/hi) |
| **Subtotal** | **+9** | **32 → 41** |

### 8.6 ColumnMetaChip (27 → 28)

| Change | Columns | Reason |
|--------|---------|--------|
| `has_sorted_mem` | +1 | SortedMemMeta bus gating (§4.7) |
| **Subtotal** | **+1** | **27 → 28** |

### 8.7 RangeCheckChip (2 → 2)

No new columns. Multiplicity column already exists.

### 8.8 Total New Columns

+105 columns across all chips (Execution +52, Poseidon +8, SSMC +18, Merge +17,
SortedMem +9, ColumnMeta +1). Plus per-chip permutation trace columns (extension field,
generated by prover, not part of main trace width).

---

## 9. Implementation Phases

### Phase A: Local Constraints (no LogUp dependency)

**A1**: Operand-to-slot linkage in ExecutionChip
- Add 49 new columns to ExecutionCols
- Add constraints in execution/air.rs
- Update trace generation in execution/trace.rs
- Tests: valid traces with correct linkage, invalid with wrong slot values

**A2**: Poseidon RC verification + is_full_round
- Add preprocessed trace support to PoseidonChip
- Extract RC constants from p3-poseidon2
- Add equality constraints against preprocessed columns
- Tests: valid RCs pass, corrupted RCs fail

**A3**: ~~Boolean opcode constraints (Not, And, Or) + tx_index monotonicity~~ **DONE** (pre-M9)
- Already implemented: `constrain_not`, `constrain_and`, `constrain_or`,
  `constrain_tx_index_monotonicity` in execution/air.rs with tests

### Phase B: LogUp Framework

**B1**: Core types and builder
- `air/interaction.rs`: `AirInteraction`, `Interaction`, `VirtualPairCol` usage
- `air/builder.rs`: `InteractionAirBuilder` trait
- `air/interaction_collector.rs`: Symbolic collection from `eval()`

**B2**: Permutation trace generation
- `air/permutation.rs`: `generate_permutation_trace()`, batch processing
- Extension field arithmetic via `p3-field`

**B3**: Permutation constraint evaluation
- `air/permutation.rs`: `eval_permutation_constraints()`
- Cross-multiply technique for batch entries

**B4**: Debug checker extension
- `air/debug.rs`: `debug_check_multi()` for LogUp balance verification
- Tests: simple 2-chip interaction (send/receive balance)

### Phase C: Bus Wiring

**C1**: Memory bus (Execution ↔ SortedMem)
- Add `tau_limbs` to ExecutionCols
- Add send interaction in execution/air.rs
- Add receive interaction in sorted_mem/air.rs
- Tests: valid access sequences, mismatched values fail

**C2**: SsmcMembership bus (SortedMem init ↔ SSMC)
- Add `mult_witness` to GlobalSsmcCols
- Add send/receive interactions
- Tests: correct membership, missing entries fail

**C3**: MergeOldList bus (SSMC ↔ Merge)
- Add `segment_is_touched` to GlobalSsmcCols, constrain boolean + per-segment constant
- Add send interaction gated by `segment_is_touched`
- Add receive interaction in Merge
- Tests: all old entries present, missing entry fails; untouched column not sent

**C4**: MergeWriteSet bus (SortedMem ↔ Merge)
- Add send/receive interactions
- Tests: write-set entries match merge trace

**C5**: PoseidonPermutation bus + hash chains (§6)
- Add `perm_output` to PoseidonCols, `perm_input` to SSMC/Merge
- Add hash chain local constraints (§6.1, §6.2)
- Add send/receive interactions
- Tests: correct hash chains, corrupted hash_acc fails

**C6**: CommitmentVerification bus
- Add send interactions in SSMC/Merge at segment boundaries
- Add receive interactions in ColumnMeta
- Tests: hash matches commitment, wrong commitment fails

**C7**: SortedMemMeta bus
- Add `meta_is_empty_old` to SortedMemCols
- Add `has_sorted_mem` to ColumnMetaCols (gate receive multiplicity)
- Add send/receive interactions
- Tests: metadata matches ColumnMeta, untouched columns excluded from bus

**C8**: RangeCheck bus (critical subset)
- Add half-decomposition columns to SortedMem r and tau
- Wire range check sends
- Tests: in-range values pass, out-of-range fails

### Phase D: Integration

**D1**: Multi-chip integration test
- Construct a full batch trace across ALL chips
- Verify LogUp balance via `debug_check_multi()`

**D2**: Documentation update
- Update architecture.md with M9 summary
- Update MEMORY.md with M9 decisions

---

## 10. New Files

```
crates/tabula-proof/src/air/
├── interaction.rs          — AirInteraction, Interaction, InteractionKind (replaces bus.rs)
├── builder.rs              — InteractionAirBuilder trait + InteractionCollector
└── permutation.rs          — Permutation trace gen + constraint evaluation

(bus.rs is renamed/replaced by interaction.rs)
```

### Modified Files

```
air/debug.rs                — debug_check_multi()
air/chips/mod.rs            — Updated InteractionKind, ChipMeta interactions()
air/chips/execution/columns.rs   — +52 columns (49 linkage + 3 tau_limbs)
air/chips/execution/air.rs       — Operand linkage, Read/Write value linkage, Memory send
air/chips/execution/trace.rs     — Updated trace gen
air/chips/poseidon/columns.rs    — +8 columns
air/chips/poseidon/air.rs        — RC verification, PoseidonPerm receive
air/chips/poseidon/trace.rs      — Preprocessed trace, perm_output
air/chips/ssmc/columns.rs        — +18 columns (16 perm_input + 1 mult_witness + 1 segment_is_touched)
air/chips/ssmc/air.rs            — Hash chain, SsmcMembership recv, MergeOldList send, etc.
air/chips/ssmc/trace.rs          — Updated trace gen
air/chips/merge/columns.rs       — +17 columns
air/chips/merge/air.rs           — Hash chain, MergeOldList recv, MergeWriteSet recv, etc.
air/chips/merge/trace.rs         — Updated trace gen
air/chips/sorted_mem/columns.rs  — +9 columns (1 meta + 8 range-check halves)
air/chips/sorted_mem/air.rs      — Memory recv, SsmcMembership send, etc.
air/chips/sorted_mem/trace.rs    — Updated trace gen
air/chips/column_meta/columns.rs — +1 column (has_sorted_mem)
air/chips/column_meta/air.rs     — CommitmentVerif recv, SortedMemMeta recv
air/chips/range_check.rs         — RangeCheck recv multiplicity
```

---

## 11. Testing Strategy

### Per-bus test pattern (×8 buses)

For each bus:
1. **Valid trace**: all LogUp sums balance → `debug_check_multi()` passes
2. **Wrong value**: corrupt one tuple element → balance fails
3. **Wrong multiplicity**: set multiplicity to 0 for a required row → fails
4. **Extra send**: add an unmatched send → fails
5. **Missing send**: omit a required send → fails

### Integration tests

1. **Single-tx batch**: one transaction, all buses active
2. **Multi-tx batch**: multiple transactions, inter-tx read-after-write
3. **Empty column**: column with `is_empty_old=1`, init value = Null
4. **Delete case**: write Null, merge delete, column becomes empty

### Estimated test count

- Phase A: ~15 tests (operand linkage, Poseidon RC)
- Phase B: ~10 tests (framework, debug checker)
- Phase C: ~40 tests (8 buses × 5 per bus)
- Phase D: ~5 tests (integration)
- **Total: ~70 new tests** (204 existing + 70 = ~274)

---

## 12. Risk Analysis

| Risk | Severity | Mitigation |
|------|----------|------------|
| Extension field degree mismatch | HIGH | Use `p3-field` `BinomialExtensionField<BabyBear, 4>` consistently |
| Fingerprint collision (wrong bus matched) | HIGH | InteractionKind tag in RLC prevents cross-bus collision |
| Permutation trace width explosion | MEDIUM | Batch size = 4 (typical), limits width to ceil(N/4)+1 |
| Hash chain input encoding mismatch | MEDIUM | Unit test Poseidon(compose) = hash_acc for known values |
| ColumnMeta multiplicity off-by-one | MEDIUM | Explicit test for each ColumnMeta receive case |
| Operand linkage degree too high | LOW | One-hot × value equality = degree 2, within limit |
| 96 new columns degrade prover perf | LOW | Acceptable for correctness; Layout B optimizes later |

---

## 13. Success Criteria

- [ ] All 204 existing tests pass unchanged
- [ ] ~70 new M9 tests pass (LogUp + local constraints)
- [ ] `debug_check_multi()` verifies LogUp balance for a full batch
- [ ] All `interactions()` methods return non-empty `Vec<InteractionDecl>`
- [ ] Operand-to-slot linkage constrains `src1_val`, `src2_val`, `cond_val`
- [ ] Poseidon RC verified against preprocessed constants
- [ ] SSMC/Merge hash chains constrained via PoseidonPermutation bus
- [ ] SSMC/Merge commitments verified against ColumnMeta via CommitmentVerification bus
- [ ] SortedMem `meta_is_empty_old` verified via SortedMemMeta bus
- [ ] Memory bus links Execution to SortedMem
- [ ] MergeOldList + MergeWriteSet buses enforce merge completeness
- [ ] `cargo test -p tabula-proof --features stark` passes
- [ ] `cargo clippy -p tabula-proof --features stark --all-targets` — zero warnings
- [ ] Architecture doc and MEMORY.md updated

---

## 14. Dependency Graph

```
Phase A (local constraints)
  A1: Operand linkage ──────────────────────┐
  A2: Poseidon RC ──────────────────────────┤
  A3: Boolean opcodes ─────────────────────┤
                                            │
Phase B (LogUp framework)                   │
  B1: Types + builder ─┐                   │
  B2: Perm trace gen ──┤                   │
  B3: Perm constraints ┤                   │
  B4: Debug checker ────┘                  │
         │                                  │
         ▼                                  ▼
Phase C (bus wiring) ◄──────────── depends on A + B
  C1: Memory ──┐
  C2: SsmcMembership ──┤
  C3: MergeOldList ──┤
  C4: MergeWriteSet ──┤
  C5: PoseidonPerm + hash chains ──┤
  C6: CommitmentVerif ──┤
  C7: SortedMemMeta ──┤
  C8: RangeCheck ──────┘
         │
         ▼
Phase D (integration) ◄──── depends on C
  D1: Multi-chip test
  D2: Documentation
```

Phases A and B are independent and can be parallelized.
Phase C depends on both A and B.
Phase D depends on C.
