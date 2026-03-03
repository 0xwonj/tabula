# M8 AIR Chips — Design & Execution Plan

> **Status: ✅ COMPLETE** — Implemented in commit `6c396b8`.

## 1. Scope

M8 implements four new AIR chips + ColumnMeta finalization, completing all AIR
constraint surfaces for Layer B+C proof. LogUp wiring is M9; M8 carries witness
columns and constrains all LOCAL semantics.

| Chip | Purpose | ~Cols | M8 Phase |
|------|---------|-------|----------|
| GlobalSSMCChip\<W\> | Column commitment (sorted entries + hash chain) | 29 | M8-1 |
| GlobalMergeChip\<W\> | State update (3-way merge proof) | 36 | M8-2 |
| PoseidonChip | Poseidon2 permutation AIR | 53 | M8-3 |
| ExecutionChip | Instruction trace + SSA + clock | ~80 | M8-4 |
| ColumnMeta update | Com\_empty hash, existence rules | 27 | M8-5 |

---

## 2. Pre-Requisite Decisions

### P1. PoseidonChip: Build from Scratch

Plonky3 provides `p3-poseidon2` (permutation function only). No `Poseidon2Air`
exists. Must build using the Keccak-f AIR (p3-keccak-air) as reference pattern:
3-file chip structure, one row per round.

### P2. Hash Chain: Iterative (Prototype)

SSMC/Merge hash chains use iterative hash chain per proof-spec §4.2:
`h_i = Poseidon(h_{i-1} || key_i || val_i)`. Sponge optimization deferred.

### P3. Width Class: Standard (W=3) Only

All M8 chips instantiated at W=3 (U64/I64). Narrow (W=1) and Wide (W=8)
deferred. Programs with Bool/Digest columns handled via max-width zero-padding
or rejection (TBD after M9).

### P4. ExecutionChip Witness: InstructionTrace Type

ExecutionResult lacks per-instruction data (opcode, slot values). Solution:
define `InstructionTrace` type in `tabula-core`, add optional recording to
interpreter. For M8 testing, construct traces manually in test fixtures.

### P5. Hash/Bus Wiring: All Deferred to M9

M8 chips carry hash\_acc and access-log columns but do NOT constrain:
- Hash chain transitions (PoseidonPermutation LogUp)
- Memory bus (Execution ↔ SortedMem LogUp)
- SSMC membership lookups
- Merge completeness LogUp
- ColumnMeta join lookups
- Range check LogUp

M8 constrains only LOCAL/transition semantics. M9 wires inter-chip LogUp.

---

## 3. Phase M8-1: GlobalSSMCChip

### 3.1 Column Layout (29 columns, W=3)

```
GlobalSsmcCols<T, const W: usize>
#[repr(C)]

  ── Identity ──
  is_real: T                          1
  table_id: T                         1
  col_id: T                           1

  ── Entry ──
  key: U64Limbs<T>                    3   row key (sorted ascending)
  value: [T; W]                       W   Tier 1 ComEnc (non-null)

  ── Boundary ──
  is_first: T                         1   first entry of (t,c) segment
  is_last: T                          1   last entry of (t,c) segment

  ── Hash accumulator (populated, NOT constrained in M8) ──
  hash_acc: [T; 8]                    8   running Poseidon hash

  ── Ordering gadgets ──
  key_ordering: StrictIneq<T>         5   key < next_key within segment
  table_diff_iz: IsZero<T>            2   lex ordering across segments
  col_diff_iz: IsZero<T>              2   lex ordering across segments
  tc_changed: T                       1   derived boolean

  Total: 1+1+1+3+3+1+1+8+5+2+2+1 = 29 (W=3)
```

### 3.2 Constraints (M8 — local/transition only)

1. **Boolean** (4): is\_real, is\_first, is\_last, tc\_changed
2. **is\_real prefix**: monotonic 1→0
3. **Key sorted uniqueness** (within segment):
   `is_real ∧ ¬is_last ⟹ key_next > key` via StrictIneq
