# Research: Eliminating GlobalSortedMem

> Status: Active research (Feb 2025)
> Prerequisite: [current-proof-architecture.md](./current-proof-architecture.md)

## 1. Thesis

Tabula's IR enforces True SSA + Normal Form rules (NF-1 through NF-4),
which **eliminate the need for intra-tx RAM consistency arguments**.
This is the core design insight that differentiates Tabula from
Cairo/Miden/StarkNet, all of which require sorted memory arguments
for general-purpose RAM.

Given this, GlobalSortedMem may be doing unnecessary work. This document
analyzes whether it can be eliminated entirely, and what architecture
would replace it.

## 2. What SortedMem Does Today

SortedMem serves four roles (see current-proof-architecture.md §6):

| Role | Description | Intra-tx? | Inter-tx? |
|------|-------------|-----------|-----------|
| **R1** | Bind Read values to base state | YES | YES |
| **R2** | Order accesses across txs in a batch | NO | YES |
| **R3** | Extract write-set for Merge | YES | YES |
| **R4** | Signal accessed columns to ColumnMeta | metadata | metadata |

**R1** is the only role that serves soundness for single-tx batches.
**R2** only matters when multiple txs in a batch access the same key.
**R3** is write-set delivery, which could be done by other means.
**R4** is metadata plumbing.

## 3. Analysis: Can Each Role Be Replaced?

### 3.1 R1: Read Value Verification

**Current path**: Exec Read -> C1 -> SortedMem init row -> C2 -> SSMC membership

**Alternative**: Exec Read -> (direct bus) -> SSMC membership

For a non-null Read of key `r` in column `(t,c)`:
- ExecutionChip claims `Read(t,c,r) = val`
- Send `(t, c, r[3], val[W])` directly to SSMC membership bus
- SSMC verifies: the entry `(key=r, value=val)` exists in its sorted list

For a null Read (key absent) from a non-empty column:
- ExecutionChip claims `Read(t,c,r) = null`
- SSMC must prove non-membership: key `r` is NOT in the sorted list
- Requires a **gap proof**: two adjacent SSMC entries `(key_i, key_{i+1})`
  such that `key_i < r < key_{i+1}`

For a null Read from an empty column:
- ExecutionChip claims `Read(t,c,r) = null`
- Column has `is_empty_old = 1` in ColumnMeta
- No SSMC entries exist for this column
- Need a mechanism to verify the column is empty

**Verdict: R1 can be replaced.** SSMC already has sorted entries.
Membership is direct LogUp. Non-membership requires gap proofs
(which we were already planning in M11, just sourced differently).
Empty-column reads need a lightweight bus to ColumnMeta.

### 3.2 R2: Inter-Tx Ordering

**Current mechanism**: SortedMem sorts all accesses by `(t,c,r,tau)`.
When tx_0 writes key k and tx_1 reads key k, the sorted order ensures
tx_1's read sees tx_0's written value.

**Three options**:

**Option A: Forbid inter-tx key conflicts (protocol-level)**

The sequencer guarantees no two txs in a batch access the same `(t,c,r)`.
Each tx reads independently from base state. No ordering needed.

- Pro: Eliminates R2 entirely
- Con: Constrains the sequencer; some applications have hot keys

**Option B: Allow conflicts, handle with lightweight mechanism**

If tx_0 writes `(t,c,r)=v1` and tx_1 reads `(t,c,r)`:
- tx_1 should see `v1`, not the base state value
- Need a mechanism to route tx_1's read to tx_0's write

This could be a small "conflict resolution table" that maps
`(t,c,r)` -> latest value across txs. But proving "latest"
requires some ordering, which reintroduces sorting.

- Pro: Handles conflicts
- Con: Reintroduces partial sorted memory

**Option C: Single-tx batches**

Restrict batches to exactly one tx. No inter-tx problem.

- Pro: Simplest possible architecture
- Con: Severe throughput limitation

**Verdict: R2 is the hardest to replace.** Option A is the cleanest
but requires protocol-level constraints. Option B partially reintroduces
the problem. Option C is too restrictive.

### 3.3 R3: Write-Set Extraction

**Current path**: SortedMem -> is_last_for_key + has_written -> C4 -> Merge

**Alternative**: Exec Write -> (direct bus) -> Merge

ExecutionChip already knows which instructions are Writes. It could
send `(t, c, r[3], val[W], is_delete)` directly to Merge via C4.

For single-tx batches or non-conflicting batches: every Write is a
final write (no coalescing needed). Direct send works perfectly.

For conflicting batches: if tx_0 writes `(t,c,r)=v1` and tx_1 writes
`(t,c,r)=v2`, Merge should only see the final write `v2`.
Coalescing is needed.

