# M10 Design: Constraint Completeness

> Complete all AIR constraints for existing chips: range checks, lex ordering direction,
> missing opcode semantics (Cmp, Hash, Lookup, Mul, DivMod), and Com_empty verification.

---

## 1. Goals and Scope

### 1.1 Goals

1. Wire **comprehensive range checks** for all u64 limbs across all chips (SSMC, Merge, Execution, SortedMem ordering diffs)
2. Enforce **strict lex ordering direction** at (t,c) segment boundaries in all 4 global tables
3. Implement AIR constraints for **5 missing opcodes**: Cmp, Hash, Lookup, Mul, DivMod
4. Verify **Com_empty** hash in ColumnMeta via PoseidonPermutation bus
5. Fix the **DivMod operand selector bug** (missing from `needs_src1`/`needs_src2`)

### 1.2 Non-Goals (deferred to M11+)

- SSMC `next_key` column and non-membership gap proofs (M11)
- SmtPathChip / state root binding (M11)
- Public input binding (ApplyBatchStatement → AIR) (M11)
- Trace assembly / Plonky3 prover integration (M12-M13)
- Layout B operand linkage (optimization)

### 1.3 Execution Order

```
Phase A: Foundation
  A1  Range check wiring (all chips)          ← foundational, enables A2
  A2  Lex ordering direction enforcement      ← depends on A1

Phase B: Simple Opcodes
  B1  Cmp constraint                          ← independent, uses StrictIneq from A1
  B2  Hash constraint                         ← independent, PoseidonPermutation bus
  B3  Lookup constraint                       ← independent, new InteractionKind
  B4  Com_empty verification                  ← independent, PoseidonPermutation bus

Phase C: Complex Arithmetic
  C1  Mul constraint                          ← highest complexity
  C2  DivMod constraint + operand fix         ← depends on C1 (shared approach)
```

Phase A is strictly prerequisite for soundness. Phase B items are independent.
Phase C is the highest-risk area (BabyBear carry overflow for u64 multiplication).

---

## 2. Architecture Decisions

### D1. Range check strategy: LimbHalves + direct sends

Each 30-bit limb is split into two 15-bit halves via `LimbHalves<T>` (2 FE).
Each half is sent to the RangeCheck bus as a [0, 2^16) lookup.
4-bit limbs (limb2) are sent directly (value < 16 < 2^16).

StrictIneq diff limbs follow the same pattern:
- `diff0`, `diff1` ∈ [0, 2^30) → LimbHalves (2 FE each)
- `diff2` ∈ [0, 16) → direct send

### D2. Lex ordering direction: range-checked (t,c) difference

At segment boundaries (`tc_changed=1`, both rows real), enforce:
```
t_{i+1} > t_i  OR  (t_{i+1} = t_i AND c_{i+1} > c_i)
```

Strategy: Introduce a single witness `lex_diff_is_table: T` (boolean).
- When `lex_diff_is_table=1`: constrain `t_{i+1} - t_i - 1 ≥ 0` via StrictIneq on (t_i, t_{i+1}) treated as u64.
- When `lex_diff_is_table=0`: constrain `t_{i+1} = t_i` AND `c_{i+1} - c_i - 1 ≥ 0`.

Since `table_id` and `col_id` are single field elements (not u64 limbs), and their values are small (< 2^16 in practice), the strict inequality can use a simpler range-check: `diff = next - current - 1`, range-check `diff ∈ [0, 2^16)` via a single RangeCheck send. No LimbHalves needed.

New columns per chip: `lex_diff_is_table: T` (1) + `lex_table_diff: T` (1) + `lex_col_diff: T` (1) = 3 FE.

### D3. Cmp approach: shared StrictIneq + equality witness

Two auxiliary witnesses: `cmp_lt_witness` (1 if lhs < rhs), `cmp_eq_witness` (1 if lhs = rhs).
One shared `StrictIneq` proves the ordering direction:
- If `lt=1`: gap = rhs - lhs - 1 (range-checked)
- If `lt=0, eq=0` (gt): gap = lhs - rhs - 1 (range-checked)
- If `eq=1`: all limb diffs = 0 (via IsZero gadget)

The constraint selects which direction the StrictIneq proves based on `cmp_lt_witness`.
Sub-operator selectors (6 one-hot) gate the final result:
```
Eq:  dst = cmp_eq_witness
Ne:  dst = 1 - cmp_eq_witness
Lt:  dst = cmp_lt_witness
Lte: dst = cmp_lt_witness + cmp_eq_witness
Gt:  dst = 1 - cmp_lt_witness - cmp_eq_witness
Gte: dst = 1 - cmp_lt_witness
```

### D4. Hash instruction: multi-permutation via PoseidonPermutation bus

The Hash instruction computes `Poseidon_sponge(domain_tag || n || ComEnc(inputs))`.
For W=3 (Standard), each input occupies 3 FE. With rate=8, one permutation absorbs
2-3 inputs. The executor tracks how many permutations are needed.

For M10, we constrain Hash by:
1. Composing `perm_input[16]` from slot values + domain tag
2. Sending `(perm_input, perm_output[8])` on PoseidonPermutation bus
3. Binding `perm_output` to destination slot

**Simplification**: M10 supports single-permutation Hash only (≤2 inputs for W=3,
which covers most use cases). Multi-permutation sponge support deferred to M11+.

### D5. Lookup: reuse access columns + new InteractionKind