4. **Boundary flags**:
   - First real row: `is_first = 1`
   - `both_real ∧ tc_changed ⟹ next.is_first = 1`
   - `both_real ∧ ¬tc_changed ⟹ next.is_first = 0`
   - `real_to_padding ⟹ is_last = 1`
   - `both_real ∧ tc_changed ⟹ is_last = 1`
   - `both_real ∧ ¬tc_changed ⟹ is_last = 0` (except last row of segment)
5. **Segment lex ordering**:
   `tc_changed ∧ both_real ⟹ (t,c) <lex (next.t, next.c)` via IsZero
6. **(M9 deferred)**: hash\_acc transition, commitment join, membership LogUp

### 3.3 Trace Generation

Input: `&[SsmcEntry]` — sorted by `(table_id, col_id, key)`.

```rust
pub struct SsmcEntry {
    pub table_id: u32,
    pub col_id: u16,
    pub key: u64,
    pub value: Vec<BabyBear>,       // w(T) field elements
    pub hash_acc: [BabyBear; 8],    // precomputed from Poseidon
}
```

Algorithm:
1. Assign is\_real, table\_id, col\_id, key, value from entries
2. Compute is\_first/is\_last from (t,c) segment boundaries
3. Set hash\_acc from precomputed witness
4. Pad to power-of-2
5. Second pass: populate IsZero, StrictIneq witnesses

Source data: `ColumnWitness.old_state` (SsmcList) provides sorted (key, value)
pairs per column. Flatten across all SSMC columns, sort globally by (t,c,key).
Compute hash\_acc using `PoseidonHasher::hash_domain()`.

### 3.4 Tests

Valid: single column, multi-column, single entry, many entries, boundary flags.
Invalid: unsorted keys, duplicate keys, wrong boundary flags, broken is\_real prefix.
Edge: all padding, single entry per column.

---

## 4. Phase M8-2: GlobalMergeChip

### 4.1 Column Layout (36 columns, W=3)

```
GlobalMergeCols<T, const W: usize>
#[repr(C)]

  ── Identity ──
  is_real: T                          1
  table_id: T                         1
  col_id: T                           1

  ── Merged key ──
  key: U64Limbs<T>                    3   strictly increasing

  ── Source encoding ──
  s1: T                               1   }  (0,0)=old_only  (0,1)=write_only
  s0: T                               1   }  (1,0)=both       (1,1)=delete

  ── Values ──
  old_val: [T; W]                     W   from OldList
  write_val: [T; W]                   W   from WriteSet
  new_val: [T; W]                     W   result for NewList

  ── Flags ──
  in_new: T                           1   1 if in NewList, 0 if deleted

  ── Hash accumulator (NOT constrained in M8) ──
  hash_acc: [T; 8]                    8   running hash of NewList

  ── Ordering gadgets ──
  key_ordering: StrictIneq<T>         5
  table_diff_iz: IsZero<T>            2
  col_diff_iz: IsZero<T>              2
  tc_changed: T                       1

  Total: 1+1+1+3+1+1+3+3+3+1+8+5+2+2+1 = 36 (W=3)
```

### 4.2 Constraints (M8 — local/transition only)

1. **Boolean** (5): is\_real, s1, s0, in\_new, tc\_changed
2. **is\_real prefix**
3. **Key sorted uniqueness**: StrictIneq within same (t,c) segment
4. **Source encoding derived selectors**:
   ```
   is_old_only   = (1-s1)(1-s0)
   is_write_only = (1-s1)·s0
   is_both       = s1·(1-s0)
   is_delete     = s1·s0
   ```
5. **Merge logic** (gated by is\_real):
   - `is_old_only ⟹ new_val = old_val ∧ in_new = 1`
   - `is_write_only ⟹ new_val = write_val ∧ in_new = 1`
   - `is_both ⟹ new_val = write_val ∧ in_new = 1`
   - `is_delete ⟹ in_new = 0`
6. **Delete null witness**: `is_delete ⟹ write_val = 0^W` (canonical null)
7. **Segment lex ordering**: same as SSMC
8. **Boundary**: first real row starts a fresh segment, etc.
9. **(M9 deferred)**: hash\_acc transition, completeness LogUp, commitment join

### 4.3 Trace Generation