**Verdict: R3 is easily replaced for non-conflicting batches.**
For conflicting batches, coalescing is harder without sorting.

### 3.4 R4: Segment Metadata

**Current path**: SortedMem -> C7 SortedMemMeta -> ColumnMeta (has_sorted_mem)

**Alternative**: ColumnMeta already receives from SSMC (C6 CommitmentVerif)
and Merge (C6). These buses bind com_old and com_new to ColumnMeta entries.
If ColumnMeta has an entry for a column, the LogUp balances. If missing,
it doesn't balance -> proof fails.

The `has_sorted_mem` flag was an extra check that SortedMem agrees with
ColumnMeta about which columns are accessed. Without SortedMem, this
check is unnecessary -- the SSMC and Merge commitment bindings are
sufficient for soundness.

**Verdict: R4 is trivially replaceable.** Remove has_sorted_mem and C7.

## 4. Proposed Architecture: SortedMem-Free

### 4.1 Assumptions

**Primary assumption: Non-conflicting batches.**

Within a batch, no two txs access (read or write) the same `(t,c,r)`.
This means:
- Every tx reads from base state (no inter-tx dependencies)
- Every tx's writes are final (no coalescing)
- The union of all txs' write-sets IS the batch write-set

This is enforced at the protocol/sequencer level, NOT in the proof.
The proof system assumes it. (See §6 for soundness analysis.)

### 4.2 New Bus Topology

**Eliminated**:
- C1 Memory bus (13-wide, between Execution and SortedMem)
- C7 SortedMemMeta bus (3-wide, between SortedMem and ColumnMeta)
- GlobalSortedMem chip (67 columns)

**New/modified buses**:

| ID | Bus | Width | Sender | Receiver | Purpose |
|----|-----|-------|--------|----------|---------|
| 1 | ReadMembership | 8 | Execution | SSMC | Non-null reads: verify (key,val) exists |
| 2 | ReadNonMembership | 5 | Execution | SSMC | Null reads from non-empty columns: verify key absent |
| 3 | MergeOldList | 8 | SSMC | Merge | Old entries for touched columns (unchanged) |
| 4 | WriteSet | 9 | Execution | Merge | Write entries directly from execution |
| 5 | PoseidonPerm | 24 | hashers | Poseidon | (unchanged) |
| 6 | CommitmentVerif | 12 | SSMC,Merge | ColumnMeta | (unchanged) |
| 7 | EmptyColRead | 2 | Execution | ColumnMeta | Null reads from empty columns |
| 8 | RangeCheck | 1 | all | RangeCheck | (unchanged) |
| 9 | StaticTableLookup | 8 | Execution | StaticTable | (unchanged) |

**Bus count**: 9 (same as before, different topology).

### 4.3 New Data Flow

```
                  C1 ReadMembership
ExecutionChip  ─────────────────►  GlobalSSMC
    │              C2 ReadNonMembership    │
    │           ─────────────────►         │
    │                                      │ C3 MergeOldList
    │  C4 WriteSet                         ▼
    │  ──────────────────────────►  GlobalMerge
    │                                      │
    │                                      │ C6 CommitmentVerif
    │  C7 EmptyColRead                     ▼
    │  ──────────────────────►     ColumnMeta
    │                                      │
    │              C5 Poseidon             │
    └──────────────────────────►   PoseidonChip
    │
    └──────────► C8 RangeCheck    RangeCheckChip
```

### 4.4 Read Verification (Without SortedMem)

**Case 1: Non-null Read from non-empty column**

```
Exec: Read(t,c,r) = val, val_is_null = 0
  └─ sends (t, c, r[3], val[W]) via C1 ReadMembership
SSMC: receives, verifies entry (key=r, value=val) exists in sorted list
```

Multiplicity on Execution side: `is_real * op_read * (1 - access_is_null)`
Multiplicity on SSMC side: `is_real * mult_witness` (same as today for C2)

**Case 2: Null Read from non-empty column**

```
Exec: Read(t,c,r) = null, val_is_null = 1
  └─ sends (t, c, r[3]) via C2 ReadNonMembership
SSMC: gap query row receives, proves key_i < r < key_{i+1}
```

This requires SSMC to have **gap query rows** interleaved with entry rows
(see §4.5 below).

Multiplicity on Execution side: `is_real * op_read * access_is_null * (1 - is_empty_col)`
(need `is_empty_col` witness in ExecutionChip to distinguish case 2 vs 3)

**Case 3: Null Read from empty column**