Lookup reuses existing `access_t`, `access_c`, `access_r`, `access_val` columns.
A new `InteractionKind::StaticTableLookup = 9` is added. ExecutionChip sends
`(access_t, access_c, access_r[3], access_val[W])` with multiplicity `is_real * op_lookup`.

The receiving side (a StaticTableChip or direct Lasso argument) is deferred to M11.
M10 declares the bus interaction; the debug checker can verify balance when a mock
static table chip provides the matching receive.

### D6. Mul/DivMod: intermediate carry with sub-limb decomposition

u64 multiplication in BabyBear (p ≈ 2^31) produces intermediate carries that exceed p.
The carry from limb 1 (c1) can reach 2^31, exceeding BabyBear's modulus.

**Solution**: Decompose c1 into two sub-limbs: `c1 = c1_lo + c1_hi * 2^16`, where
`c1_lo ∈ [0, 2^16)` and `c1_hi ∈ [0, 2^15)`. Both fit in the range-check table.

The carry from the 64-bit boundary (c2) can reach ≈2^56, far exceeding p.
**Solution**: Split c2 into two field elements: `c2 = c2_lo + c2_hi * 2^16`,
then further decompose as needed until each piece < p.

**Risk**: This area has the highest complexity. The exact carry decomposition may need
iteration during implementation. The column count estimate below is approximate.

### D7. Interaction with failed transactions

All opcode constraints are **gated by `is_real`** only (not by tx success/failure).
The executor MUST pre-filter: only include txs whose operations are valid (no overflow,
no division by zero). Failed txs are excluded from the batch trace entirely.

This matches the current architecture where `ExecutionResult` only contains successfully
executed instructions. Abort semantics are handled at the executor level, not in the AIR.

---

## 3. Detailed Specifications

### A1. Range Check Wiring

**Goal**: Send all unrange-checked u64 limbs and StrictIneq diffs to the RangeCheck bus.

#### A1-1. SSMC range checks

**File**: `crates/tabula-proof/src/air/chips/ssmc/columns.rs`

Add after `segment_is_touched` (line 67):
```rust
// ── Range check half-decomposition (M10-A1) ──
pub key_l0_halves: LimbHalves<T>,        // 2 FE
pub key_l1_halves: LimbHalves<T>,        // 2 FE
pub ordering_diff0_halves: LimbHalves<T>, // 2 FE
pub ordering_diff1_halves: LimbHalves<T>, // 2 FE
```

**Width impact**: 45 → 53 (+8)

**File**: `crates/tabula-proof/src/air/chips/ssmc/air.rs`

Add `constrain_range_check_halves()`:
- `key.limb0 = key_l0_halves.lo + key_l0_halves.hi * 2^15`
- `key.limb1 = key_l1_halves.lo + key_l1_halves.hi * 2^15`
- `key_ordering.diff0 = ordering_diff0_halves.lo + ordering_diff0_halves.hi * 2^15`
- `key_ordering.diff1 = ordering_diff1_halves.lo + ordering_diff1_halves.hi * 2^15`

Add `send_range_checks()`:
- 9 sends: `key_l0_halves.lo/hi`, `key_l1_halves.lo/hi`, `key.limb2`,
  `ordering_diff0_halves.lo/hi`, `ordering_diff1_halves.lo/hi`, `key_ordering.diff2`
- All with multiplicity `is_real`

**File**: `crates/tabula-proof/src/air/chips/ssmc/trace.rs`

Populate `key_l0_halves`, `key_l1_halves`, `ordering_diff0_halves`, `ordering_diff1_halves`
from limb values: `lo = val & 0x7FFF`, `hi = val >> 15`.

#### A1-2. Merge range checks

**File**: `crates/tabula-proof/src/air/chips/merge/columns.rs`

Add after `tc_changed` (line 77):
```rust
pub key_l0_halves: LimbHalves<T>,        // 2 FE
pub key_l1_halves: LimbHalves<T>,        // 2 FE
pub ordering_diff0_halves: LimbHalves<T>, // 2 FE
pub ordering_diff1_halves: LimbHalves<T>, // 2 FE
```

**Width impact**: 52 → 60 (+8)

Same constraint and trace pattern as SSMC (A1-1).

#### A1-3. Execution range checks

**File**: `crates/tabula-proof/src/air/chips/execution/columns.rs`

Add after `slot_written` (line 125):
```rust
// ── Range check half-decomposition (M10-A1) ──
pub access_r_l0_halves: LimbHalves<T>,  // 2 FE
pub access_r_l1_halves: LimbHalves<T>,  // 2 FE
pub tau_l0_halves: LimbHalves<T>,       // 2 FE
pub tau_l1_halves: LimbHalves<T>,       // 2 FE
```

**Width impact**: 170 → 178 (+8)

Add `constrain_range_check_halves()`:
- Reconstruct access_r limbs and tau limbs from halves.
- Gate by `is_real * is_access` (only when accessing memory).

Add `send_range_checks()`:
- 10 sends: `access_r_l0_halves.lo/hi`, `access_r_l1_halves.lo/hi`, `access_r.limb2`,
  `tau_l0_halves.lo/hi`, `tau_l1_halves.lo/hi`, `tau_limbs.limb2`
- Multiplicity: `is_real * is_access`

#### A1-4. SortedMem ordering diff range checks

**File**: `crates/tabula-proof/src/air/chips/sorted_mem/columns.rs`

Add after existing `tau_l1_halves` (around line 110):
```rust
pub ordering_diff0_halves: LimbHalves<T>, // 2 FE
pub ordering_diff1_halves: LimbHalves<T>, // 2 FE
```