Input: `&[MergeRow]` — flattened from `MergeTrace` per column, sorted by (t,c,key).

```rust
pub struct MergeRow {
    pub table_id: u32,
    pub col_id: u16,
    pub key: u64,
    pub source: MergeSource,         // OldOnly | WriteOnly | Both
    pub old_val: Option<Vec<BabyBear>>,
    pub write_val: Option<Vec<BabyBear>>,
    pub new_val: Option<Vec<BabyBear>>,
    pub in_new: bool,
    pub hash_acc: [BabyBear; 8],     // precomputed
}
```

Source data: `ColumnWitness.merge_trace` (Option\<MergeTrace\>) — None for
untouched or SMT columns. Flatten all MergeStep entries across touched SSMC
columns, sort by (t,c,key). Map `MergeSource` → (s1,s0). Compute hash\_acc
for NewList entries using PoseidonHasher.

### 4.4 Tests

Valid: old\_only, write\_only, both, delete cases; multi-column merge; empty merge.
Invalid: wrong new\_val for source type, in\_new=1 on delete, unsorted keys,
wrong source encoding.
Edge: single-entry column, all deletes (empty new state).

---

## 5. Phase M8-3: PoseidonChip

### 5.1 Configuration

```
Poseidon2-BabyBear (from p3-baby-bear):
  Field:          BabyBear (p = 2013265921)
  Width:          16
  S-box:          x^7
  Full rounds:    8  (4 initial + 4 final)
  Partial rounds: 13
  Total rounds:   21
  Digest:         8 FE (squeeze from first 8 elements)
```

### 5.2 Column Layout (53 columns)

One row per round. 21 rows per permutation invocation.

```
PoseidonCols<T>
#[repr(C)]

  ── State ──
  state: [T; 16]                      16  state BEFORE this round's operations

  ── S-box intermediates (degree ≤ 3) ──
  sbox_y2: [T; 16]                    16  y^2 where y = state[i] + rc[i]
  sbox_y3: [T; 16]                    16  y^3 = y · y2

  ── Round control ──
  round_ctr: T                         1  round index (0..20)
  is_full_round: T                     1  1 for full, 0 for partial
  is_first_round: T                    1  1 for round 0 of a permutation
  is_last_round: T                     1  1 for round 20
  is_real: T                           1  row gating

  Total: 16 + 16 + 16 + 5 = 53
```

For partial rounds, only `sbox_y2[0]` and `sbox_y3[0]` are meaningful.
Elements 1..15 use identity S-box (no intermediates needed, constrained to
equal `state[i] + rc[i]` directly).

### 5.3 Constraints

**S-box (full round, all 16 elements, gated by is\_full\_round):**
```
For each i in 0..16:
  y = state[i] + rc[round][i]          (linear, implicit)
  sbox_y2[i] = y^2                     (degree 2)
  sbox_y3[i] = y · sbox_y2[i]          (degree 2)
  sbox_out[i] = sbox_y3[i] · sbox_y2[i]^2   — NO, this is degree 3
```

Refined decomposition:
```
  sbox_y2[i] = (state[i] + rc)^2         (degree 2: 1 constraint)
  sbox_y3[i] = (state[i] + rc) · sbox_y2[i]   (degree 2: 1 constraint)
  sbox_out[i] = sbox_y3[i] · sbox_y2[i]^2     (degree 3: 1 constraint)

  Total S-box: sbox_out = y^3 · y^4 = y^7 ✓
```

3 constraints per element, 48 constraints per full round.

**S-box (partial round, element 0 only, gated by ¬is\_full\_round):**
- Same 3 constraints for element 0
- Elements 1..15: `sbox_out[i] = state[i] + rc[round][i]` (identity, 15 constraints)
- Total: 3 + 15 = 18 constraints per partial round

**Linear layer (MDS, every round):**
```
next.state = MDS × sbox_out    (16 linear constraints)
```

Where `next.state` is the next row's state column (transition constraint).
The MDS matrix is the Poseidon2 external/internal diffusion matrix from
`p3-poseidon2`. External rounds use the external diffusion; internal rounds
use the internal diffusion.