```
Exec: Read(t,c,r) = null, val_is_null = 1, is_empty_col = 1
  └─ sends (t, c) via C7 EmptyColRead
ColumnMeta: receives, verifies is_empty_old = 1 for this (t,c)
```

Multiplicity on Execution side: `is_real * op_read * access_is_null * is_empty_col`
Multiplicity on ColumnMeta side: `is_real * empty_read_mult`

### 4.5 SSMC Gap Query Rows

To handle non-membership (Case 2), SSMC gains **gap query rows**
interleaved at the correct sorted positions:

```
SSMC segment for (t=1, c=2):
  row 0: (entry, key=5, val=v1)         is_entry=1
  row 1: (gap,   query_key=7)           is_entry=0, is_gap_query=1
  row 2: (entry, key=10, val=v2)        is_entry=1
  row 3: (gap,   query_key=12)          is_entry=0, is_gap_query=1
  row 4: (entry, key=15, val=v3)        is_entry=1
```

**Gap query constraints**:
- Key ordering: gap query's key strictly between adjacent entries
- No hash contribution: gap queries don't contribute to hash chain
  (hash_acc carries forward unchanged)
- LogUp: gap query sends to C2 ReadNonMembership bus
- Multiplicity: each gap query matched to exactly one Execution null-read

**Boundary gap queries** (before first entry or after last):
- Before first: `is_first=1`, only upper bound constraint (`query_key < first_entry_key`)
- After last: `is_last=1`, only lower bound constraint (`last_entry_key < query_key`)

**Column additions to SSMC**: +3 cols (is_gap_query, query_key lower/upper StrictIneq
can reuse existing key_ordering gadget with modified gating)

### 4.6 Write-Set Delivery

```
Exec: Write(t,c,r,val,is_null)
  └─ sends (t, c, r[3], val[W], is_delete) via C4 WriteSet
Merge: receives, same as today
```

Multiplicity on Execution side: `is_real * op_write`
Multiplicity on Merge side: `is_real * (1 - is_old_only)` (same as today for C4)

No change to Merge chip. The bus tuple is identical; only the sender changes
from SortedMem to Execution.

### 4.7 ColumnMeta Changes

Remove:
- `has_sorted_mem` column
- C7 SortedMemMeta bus receive

Add:
- `empty_read_mult` witness column (for EmptyColRead bus)
- C7 EmptyColRead bus receive

Net: approximately same column count.

### 4.8 ExecutionChip Changes

Add:
- `is_empty_col` witness column (1 col): prover indicates the read target
  column is empty. Required to distinguish Case 2 vs Case 3.

Modify:
- Replace C1 Memory send with:
  - C1 ReadMembership send (gated by op_read * (1 - access_is_null))
  - C2 ReadNonMembership send (gated by op_read * access_is_null * (1 - is_empty_col))
  - C7 EmptyColRead send (gated by op_read * access_is_null * is_empty_col)
  - C4 WriteSet send (gated by op_write)

Net column change: +1 (is_empty_col).

## 5. Column Budget Comparison

| Chip | Current | Proposed | Delta |
|------|---------|----------|-------|
| ExecutionChip | 278 | 279 | +1 |
| GlobalSortedMem | 67 | 0 | **-67** |
| GlobalSSMC | 66 | ~72 | +6 |
| GlobalMerge | 74 | 74 | 0 |
| ColumnMeta | 56 | 56 | 0 |
| PoseidonChip | 112 | 112 | 0 |
| RangeCheckChip | 2 | 2 | 0 |
| **Total** | **655** | **595** | **-60** |

**Net savings: ~60 columns (9% reduction).**

But column count isn't the full picture. SortedMem has O(A) rows where
A = total access count across all txs. Eliminating it saves A rows of
67-wide trace. The SSMC gap query rows add at most G rows (G = number of
null reads) of ~72-wide trace. Since G <= A, the net row savings is
(A - G) * 67 columns, which is significant.

## 6. Soundness Analysis

### 6.1 Non-Conflicting Batch Assumption

If the assumption is violated (two txs access same key), what happens?

**Scenario**: tx_0 writes `(t,c,r)=v1`, tx_1 reads `(t,c,r)`.

Without SortedMem, tx_1's Read sends `(t,c,r,base_val)` to SSMC membership.
SSMC verifies that `base_val` is the committed value. This is **correct
with respect to the base state** but **incorrect with respect to tx_0's write**.

The proof would accept, but the state transition would be wrong:
tx_1 sees the old value instead of tx_0's write.

**This is not a soundness failure of the proof system itself.** The proof
correctly verifies that:
1. tx_0's execution is correct (reads from base, writes v1)
2. tx_1's execution is correct (reads base_val from base, which is genuine)
3. The write-set includes tx_0's write (v1) and tx_1's write (if any)
4. The new state reflects all writes