**Width impact**: 42 → 46 (+4)

Add to existing `send_range_checks()`:
- 5 additional sends: `ordering_diff0_halves.lo/hi`, `ordering_diff1_halves.lo/hi`,
  `ordering.diff2`
- Multiplicity: `is_real` (same as existing r/tau sends)

Add `constrain_ordering_diff_halves()`:
- `ordering.diff0 = ordering_diff0_halves.lo + ordering_diff0_halves.hi * 2^15`
- `ordering.diff1 = ordering_diff1_halves.lo + ordering_diff1_halves.hi * 2^15`

#### A1 Tests

Per chip (~5 tests each, ~20 total):
- `range_check_sends_correct_count` — verify multiplicities
- `range_check_halves_reconstruction` — correct lo/hi split
- `corrupted_half_fails` — wrong lo/hi → constraint fail
- `out_of_range_limb_detected` — limb > 2^30 detected via range check

---

### A2. Lex Ordering Direction Enforcement

**Goal**: At (t,c) segment boundaries, enforce `(t_i, c_i) <lex (t_{i+1}, c_{i+1})`.

#### A2-1. New columns (per chip)

Add to SSMC, Merge, SortedMem, ColumnMeta columns:
```rust
// ── Lex ordering direction (M10-A2) ──
pub lex_diff_is_table: T,  // 1: table_id changed; 0: same table, col_id changed
pub lex_table_diff: T,     // next.table_id - local.table_id - 1 (when table changes)
pub lex_col_diff: T,       // next.col_id - local.col_id - 1 (when col changes)
```

**Width impact per chip**: +3

- SSMC: 53 → 56
- Merge: 60 → 63
- SortedMem: 46 → 49
- ColumnMeta: 28 → 31

#### A2-2. Constraints

New `constrain_lex_ordering_direction()` in each chip:

```
Gate: both_real * tc_changed  (segment boundary between real rows)

// Boolean
assert_bool(lex_diff_is_table)

// Case 1: table_id changed
lex_diff_is_table * (lex_table_diff - (next.table_id - local.table_id - 1)) = 0
lex_diff_is_table * lex_table_diff → range-check [0, 2^16) via RangeCheck bus

// Case 2: same table, col_id changed
(1 - lex_diff_is_table) * (next.table_id - local.table_id) = 0  // table must be equal
(1 - lex_diff_is_table) * (lex_col_diff - (next.col_id - local.col_id - 1)) = 0
(1 - lex_diff_is_table) * lex_col_diff → range-check [0, 2^16) via RangeCheck bus
```

The range check on `lex_table_diff` proves `next.table_id - local.table_id - 1 ∈ [0, 2^16)`,
i.e., `next.table_id > local.table_id`. Similarly for `lex_col_diff`.

Range-check multiplicities:
- SSMC/Merge/SortedMem: `is_real * tc_changed * lex_diff_is_table` for table diff,
  `is_real * tc_changed * (1 - lex_diff_is_table)` for col diff
- ColumnMeta: `is_real * (1 - table_same * col_same) * lex_diff_is_table` (analogous)

#### A2-3. ColumnMeta special case

ColumnMeta currently uses `table_diff_iz` and `col_diff_iz` for uniqueness. The new
lex ordering direction supplements (not replaces) the existing uniqueness constraint.
Update the comment at line 74-75 from "deferred to M10" to reference the new constraint.

#### A2 Tests (~12 total)

Per chip:
- `lex_ordering_correct` — ascending (t,c) passes
- `lex_ordering_reversed_table_fails` — t decreases → fails
- `lex_ordering_same_table_reversed_col_fails` — same t, c decreases → fails
- `lex_ordering_same_tc_fails` — duplicate (t,c) still caught by existing uniqueness

---

### B1. Cmp Opcode Constraint

**Goal**: Constrain comparison operations (Eq, Ne, Lt, Lte, Gt, Gte).

#### B1-1. New columns

**File**: `crates/tabula-proof/src/air/chips/execution/columns.rs`

Add after range-check columns:
```rust
// ── Cmp opcode (M10-B1) ──
pub cmp_is_eq: T,     // 1 FE — one-hot: Eq
pub cmp_is_ne: T,     // 1 FE — one-hot: Ne
pub cmp_is_lt: T,     // 1 FE — one-hot: Lt
pub cmp_is_lte: T,    // 1 FE — one-hot: Lte
pub cmp_is_gt: T,     // 1 FE — one-hot: Gt
pub cmp_is_gte: T,    // 1 FE — one-hot: Gte

pub cmp_lt_witness: T,    // 1 FE — 1 if lhs < rhs (ordering witness)
pub cmp_eq_witness: T,    // 1 FE — 1 if lhs = rhs (equality witness)

pub cmp_ineq: StrictIneq<T>,            // 3 FE — gap decomposition
pub cmp_ineq_diff0_halves: LimbHalves<T>, // 2 FE — range check
pub cmp_ineq_diff1_halves: LimbHalves<T>, // 2 FE — range check

pub cmp_eq_combined_iz: IsZero<T>,       // 2 FE — IsZero on combined diff
```

**Width impact**: +17 → Execution 178 → 195

#### B1-2. Constraints

New `constrain_cmp()`:

```
Gate: is_real * op_cmp

// Sub-selector one-hot
sum = cmp_is_eq + cmp_is_ne + cmp_is_lt + cmp_is_lte + cmp_is_gt + cmp_is_gte
op_cmp * (sum - 1) = 0
Boolean: each cmp_is_* ∈ {0,1}

// Witness booleans
Boolean: cmp_lt_witness, cmp_eq_witness
op_cmp * cmp_lt_witness * cmp_eq_witness = 0  // mutually exclusive

// Equality detection: combined_diff = reconstruct(src1) - reconstruct(src2)
// constrain_is_zero(combined_diff, cmp_eq_combined_iz)
// cmp_eq_witness = cmp_eq_combined_iz.is_zero

// Ordering proof (conditional direction):
// When cmp_lt_witness=1: StrictIneq proves src2 > src1 (gap = src2 - src1 - 1)
// When cmp_lt_witness=0 AND cmp_eq_witness=0: StrictIneq proves src1 > src2
//
// Constraint:
//   cmp_lt_witness: ineq_target = reconstruct(src2) - reconstruct(src1) - 1
//   NOT cmp_lt_witness AND NOT cmp_eq_witness: ineq_target = reconstruct(src1) - reconstruct(src2) - 1
//   cmp_eq_witness: no ordering constraint needed (equality handles it)
//
// Combined: ineq_target = cmp_lt_witness * (s2-s1-1) + (1-lt-eq) * (s1-s2-1)
// constrain_strict_ineq(ineq_target, cmp_ineq)
// + range checks on cmp_ineq diff limbs

// Result binding (per sub-selector):
for each written slot s:
  cmp_is_eq  * (slots[s][0] - cmp_eq_witness) = 0
  cmp_is_ne  * (slots[s][0] - (1 - cmp_eq_witness)) = 0
  cmp_is_lt  * (slots[s][0] - cmp_lt_witness) = 0
  cmp_is_lte * (slots[s][0] - cmp_lt_witness - cmp_eq_witness) = 0
  cmp_is_gt  * (slots[s][0] - (1 - cmp_lt_witness - cmp_eq_witness)) = 0
  cmp_is_gte * (slots[s][0] - (1 - cmp_lt_witness)) = 0
  // Higher limbs zero:
  op_cmp * slot_written[s] * slots[s][1] = 0
  op_cmp * slot_written[s] * slots[s][2] = 0
  // Not null:
  op_cmp * slot_written[s] * slot_is_null[s] = 0
```

Range-check sends for cmp_ineq diffs: 5 sends with multiplicity `is_real * op_cmp * (1 - cmp_eq_witness)`.

#### B1-3. Trace generation

In `generate_execution_trace`, for `Opcode::Cmp(sub_op)`:
- Set the appropriate `cmp_is_*` sub-selector
- Compute `cmp_lt_witness` and `cmp_eq_witness` from actual operand values
- Compute `cmp_ineq` diff limbs: gap decomposition of the appropriate direction
- Compute `cmp_ineq_diff*_halves`
- Compute `cmp_eq_combined_iz` inverse witness

#### B1 Tests (~8)

- `cmp_eq_true/false` — equality detection
- `cmp_lt_true/false` — less-than ordering
- `cmp_lte_boundary` — equal case returns true for lte
- `cmp_gt/gte` — greater-than variants
- `cmp_wrong_result_fails` — incorrect result in dst slot
- `cmp_wrong_lt_witness_fails` — wrong ordering witness

---

### B2. Hash Opcode Constraint

**Goal**: Constrain single-permutation Hash via PoseidonPermutation bus.

#### B2-1. New columns

**File**: `crates/tabula-proof/src/air/chips/execution/columns.rs`

```rust
// ── Hash opcode (M10-B2) ──
pub hash_perm_input: [T; 16],   // 16 FE — Poseidon permutation input
pub hash_perm_output: [T; 8],   // 8 FE — permutation output (digest)
```

**Width impact**: +24 → Execution 195 → 219

#### B2-2. Constraints

New `constrain_hash()`:

```
Gate: is_real * op_hash

// Input composition: hash_perm_input = [domain_tag, n, ComEnc(src1), ComEnc(src2), pad...]
// For single-permutation (≤2 inputs at W=3, using 6 of 8 rate elements):
//   hash_perm_input[0] = DOMAIN_TAG_HASH (constant)
//   hash_perm_input[1] = n (number of inputs, from witness)
//   hash_perm_input[2..5] = src1_val[0..3] (first input ComEnc)
//   hash_perm_input[5..8] = src2_val[0..3] (second input, if n=2; zero-padded if n=1)
//   hash_perm_input[8..16] = 0 (capacity, initialized to zero for fresh sponge)

// Result binding: perm_output[0..W] → destination slot
for each written slot s, for i in 0..W:
  op_hash * slot_written[s] * (slots[s][i] - hash_perm_output[i]) = 0
  op_hash * slot_written[s] * slot_is_null[s] = 0  // hash result not null
```

New `send_hash_permutation()`:

```
send(PoseidonPermutation,
     values: [hash_perm_input[0..16], hash_perm_output[0..8]],  // 24 elements
     multiplicity: is_real * op_hash)
```

#### B2-3. Operand handling

Hash needs operand values but uses a variable number of inputs. For M10 (single permutation):
- Add `op_hash` to `needs_src1` and `needs_src2` in `constrain_operand_selectors`
- `needs_src2` gated by `n ≥ 2` (additional witness boolean `hash_has_src2`)

#### B2-4. Wide (W=8) limitation

Hash produces a Digest (8 FE) but ExecutionChip is generic over W=3.
The current slot width can only store 3 FE per slot.