**Round control:**
```
round_ctr: increments by 1 each row (within a permutation)
is_full_round: 1 for rounds 0..3 and 17..20, 0 for rounds 4..16
is_first_round: round_ctr = 0
is_last_round: round_ctr = 20
```

**Permutation boundary:**
```
is_last_round: the sbox_out after MDS gives the permutation output
Next row (if real): starts a new permutation (is_first_round=1)
```

**Bus interaction (M9):**
- `InteractionKind::PoseidonPermutation`
- Send: (input[0..15], output[0..15]) per permutation
- Receive: SSMC/Merge hash chain rows, Hash instruction

### 5.4 Trace Generation

For each Poseidon permutation invocation:
1. Start with 16-element input state
2. For each of 21 rounds:
   a. Compute y = state + round\_constants
   b. Apply S-box (full or partial)
   c. Apply MDS linear layer
   d. Record state, sbox\_y2, sbox\_y3
3. Output = final state after round 20

Use `p3_poseidon2::Poseidon2` to compute the permutation for witness
correctness, then verify intermediate values match the AIR trace.

### 5.5 Tests

Valid: known test vector, multiple permutations, padding.
Invalid: corrupted S-box intermediate, wrong state transition, wrong MDS output.
Soundness: forged sbox\_y2 (wrong square).

---

## 6. Phase M8-4: ExecutionChip

### 6.1 Witness Enrichment

**New type in `tabula-core` (or `tabula-proof`):**

```rust
pub struct InstructionRecord {
    pub opcode: u8,              // discriminant (0..11)
    pub is_access: bool,         // Read/Write
    pub dst_slots: SmallVec<Slot>,
    pub src_values: SmallVec<BabyBear>,  // resolved inputs (FE)
    pub dst_values: SmallVec<BabyBear>,  // computed outputs (FE)
    // For access instructions:
    pub access_table: Option<u32>,
    pub access_col: Option<u16>,
    pub access_row_key: Option<u64>,
    pub access_value: Option<Vec<BabyBear>>,
    pub access_is_null: Option<bool>,
}
```

**M8 approach**: For testing, construct InstructionRecord sequences manually.
Integration with interpreter (optional feature flag) deferred to M9 prep.

### 6.2 Column Layout (~80 columns, MAX\_SLOTS=16, W=3)

```
ExecutionCols<T, const S: usize, const W: usize>
#[repr(C)]

  ── Control ──
  is_real: T                           1
  tx_index: T                          1   transaction index in batch

  ── Opcode (one-hot, 12 selectors) ──
  op_read: T                           1
  op_write: T                          1
  op_arith: T                          1   (Add/Sub/Mul via sub-selector)
  op_divmod: T                         1
  op_cmp: T                            1   (Eq/Neq/Lt/Lte/Gt/Gte via sub)
  op_not: T                            1
  op_and: T                            1
  op_or: T                             1
  op_assert: T                         1
  op_hash: T                           1
  op_select: T                         1
  op_lookup: T                         1

  ── Clock & Timestamp ──
  is_access: T                         1   = op_read + op_write
  clk: T                               1   access counter
  tau: T                                1   = clk + 1 (when is_access)

  ── Access log (populated when is_access=1) ──
  access_t: T                          1
  access_c: T                          1
  access_r: U64Limbs<T>                3
  access_is_write: T                   1
  access_val: [T; W]                   W
  access_is_null: T                    1

  ── SSA Slots (Layout A: carry) ──
  slots: [[T; W]; S]                   S×W   slot values (val only, no null)
  slot_is_null: [T; S]                 S     per-slot null flag
  slot_written: [T; S]                 S     1 if this instruction writes to slot s

  Total: 1+1+12+3+6+W+S×W+S+S
       = 23 + W + S(W+2)
       = 23 + 3 + 16(5)
       = 106
```

Note: 106 columns is wide. Optimizations (Layout B, fewer opcode selectors)
planned for post-M9. For M8 correctness, accept the width.

### 6.3 Constraints (M8 — local only)

**1. Boolean** (12 opcode + is\_access + access\_is\_write + S×slot\_written):
   `12 + 2 + S` boolean constraints.