The issue is **semantic**: the batch semantics assumed sequential execution
`S_{i+1} = apply(S_i, WriteSet_i)`, but the proof verified parallel
execution (all txs read from base state). These diverge when txs conflict.

**Resolution options**:

a. **Define batch semantics as parallel**: All txs read from base state.
   Write conflicts resolved by priority (e.g., last tx index wins).
   This is a valid semantic model used by some systems.

b. **Enforce non-conflict at protocol level**: Sequencer must ensure
   no conflicts. If violated, the batch is invalid. The proof doesn't
   catch it, but the protocol rejects the batch before proving.

c. **Add a lightweight conflict check**: A small auxiliary table that
   verifies no two txs in the batch accessed the same key.
   This is cheaper than full SortedMem.

### 6.2 Read Soundness (Non-null)

Claim: if Execution claims `Read(t,c,r) = val` with `val_is_null=0`,
and the LogUp balances, then `(key=r, value=val)` genuinely exists in
the SSMC commitment for `(t,c)`.

Proof: SSMC receives via C1 ReadMembership with `mult = mult_witness`.
The entry `(key=r, value=val)` must exist as a real SSMC row. SSMC's
hash chain computes Com_old over all entries. ColumnMeta verifies
Com_old against oldRoot via SMT path. Therefore val is the committed
value for key r in column (t,c). QED.

### 6.3 Read Soundness (Null, Non-empty)

Claim: if Execution claims `Read(t,c,r) = null` from a non-empty column,
and the LogUp balances, then key r genuinely does not exist in the
SSMC commitment for (t,c).

Proof: SSMC gap query row receives via C2 ReadNonMembership.
The gap query row sits between entries key_i and key_{i+1} with
key_i < r < key_{i+1} (strict inequalities constrained by AIR).
SSMC entries are strictly sorted. Therefore no entry with key=r exists.
The hash chain excludes gap query rows. Com_old is correctly computed.
ColumnMeta verifies against oldRoot. QED.

### 6.4 Read Soundness (Null, Empty)

Claim: if Execution claims `Read(t,c,r) = null` from an empty column,
and the LogUp balances, then the column genuinely has no entries.

Proof: ColumnMeta receives via C7 EmptyColRead.
ColumnMeta's `is_empty_old=1` is verified by: Com_old = Com_empty =
Poseidon(0x00||t||c||padding), which is verified via Poseidon bus.
Com_empty matches the empty commitment. ColumnMeta verifies against
oldRoot via SMT path. QED.

### 6.5 Write Soundness

Claim: every Execution Write is reflected in the new state.

Proof: Execution sends Write entries via C4 WriteSet to Merge.
LogUp ensures multiset equality: every Write in Execution appears
in Merge. Merge's 3-way merge produces NewList. Merge's hash chain
computes Com_new. ColumnMeta receives Com_new and verifies against
newRoot via SMT path. QED.

### 6.6 Completeness (No Phantom Reads/Writes)

Claim: the prover cannot introduce phantom reads or writes.

For reads: every C1/C2/C7 send from Execution has multiplicity gated
by `is_real * op_read * [condition]`. These are boolean columns
constrained by the AIR. The prover cannot add extra sends without
adding execution rows (which must satisfy opcode one-hot and other
constraints).

For writes: every C4 send has multiplicity `is_real * op_write`.
Same argument.

For SSMC: every receive has multiplicity gated by real SSMC rows.
The prover cannot add phantom SSMC entries without affecting the
hash chain (and therefore Com_old), which would fail verification
against oldRoot.

## 7. Inter-Tx Conflict Handling

### 7.1 Option A: Parallel Batch Semantics

Redefine batch semantics:

```
for each tx_i in batch:
    WriteSet_i = execute(BaseState, tx_i)    // all read from base

WriteSet_batch = coalesce(WriteSet_0, ..., WriteSet_{N-1})
    // conflict resolution: last tx_index wins

newState = apply(BaseState, WriteSet_batch)
```

**Pros**:
- All txs are independent (embarrassingly parallel)
- No ordering argument needed
- SortedMem completely eliminated
- Coalescing handled by Merge (already has sorted keys + source encoding)

**Cons**:
- Different semantics from sequential model
- tx_1 cannot observe tx_0's writes (breaks some applications)
- Conflict resolution policy must be defined and enforced

**Coalescing in Merge**: If two txs write to the same key, both writes
appear in Merge's write-set input. Merge must select the one with
the highest tx_index. This requires adding `tx_index` to the C4 tuple
and a "latest write wins" constraint in Merge. This is a small change.