**M10 approach**: For W=3, only the first 3 elements of the digest are stored in the slot.
This is a **lossy truncation** — programs that need full 8-FE digests must use W=8 chip instance.
Full Wide support is deferred.

**Alternative**: Store the full 8-FE digest across 3 slots (slot_written sets 3 flags).
This is architecturally cleaner but more complex. Deferred unless needed.

#### B2 Tests (~5)

- `hash_single_input` — one input, correct digest
- `hash_two_inputs` — two inputs, correct composition
- `hash_wrong_output_fails` — tampered perm_output → bus imbalance
- `hash_wrong_domain_tag_fails` — wrong domain tag in perm_input

---

### B3. Lookup Opcode Constraint

**Goal**: Emit LogUp interaction for static table lookups.

#### B3-1. New InteractionKind

**File**: `crates/tabula-proof/src/air/interaction.rs`

```rust
StaticTableLookup = 9,  // ExecutionChip → StaticTableChip (M11)
```

#### B3-2. Constraints

**File**: `crates/tabula-proof/src/air/chips/execution/air.rs`

New `constrain_lookup()`:

```
Gate: is_real * op_lookup

// Lookup result binding (access_val → destination slot):
for each written slot s, for i in 0..W:
  op_lookup * slot_written[s] * (slots[s][i] - access_val[i]) = 0
  op_lookup * slot_written[s] * slot_is_null[s] = 0  // Lookup always non-null
```

New `send_static_table_lookup()`:

```
send(StaticTableLookup,
     values: [access_t, access_c, access_r.limb0/1/2, access_val[W]],  // 3+W+2 elements
     multiplicity: is_real * op_lookup)
```

**Note**: The receiving side (StaticTableChip) is deferred to M11.
The debug checker will verify bus balance when paired with a mock receiver.

#### B3-3. is_access update