**2. Opcode exactly-one** (when is\_real):
   `op_read + op_write + ... + op_lookup = 1`

**3. is\_access derived**:
   `is_access = op_read + op_write`

**4. Clock recurrence** (transition, both\_real):
   `next.clk = local.clk + local.is_access`
   First row: `clk = 0`

**5. Timestamp binding** (local, is\_real):
   `is_access · (tau - clk - 1) = 0`

**6. SSA slot carry** (transition, both\_real, per slot s):
   `(1 - local.slot_written[s]) · (next.slots[s][i] - local.slots[s][i]) = 0`
   for each i in 0..W. Slots not written this instruction carry forward.

**7. Per-opcode semantics** (gated by opcode selector):

| Opcode | Constraint summary |
|--------|-------------------|
| Read | `slot_written[dst_val]=1`, `slot_written[dst_is_null]=1`, access log populated |
| Write | no slot written, access log populated with src values |
| Arith(Add) | `slots[dst] = slots[lhs] + slots[rhs]` (per limb) |
| Arith(Sub) | `slots[dst] = slots[lhs] - slots[rhs]` |
| Arith(Mul) | `slots[dst] = slots[lhs] · slots[rhs]` (field mul, NOT integer mul) |
| DivMod | `slots[dst_q] · slots[rhs] + slots[dst_r] = slots[lhs]` + range checks |
| Cmp | `slots[dst] ∈ {0,1}`, result correct per comparison op |
| Not | `slots[dst] = 1 - slots[src]` (boolean not) |
| And | `slots[dst] = slots[lhs] · slots[rhs]` (boolean and) |
| Or | `slots[dst] = slots[lhs] + slots[rhs] - slots[lhs]·slots[rhs]` |
| Assert | `slots[cond] = 1` (or tx fails) |
| Select | `slots[dst] = cond·slots[if_true] + (1-cond)·slots[if_false]` |
| Hash | slot\_written[dst]=1, PoseidonPermutation bus (M9) |
| Lookup | slot\_written[dst]=1, static table bus (M9) |

**8. Memory bus (M9 deferred)**:
   `is_access=1` rows → LogUp into GlobalSortedMem

### 6.4 Opcode Priority

**M8-4a (core, implement first):** Read, Write, Arith(Add/Sub/Mul), Assert, Select
**M8-4b (secondary):** Cmp, Not, And, Or, DivMod
**M8-4c (bus-dependent):** Hash, Lookup (meaningful only after M9 LogUp)

### 6.5 Trace Generation

Input: `&[InstructionRecord]` per transaction, concatenated for the batch.

Algorithm:
1. Initialize all slots to zero
2. For each instruction:
   a. Set opcode one-hot selector
   b. Set is\_access, clk (running sum), tau
   c. For access instructions: populate access log columns
   d. Execute instruction: compute output values, set dst slot
   e. Set slot\_written flags
   f. Write all slot values (carry from previous + new writes)
3. Pad to power-of-2

### 6.6 Tests

Valid: simple arithmetic sequence, read-then-write, SSA carry across rows.
Invalid: wrong slot carry, broken clock recurrence, wrong arithmetic result.
Edge: single instruction, max slots used, failed tx (Assert violation).

---

## 7. Phase M8-5: ColumnMeta Finalization

### 7.1 New Constraints (additions to existing ColumnMetaChip)

Current ColumnMeta (M6/M7) has: lex ordering, untouched binding, boolean flags.
M8 adds:

1. **Com\_empty hash verification** (requires PoseidonChip, wired in M9):
   `is_empty_old = 1 ⟹ Com_old = Poseidon(0x00 || t || c)`
   `is_empty_new = 1 ⟹ Com_new = Poseidon(0x00 || t || c)`
   (Carried as witness columns; hash verification via PoseidonPermutation bus in M9)

2. **Existence rules** (carried as invariants, enforced via LogUp in M9):
   - `tag=0 ∧ is_empty_old=0 ⟹ GlobalSSMC has rows for (t,c)` (SsmcMembership bus)
   - `tag=0 ∧ is_touched=1 ⟹ GlobalMerge has rows for (t,c)` (MergeCompleteness bus)