### 7.2 Option B: Conflict-Free Batches (Enforced)

```
for each tx_i in batch:
    WriteSet_i = execute(S_i, tx_i)
    S_{i+1} = apply(S_i, WriteSet_i)
    // sequential, but guaranteed no conflicts
```

If the sequencer guarantees no two txs access the same `(t,c,r)`,
then `execute(S_i, tx_i) = execute(BaseState, tx_i)` for all i.
(Because no prior tx modified any key that tx_i reads.)

**Enforcement**: The sequencer runs a conflict check before batching.
This is a pre-processing step, not part of the proof. The proof
assumes non-conflict; if violated, the proof is still valid but
the state transition is semantically wrong.

**Optional in-proof enforcement**: Add a lightweight "conflict check"
table that verifies all `(t,c,r)` across all txs are distinct.
This is O(A) rows with sorting, but much cheaper than full SortedMem
(only key identity columns, no value/memory columns).

### 7.3 Option C: Hybrid — SortedMem-Lite for Hot Keys

Keep a reduced SortedMem only for keys accessed by multiple txs.
Route single-tx keys directly to SSMC/Merge.

**KeyRoute classification** (already designed in proof-optimization-architecture.md):
- `SingleTxKey` -> direct to SSMC/Merge (no SortedMem)
- `MultiTxKey` -> SortedMem-Lite (reduced columns)

This is the most complex option but handles all cases.

### 7.4 Recommendation

**Option B (Conflict-Free Batches)** is the recommended starting point:

1. Simplest architecture (no SortedMem at all)
2. Most common in practice (sequencers already avoid conflicts for performance)
3. Can be upgraded to Option A or C later if needed
4. Optional in-proof conflict check for extra safety

## 8. Migration Path

### Phase 0: Validation
- Formal verification of soundness arguments (§6)
- Benchmark column savings vs. added SSMC complexity
- Review with protocol team: is non-conflicting batch assumption acceptable?

### Phase 1: SSMC Gap Queries
- Add is_gap_query flag and gap query rows to SSMC
- Add ReadNonMembership bus
- Unit tests for gap query constraints
- No other chip changes yet

### Phase 2: Direct Read/Write Buses
- Add ReadMembership bus (Execution -> SSMC)
- Modify WriteSet bus sender (Execution instead of SortedMem)
- Add EmptyColRead bus (Execution -> ColumnMeta)
- Add is_empty_col witness to ExecutionChip

### Phase 3: Remove SortedMem
- Remove GlobalSortedMem chip
- Remove Memory bus C1
- Remove SortedMemMeta bus C7
- Remove has_sorted_mem from ColumnMeta
- Update all tests

### Phase 4: (Optional) Conflict Check
- Add lightweight conflict-check table
- Verifies all accessed (t,c,r) across all txs are distinct

## 9. Open Questions

1. **Non-conflicting batch assumption**: Is this acceptable at the
   protocol level? What applications require inter-tx read-after-write?

2. **Empty column edge case**: Is the EmptyColRead bus the right
   mechanism, or should we handle empty columns differently?

3. **SSMC gap query efficiency**: Adding gap query rows increases SSMC
   row count. Is this offset by the SortedMem elimination?

4. **Write-set coalescing**: If we later support conflicting batches
   (Option A), how does Merge handle multiple writes to the same key?

5. **Existing M11 design**: The M11 design assumes SortedMem exists
   (gap witnesses sourced from SortedMem). If we eliminate SortedMem,
   M11 must be redesigned. Should we redesign M11 before implementing it?

## 10. Summary

| Aspect | Current | Proposed |
|--------|---------|----------|
| Chips | 7 | 6 (-SortedMem) |
| Total columns | ~655 | ~595 (-60, -9%) |
| Buses | 9 | 9 (different topology) |
| SortedMem rows | O(A) | 0 |
| SSMC extra rows | 0 | O(G) gap queries |
| Batch semantics | Sequential | Parallel (or conflict-free sequential) |
| Inter-tx conflicts | Handled by SortedMem | Forbidden or protocol-resolved |
| Read verification | Exec->SortedMem->SSMC | Exec->SSMC (direct) |
| Write delivery | Exec->SortedMem->Merge | Exec->Merge (direct) |
| Non-membership | SortedMem gap witness | SSMC gap query rows |

**Key insight**: Tabula's SSA + NF design was intended to eliminate
intra-tx memory arguments. The logical continuation is to eliminate
the inter-tx memory argument as well, by pushing conflict resolution
to the protocol layer. This yields a simpler, smaller, more direct
proof architecture where ExecutionChip talks directly to SSMC and Merge.