Currently `is_access = op_read + op_write`. Lookup does NOT set `is_access`
(it doesn't participate in the state memory argument per proof-spec §10.5).
This is already correct — no change needed.

However, Lookup DOES populate `access_t`, `access_c`, `access_r`, `access_val` columns.
Add a new flag `uses_access_cols = op_read + op_write + op_lookup` for column population gating.
Or: rename/extend trace generation to populate access columns for Lookup too.

#### B3 Tests (~4)

- `lookup_result_binding` — access_val flows to destination slot
- `lookup_bus_send_correct` — StaticTableLookup bus send with correct fingerprint
- `lookup_not_null` — slot_is_null must be 0

---

### B4. Com_empty Verification

**Goal**: When `is_empty_old=1` or `is_empty_new=1`, verify `Com = Poseidon(0x00 || t || c)`.

#### B4-1. New columns

**File**: `crates/tabula-proof/src/air/chips/column_meta/columns.rs`

```rust
// ── Com_empty verification (M10-B4) ──
pub empty_perm_input: [T; 16],   // 16 FE — Poseidon input for Com_empty
pub empty_perm_output: [T; 8],   // 8 FE — expected Com_empty digest
pub has_empty_check: T,          // 1 FE — 1 if any empty verification needed
```

**Width impact**: 31 → 56 (+25)

**Note**: This is a large column addition. If column budget is a concern, the empty
check can share the Poseidon bus with SSMC/Merge by reusing existing hash infrastructure.
Alternative: compute `Com_empty` outside the AIR (as a public input) and constrain equality
directly. This avoids 25 new columns at the cost of adding `Com_empty` values to public inputs.

**Decision**: Use the public-input approach if column budget is tight. For now, specify the
full in-AIR approach.

#### B4-2. Constraints

New `constrain_com_empty()`:

```
has_empty_check = is_empty_old + is_empty_new - is_empty_old * is_empty_new
// (1 if either is empty; avoids double-counting when both are empty)

// Compose Poseidon input for Com_empty:
// empty_perm_input = [0x00, table_id, col_id, 0, 0, ..., 0]  (domain tag 0x00)
has_empty_check * (empty_perm_input[0] - DOMAIN_SSMC) = 0
has_empty_check * (empty_perm_input[1] - table_id) = 0
has_empty_check * (empty_perm_input[2] - col_id) = 0
has_empty_check * empty_perm_input[3..16] = 0  // zero padding

// Verify Com_old = empty_perm_output when is_empty_old:
for i in 0..8:
  is_empty_old * (com_old[i] - empty_perm_output[i]) = 0

// Verify Com_new = empty_perm_output when is_empty_new:
for i in 0..8:
  is_empty_new * (com_new[i] - empty_perm_output[i]) = 0
```

New `send_com_empty_permutation()`:

```
send(PoseidonPermutation,
     values: [empty_perm_input[16], empty_perm_output[8]],
     multiplicity: is_real * has_empty_check)
```

#### B4 Tests (~4)

- `com_empty_old_verified` — empty old column, correct hash
- `com_empty_new_verified` — delete-all case, correct hash
- `com_empty_wrong_hash_fails` — wrong com_old for empty column
- `com_empty_domain_tag` — verify domain tag is 0x00

---

### C1. Mul Opcode Constraint

**Goal**: Constrain u64 multiplication via limb cross-products with carry chain.

#### C1-1. Approach

For `dst = lhs * rhs` (both u64, result u64, no overflow):

Decompose both operands into 30+30+4 limbs:
```
lhs = a0 + a1 * 2^30 + a2 * 2^60   (a0,a1 ∈ [0, 2^30), a2 ∈ [0,16))
rhs = b0 + b1 * 2^30 + b2 * 2^60
dst = r0 + r1 * 2^30 + r2 * 2^60
```

Cross-limb products grouped by power:
```
T0 = a0*b0                     (contributes to 2^0)
T1 = a0*b1 + a1*b0             (contributes to 2^30)
T2 = a0*b2 + a1*b1 + a2*b0     (contributes to 2^60)
T3 = a1*b2 + a2*b1             (overflow: 2^90)
T4 = a2*b2                     (overflow: 2^120)
```

Carry chain:
```
T0 = r0 + c0 * 2^30
T1 + c0 = r1 + c1 * 2^30
T2 + c1 = r2 + c2 * 16        (boundary at 2^64: 16 = 2^64 / 2^60)
T3 + c2 = 0                   (no overflow)
T4 = 0                        (no overflow)
```

**BabyBear challenge**: `c1` can reach `≈ 2^31`, exceeding `p ≈ 2^31`.

**Solution**: Decompose `c1 = c1_lo + c1_hi * 2^16`:
- `c1_lo ∈ [0, 2^16)` — fits in range-check table
- `c1_hi ∈ [0, 2^15)` — fits in range-check table

Similarly, `c2` can reach ≈ 2^56 >> p. Decompose into 4 sub-limbs:
```
c2 = c2_a + c2_b * 2^16 + c2_c * 2^32 + c2_d * 2^48
```
Each sub-limb ∈ [0, 2^16). But c2_d ∈ [0, 2^8) (since c2 < 2^56).

**No-overflow constraints** (field equations):
```
T3 + c2_a + c2_b * 2^16 + c2_c * 2^32 + c2_d * 2^48 = 0
T4 = 0
```

Since T3 = a1*b2 + a2*b1 (max ≈ 2*15*(2^30-1) ≈ 2^35) and c2 (max ≈ 2^56),
the equation T3 + c2 = 0 must hold in the integers (not mod p).
This means T3 = -c2, but both are non-negative, so T3 + c2 = 0 iff T3 = 0 AND c2 = 0.
And T4 = a2*b2 = 0 iff a2 = 0 OR b2 = 0.

**Simplification**: For no-overflow multiplication, we need:
```
c2 = 0  AND  T3 = 0  AND  T4 = 0
```
Which means:
```
a0*b2 + a1*b1 + a2*b0 + c1 = r2   (c2 = 0, so no carry out)
a1*b2 + a2*b1 = 0
a2*b2 = 0
```

This simplifies significantly! The c2 decomposition is unnecessary.

#### C1-2. New columns

```rust
// ── Mul opcode (M10-C1) ──
pub mul_c0: T,                    // 1 FE — carry from limb 0→1
pub mul_c0_halves: LimbHalves<T>, // 2 FE — range check [0, 2^30)
pub mul_c1_lo: T,                 // 1 FE — carry low [0, 2^16)
pub mul_c1_hi: T,                 // 1 FE — carry high [0, 2^15)
```

**Width impact**: +5 → Execution 219 → 224

#### C1-3. Constraints

New `constrain_arith_mul()`:

```
Gate: is_real * op_arith * arith_is_mul

// Operand limb access (via src1_sel / src2_sel linkage):
a0 = src1_val[0], a1 = src1_val[1], a2 = src1_val[2]
b0 = src2_val[0], b1 = src2_val[1], b2 = src2_val[2]

// Find result slot (the written slot):
for each slot s where slot_written[s]:
  r0 = slots[s][0], r1 = slots[s][1], r2 = slots[s][2]

// Carry chain:
// (1) a0*b0 = r0 + mul_c0 * 2^30
gate * (a0*b0 - r0 - mul_c0 * 2^30) = 0

// (2) a0*b1 + a1*b0 + mul_c0 = r1 + (mul_c1_lo + mul_c1_hi * 2^16) * 2^30
gate * (a0*b1 + a1*b0 + mul_c0 - r1 - (mul_c1_lo + mul_c1_hi * 2^16) * 2^30) = 0

// (3) a0*b2 + a1*b1 + a2*b0 + mul_c1_lo + mul_c1_hi * 2^16 = r2
gate * (a0*b2 + a1*b1 + a2*b0 + mul_c1_lo + mul_c1_hi * 2^16 - r2) = 0

// (4) No overflow:
gate * (a1*b2 + a2*b1) = 0
gate * (a2*b2) = 0

// Range checks:
gate * assert_bool(mul_c0_halves.lo, mul_c0_halves.hi) // via LimbHalves
send_range_check(mul_c0_halves.lo, mul_c0_halves.hi, mul_c1_lo, mul_c1_hi)
// mul_c0: [0, 2^30) via halves; mul_c1_lo: [0, 2^16) direct; mul_c1_hi: [0, 2^15) direct
```

**Range-check sends**: 4 additional (mul_c0_halves.lo/hi, mul_c1_lo, mul_c1_hi)
with multiplicity `is_real * op_arith * arith_is_mul`.

#### C1 Tests (~6)

- `mul_small_values` — 3 * 5 = 15
- `mul_large_values` — (2^30 - 1) * 2 = 2^31 - 2
- `mul_carry_propagation` — values that produce carries
- `mul_wrong_result_fails` — incorrect dst
- `mul_overflow_detected` — product > 2^64 fails (T3 ≠ 0)
- `mul_range_check_sends` — correct number of range-check bus sends

---

### C2. DivMod Opcode Constraint

**Goal**: Constrain `lhs = q * rhs + rem` with `0 ≤ rem < rhs` and `rhs ≠ 0`.

#### C2-1. Operand selector fix

**File**: `crates/tabula-proof/src/air/chips/execution/air.rs`

Add `op_divmod` to `needs_src1` and `needs_src2`:

```rust
let needs_src1: AB::Expr = local.op_arith.clone().into()
    + local.op_cmp.clone().into()
    + local.op_divmod.clone().into()  // ← ADD THIS
    + ...;

let needs_src2: AB::Expr = local.op_arith.clone().into()
    + local.op_cmp.clone().into()
    + local.op_divmod.clone().into()  // ← ADD THIS
    + ...;
```

#### C2-2. Approach

DivMod produces two results: `dst_q` (quotient) and `dst_r` (remainder).
Both must be written to separate slots.

The constraint: `lhs = q * rhs + rem` is a multiplication plus addition.
The same BabyBear carry-overflow concern applies: `q * rhs` may produce intermediate
values exceeding p.

**Approach**: Reuse the Mul carry chain to prove `q * rhs = lhs - rem`:
```
q0*d0 = (lhs0 - rem0) + c0 * 2^30          // with borrow handling
q0*d1 + q1*d0 + c0 = (lhs1 - rem1) + c1 * 2^30
q0*d2 + q1*d1 + q2*d0 + c1 = (lhs2 - rem2)
```

Plus: `rem < rhs` via StrictIneq (gap = rhs - rem - 1, range-checked).
Plus: `rhs ≠ 0` via IsZero on combined rhs (inverse witness).

#### C2-3. New columns

```rust
// ── DivMod opcode (M10-C2) ──
pub divmod_c0: T,                    // 1 FE — carry from product limb 0
pub divmod_c0_halves: LimbHalves<T>, // 2 FE
pub divmod_c1_lo: T,                 // 1 FE — carry from product limb 1 (lo)
pub divmod_c1_hi: T,                 // 1 FE — carry from product limb 1 (hi)
pub divmod_rem_ineq: StrictIneq<T>,  // 3 FE — gap: rhs - rem - 1
pub divmod_rem_diff0_halves: LimbHalves<T>, // 2 FE — range check
pub divmod_rem_diff1_halves: LimbHalves<T>, // 2 FE — range check
pub divmod_rhs_iz: IsZero<T>,        // 2 FE — rhs ≠ 0 check
pub divmod_second_dst: T,            // 1 FE — index of second written slot
```

**Width impact**: +15 → Execution 224 → 239

#### C2-4. Dual slot write

DivMod writes to two slots. Currently `slot_written` supports writing one slot per
instruction. For DivMod, two `slot_written[s]` flags must be set.

**Constraint**: `op_divmod * (Σ slot_written[s] - 2) = 0` — exactly 2 slots written.
The first written slot holds quotient, the second holds remainder (ordered by slot index).

This changes `constrain_slot_carry`: for DivMod, TWO slots are allowed to change.
The carry constraint becomes: `(1 - slot_written[s]) * (next.slots[s] - local.slots[s]) = 0`
which already works correctly (it only carries slots that are NOT written).

#### C2-5. Constraints

New `constrain_divmod()`:

```
Gate: is_real * op_divmod

// Identify q_slot and r_slot (two written slots, ordered by index)
// q = first written slot values, r = second written slot values

// Division identity: lhs = q * rhs + rem
// Rewritten: q * rhs = lhs - rem
// Carry chain (same structure as Mul C1, with a=q, b=rhs, result=lhs-rem):
divmod_gate * (q0*d0 - (l0 - rem0) - divmod_c0 * 2^30) = 0
divmod_gate * (q0*d1 + q1*d0 + divmod_c0 - (l1 - rem1)
    - (divmod_c1_lo + divmod_c1_hi * 2^16) * 2^30) = 0
divmod_gate * (q0*d2 + q1*d1 + q2*d0 + divmod_c1_lo + divmod_c1_hi * 2^16
    - (l2 - rem2)) = 0
// No overflow:
divmod_gate * (q1*d2 + q2*d1) = 0
divmod_gate * (q2*d2) = 0

// Remainder bound: rem < rhs via StrictIneq
constrain_strict_ineq(rem, rhs, divmod_rem_ineq)
// + range checks on divmod_rem_ineq diffs

// Non-zero divisor: rhs ≠ 0
constrain_is_zero(rhs_combined, divmod_rhs_iz)
divmod_gate * divmod_rhs_iz.is_zero = 0  // rhs_combined ≠ 0

// Result not null:
op_divmod * slot_is_null[q_slot] = 0
op_divmod * slot_is_null[r_slot] = 0
```

#### C2 Tests (~6)

- `divmod_basic` — 7 / 3 = (2, 1)
- `divmod_exact_division` — 6 / 3 = (2, 0)
- `divmod_large_values` — large dividend/divisor
- `divmod_wrong_quotient_fails` — incorrect q
- `divmod_remainder_too_large_fails` — rem ≥ rhs
- `divmod_zero_divisor_fails` — rhs = 0

---

## 4. Column Impact Summary

| Chip | M9 Width | Range Check (A1) | Lex Order (A2) | Opcodes (B/C) | M10 Width |
|------|----------|-----------------|----------------|---------------|-----------|
| ExecutionChip | 170 | +8 | — | +17 Cmp, +24 Hash, +5 Mul, +15 DivMod | **239** |
| GlobalSSMC | 45 | +8 | +3 | — | **56** |
| GlobalMerge | 52 | +8 | +3 | — | **63** |
| GlobalSortedMem | 42 | +4 | +3 | — | **49** |
| ColumnMeta | 28 | — | +3 | +25 Com_empty | **56** |
| PoseidonChip | 93 | — | — | — | **93** |
| RangeCheckChip | 2 | — | — | — | **2** |
| **Total** | **432** | **+28** | **+12** | **+86** | **558** |

**Preprocessed traces**: PoseidonChip 17 (unchanged).

---

## 5. New InteractionKind

| Variant | Tag | Added In |
|---------|-----|----------|
| `StaticTableLookup` | 9 | M10-B3 |

Total: 9 variants (was 8 in M9).

---

## 6. Test Plan

| Phase | New Tests | Updated Tests | Description |
|-------|-----------|---------------|-------------|
| A1 | ~20 | ~10 | Range check sends per chip, corrupted halves, width assertions |
| A2 | ~12 | ~4 | Lex ordering direction per chip |
| B1 | ~8 | 0 | Cmp 6 sub-operators + failure cases |
| B2 | ~5 | 0 | Hash composition + Poseidon bus |
| B3 | ~4 | 0 | Lookup bus send + result binding |
| B4 | ~4 | ~2 | Com_empty hash verification |
| C1 | ~6 | ~2 | Mul carry chain + overflow |
| C2 | ~6 | ~2 | DivMod identity + remainder bound |
| **Total** | **~65** | **~20** | |

Expected test count after M10: 250 + 65 = ~315.

---

## 7. Success Criteria

- [ ] All u64 limbs range-checked across all chips (SSMC, Merge, Execution, SortedMem diffs)
- [ ] (t,c) segment boundaries enforce strict lexicographic increasing order
- [ ] Cmp produces correct boolean result for all 6 sub-operators
- [ ] Hash composes correct Poseidon input and binds output to destination slot
- [ ] Lookup emits StaticTableLookup bus interaction
- [ ] Mul carry chain correct for all valid u64 multiplications
- [ ] DivMod division identity holds with remainder bound and non-zero divisor
- [ ] Com_empty verified via Poseidon when is_empty_old/new = 1
- [ ] DivMod operand selectors fixed (op_divmod in needs_src1/needs_src2)
- [ ] ~315 tests pass, zero clippy warnings
- [ ] `cargo test --workspace` passes

---

## 8. Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Mul/DivMod carry overflow in BabyBear | High | c1 sub-limb decomposition proven correct; c2=0 for no-overflow simplifies significantly |
| ColumnMeta +25 cols for Com_empty | Medium | Alternative: public-input approach (pre-compute Com_empty outside AIR). Decision at implementation time |
| Hash Wide(W=8) not supported | Medium | M10 supports lossy truncation to W=3. Full W=8 deferred. Document limitation |
| Execution chip growing to 239 cols | Low | Still well within STARK trace width limits. Optimization via Layout B deferred |
| StaticTableLookup has no receiver in M10 | Low | Debug checker tests use mock receiver. Real receiver in M11 |

---

## 9. File Change Summary

| File | Changes |
|------|---------|
| `air/interaction.rs` | Add `StaticTableLookup = 9` |
| `air/chips/ssmc/columns.rs` | +8 cols (LimbHalves × 4) |
| `air/chips/ssmc/air.rs` | `constrain_range_check_halves`, `send_range_checks`, `constrain_lex_ordering_direction` |
| `air/chips/ssmc/trace.rs` | Populate halves + lex columns |
| `air/chips/merge/columns.rs` | +8 cols (LimbHalves × 4) |
| `air/chips/merge/air.rs` | `constrain_range_check_halves`, `send_range_checks`, `constrain_lex_ordering_direction` |
| `air/chips/merge/trace.rs` | Populate halves + lex columns |
| `air/chips/sorted_mem/columns.rs` | +4 cols (LimbHalves × 2) + 3 lex cols |
| `air/chips/sorted_mem/air.rs` | Ordering diff range checks + lex direction |
| `air/chips/sorted_mem/trace.rs` | Populate new columns |
| `air/chips/execution/columns.rs` | +8 range + 17 Cmp + 24 Hash + 5 Mul + 15 DivMod = +69 cols |
| `air/chips/execution/air.rs` | `constrain_cmp`, `constrain_hash`, `send_hash_permutation`, `constrain_lookup`, `send_static_table_lookup`, `constrain_arith_mul`, `constrain_divmod`, `send_range_checks`, fix operand selectors |
| `air/chips/execution/trace.rs` | Populate Cmp/Hash/Lookup/Mul/DivMod columns |
| `air/chips/column_meta/columns.rs` | +3 lex + 25 Com_empty = +28 cols |
| `air/chips/column_meta/air.rs` | `constrain_lex_ordering_direction`, `constrain_com_empty`, `send_com_empty_permutation` |
| `air/chips/column_meta/trace.rs` | Populate lex + Com_empty columns |
| `tests/chips/ssmc.rs` | Range check + lex tests |
| `tests/chips/merge.rs` | Range check + lex tests |
| `tests/chips/sorted_mem.rs` | Ordering diff range check + lex tests |
| `tests/chips/execution.rs` | Cmp/Hash/Lookup/Mul/DivMod tests, width assertion update |
| `tests/chips/column_meta.rs` | Lex + Com_empty tests |
| `tests/infra/bus.rs` | Update bus balance tests for new interactions |