3. **is\_touched consistency** (local constraint):
   `is_touched = 0 ⟹ is_empty_new = is_empty_old` (untouched column state preserved)

4. **Empty→non-empty transition**:
   `is_empty_old=1 ∧ is_touched=1 ⟹ is_empty_new=0` (writes to empty column make it non-empty)
   Exception: if all writes are deletes, column may remain empty (edge case, deferred).

### 7.2 Column Layout Update

Add to existing ColumnMetaCols (width 27 → unchanged, fields already present):
- No new columns needed; constraints use existing fields.
- `is_empty_old`, `is_empty_new`, `is_touched` already in the layout.

### 7.3 Tests

Valid: empty→non-empty transition, untouched consistency.
Invalid: is\_touched=0 but is\_empty changed, is\_empty\_old=1 with wrong Com\_old.

---

## 8. LogUp Fingerprint Specifications (M9 Prep)

Define fingerprint tuples for each bus. M8 chips declare these in
`ChipMeta::interactions()` but LogUp wiring is M9.

### 8.1 Memory Bus

```
Tuple: (t, c, r_limbs[3], tau_limbs[3], is_write, val[W], val_is_null)
Width: 3 + 3 + 1 + W + 1 = 11 (W=3)

Send: ExecutionChip (m = is_access)
Recv: GlobalSortedMem (m = is_real · (1 - is_init))
```

### 8.2 RangeCheck Bus

```
Tuple: (value)
Width: 1

Send: Any chip needing u16 range proof (StrictIneq limbs, U64Limbs)
Recv: RangeCheckChip (preprocessed [0, 2^16))
```

### 8.3 SsmcMembership Bus

```
Tuple: (t, c, key_limbs[3], val[W])
Width: 2 + 3 + W = 8 (W=3)

Send: GlobalSortedMem init rows (m = is_real · is_init · [open via SSMC])
Recv: GlobalSSMC (m = is_real)
```

### 8.4 MergeCompleteness Bus (two sub-buses)

**OldList sub-bus:**
```
Tuple: (t, c, key_limbs[3], old_val[W])
Width: 2 + 3 + W = 8

Send: GlobalSSMC old entries (m = is_real)
Recv: GlobalMerge old_only/both/delete rows (m = is_real · (is_old_only + is_both + is_delete))
```

**WriteSet sub-bus:**
```
Tuple: (t, c, key_limbs[3], write_val[W])
Width: 2 + 3 + W = 8

Send: GlobalSortedMem write-set rows (m = is_real · is_last_for_key · has_written)
Recv: GlobalMerge write_only/both/delete rows (m = is_real · (is_write_only + is_both + is_delete))
```

### 8.5 ColumnMetaJoin Bus

```
Tuple: (t, c, tag, Com_old[8], is_empty_old, is_touched)
Width: 2 + 1 + 8 + 1 + 1 = 13

Send: GlobalSSMC first-of-segment rows, GlobalMerge first-of-segment rows
Recv: ColumnMeta (m = is_real)
```

### 8.6 PoseidonPermutation Bus

```
Tuple: (input_state[16], output_state[16])
Width: 32

Send: SSMC hash chain rows, Merge hash chain rows, Hash instruction
Recv: PoseidonChip (m = is_real · is_first_round, once per permutation)
```

---

## 9. Implementation Schedule & Dependencies

### 9.1 Dependency Graph

```
M8-0: Decisions (this document)
  │
  ├── M8-1: GlobalSSMCChip ──────────┐
  │     (reuses SortedMem patterns)   │
  │                                   ├── M8-5: ColumnMeta finalization
  ├── M8-2: GlobalMergeChip ─────────┘
  │     (similar to SSMC)             │
  │                                   │
  ├── M8-3: PoseidonChip ────────────┘
  │     (standalone, complex)           (needed for Com_empty in tests)
  │
  └── M8-4: ExecutionChip
        (independent, needs InstructionTrace)
```

M8-1 and M8-2 are parallel. M8-3 is independent. M8-4 is independent.
M8-5 depends on M8-1, M8-2 (integration), and M8-3 (Com\_empty test).

### 9.2 Phase Timeline (estimated)

| Phase | Effort | Blocker |
|-------|--------|---------|
| M8-1: GlobalSSMCChip | 3-4 days | None (clear spec) |
| M8-2: GlobalMergeChip | 3-4 days | None (parallel with M8-1) |
| M8-3: PoseidonChip | 5-7 days | None (S-box math, MDS matrix) |
| M8-4a: ExecutionChip (core ops) | 5-7 days | InstructionTrace type |
| M8-4b: ExecutionChip (secondary ops) | 2-3 days | M8-4a |
| M8-5: ColumnMeta + integration | 2-3 days | M8-1, M8-2, M8-3 |

### 9.3 Parallelization

- M8-1 ∥ M8-2 (both are sorted-list chips)
- M8-3 ∥ M8-1/M8-2 (PoseidonChip is standalone)
- M8-4 ∥ M8-1/M8-2/M8-3 (ExecutionChip is independent)

Maximum parallelism: all 4 in parallel (if multiple developers).
Sequential path: M8-1 → M8-2 → M8-3 → M8-4 → M8-5.

---

## 10. Files to Create/Modify

### New files:
```
crates/proof/src/air/chips/ssmc/mod.rs
crates/proof/src/air/chips/ssmc/columns.rs
crates/proof/src/air/chips/ssmc/air.rs
crates/proof/src/air/chips/ssmc/trace.rs

crates/proof/src/air/chips/merge/mod.rs
crates/proof/src/air/chips/merge/columns.rs
crates/proof/src/air/chips/merge/air.rs
crates/proof/src/air/chips/merge/trace.rs

crates/proof/src/air/chips/poseidon/mod.rs
crates/proof/src/air/chips/poseidon/columns.rs
crates/proof/src/air/chips/poseidon/air.rs
crates/proof/src/air/chips/poseidon/trace.rs
crates/proof/src/air/chips/poseidon/constants.rs  (round constants)

crates/proof/src/air/chips/execution/mod.rs
crates/proof/src/air/chips/execution/columns.rs
crates/proof/src/air/chips/execution/air.rs
crates/proof/src/air/chips/execution/trace.rs
```

### Modified files:
```
crates/proof/src/air/chips/mod.rs       — TabulaAir enum + dispatch
crates/proof/src/air/mod.rs             — re-exports
crates/proof/src/air/chips/column_meta/air.rs  — new constraints
crates/proof/src/air/chips/column_meta/trace.rs — updated tests
crates/core/src/event.rs                — InstructionRecord type (M8-4)
```

---

## 11. Testing Strategy

Each chip: ~15-20 tests via `debug_check()`.

| Chip | Valid | Invalid | Soundness | Edge | Total |
|------|-------|---------|-----------|------|-------|
| SSMC | 5 | 4 | 2 | 3 | ~14 |
| Merge | 5 | 5 | 2 | 3 | ~15 |
| Poseidon | 3 | 4 | 3 | 2 | ~12 |
| Execution (core) | 5 | 5 | 2 | 3 | ~15 |
| Execution (secondary) | 4 | 3 | 1 | 2 | ~10 |
| ColumnMeta update | 3 | 2 | 1 | 1 | ~7 |

Estimated total: ~73 new tests. Combined with existing 100: ~173 tests.

### Verification per phase:
```bash
cargo test -p tabula-proof --features stark
cargo clippy -p tabula-proof --features stark --all-targets
```

---

## 12. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| PoseidonChip S-box degree | MEDIUM | Use y2/y3 intermediates (degree ≤ 3) |
| Poseidon MDS matrix extraction | MEDIUM | Use p3-poseidon2 internal API or hardcode |
| ExecutionChip width (~106 cols) | LOW | Acceptable for M8; Layout B in future |
| InstructionTrace integration | LOW | M8 uses manual test fixtures; bridge later |
| StrictIneq soundness (no range check) | LOW | Already identified; M9 wires RangeCheck |
| Width-class heterogeneity | LOW | W=3 only in M8; handled post-M9 |
